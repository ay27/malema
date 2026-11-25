use crate::i18n;
use crate::sources::{Source, Turn};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use time::OffsetDateTime;

const DAY: i64 = 86_400;
const EPOCH_MONDAY: i64 = 4;
const MAX_TURNS: u32 = 5_000;
/// Model names travel verbatim, so the wire format needs a shape both ends can
/// check without knowing the model. Keep this in sync with `cloudflare/src/index.ts`.
const MAX_NAME: usize = 64;
/// Messages whose tool never logged a model are still counted. The board folds
/// this bucket into "others" instead of hiding those tools entirely.
const UNKNOWN_MODEL: &str = "unknown";
pub const LEADERBOARD_URL: &str = "https://malema.app";
pub const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    protocol: u32,
    ruleset: String,
    language_version: String,
}

fn contract() -> &'static Contract {
    static CONTRACT: OnceLock<Contract> = OnceLock::new();
    CONTRACT.get_or_init(|| {
        serde_json::from_str(include_str!("../leaderboard-contract.json"))
            .expect("valid leaderboard contract")
    })
}

pub fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn day(timestamp: i64) -> i64 {
    timestamp.div_euclid(DAY)
}

fn bucket(day: i64, width: i64) -> i64 {
    day - (day - EPOCH_MONDAY).rem_euclid(width)
}

fn open(week: i64, now: i64) -> bool {
    week <= day(now) && day(now) < bucket(week, 28) + 35
}

fn harness(source: Source) -> &'static str {
    match source {
        Source::Codex => "codex",
        Source::Claude => "claude",
        Source::Dsh => "deepseek",
        Source::WorkBuddy => "workbuddy",
        Source::Cursor => "cursor",
        Source::Pi => "pi",
    }
}

/// Keep the reported name as the tool spelled it. Normalising to lowercase only
/// merges case variants; it never maps an unknown model onto a known one, so a
/// model released today shows up on the board today instead of disappearing.
/// A message with no usable name is filed under `UNKNOWN_MODEL` rather than
/// dropped, so a tool that does not log its model still reaches the board.
fn model(raw: Option<&str>) -> String {
    let name: String = raw
        .unwrap_or_default()
        .trim()
        .chars()
        .filter(|&c| !c.is_control())
        .map(|c| c.to_ascii_lowercase())
        .take(MAX_NAME)
        .collect();
    let name = name.trim_end();
    if name.is_empty() {
        UNKNOWN_MODEL.to_owned()
    } else {
        name.to_owned()
    }
}

fn language(text: &str) -> Option<&'static str> {
    let mut han = 0_usize;
    let mut latin = 0_usize;
    let mut latin_words = 0_usize;
    let mut other = 0_usize;
    let mut fenced = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced || line.starts_with('>') {
            continue;
        }
        for word in line.split_whitespace() {
            if word.contains("://") || word.starts_with('/') || word.starts_with('`') {
                continue;
            }
            if word
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .chars()
                .all(|c| c.is_ascii_alphabetic())
                && word.chars().any(|c| c.is_ascii_alphabetic())
            {
                latin_words += 1;
            }
            for c in word.chars() {
                if matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
                {
                    han += 1;
                } else if c.is_ascii_alphabetic() {
                    latin += 1;
                } else if c.is_alphabetic() {
                    other += 1;
                }
            }
        }
    }
    if other > 0 {
        return None;
    }
    if han >= 2 && han.saturating_mul(3) >= latin {
        Some("zh")
    } else if han == 0 && latin_words >= 2 && latin >= 5 {
        Some("en")
    } else {
        None
    }
}

#[derive(Clone, Debug, Default)]
pub struct Weeks(BTreeMap<i64, BTreeMap<(&'static str, String, &'static str), (u32, u32)>>);

impl Weeks {
    fn add(&mut self, week: i64, key: (&'static str, String, &'static str), hit: bool) {
        let (turns, hits) = self.0.entry(week).or_default().entry(key).or_default();
        if *turns < MAX_TURNS {
            *turns += 1;
            *hits += u32::from(hit);
        }
    }

    fn cells(&self, week: i64) -> Vec<Cell> {
        self.0
            .get(&week)
            .into_iter()
            .flatten()
            .map(|((harness, model, language), &(turns, hits))| Cell {
                harness: (*harness).to_owned(),
                model: model.clone(),
                language: (*language).to_owned(),
                turns,
                hits,
            })
            .collect()
    }

    #[cfg(test)]
    fn counts(&self) -> (u64, u64) {
        self.0
            .values()
            .flat_map(|week| week.values())
            .fold((0, 0), |(turns, hits), &(n, h)| {
                (turns + u64::from(n), hits + u64::from(h))
            })
    }
}

pub struct Collection {
    pub live: Weeks,
    pub preview: Weeks,
    pub preview_since: i64,
    since: Option<i64>,
    now: i64,
}

impl Collection {
    pub fn new(now: i64, since: Option<i64>) -> Self {
        Self {
            live: Weeks::default(),
            preview: Weeks::default(),
            preview_since: (day(now) - 6) * DAY,
            since,
            now,
        }
    }

    pub fn collect(&mut self, source: Source, turn: &Turn, hit: bool) {
        let Some(timestamp) = turn.timestamp.filter(|&ts| ts <= self.now) else {
            return;
        };
        let week = bucket(day(timestamp), 7);
        let live = self.since.is_some_and(|since| timestamp >= since);
        let preview = timestamp >= self.preview_since;
        if !open(week, self.now) || (!live && !preview) {
            return;
        }
        let Some(language) = language(&turn.text) else {
            return;
        };
        let key = (harness(source), model(turn.model.as_deref()), language);
        if live {
            self.live.add(week, key.clone(), hit);
        }
        if preview {
            self.preview.add(week, key, hit);
        }
    }

    pub fn since(&self) -> Option<i64> {
        self.since
    }

    pub fn start(&mut self, since: i64, history: bool) {
        self.since = Some(since);
        self.live = if history {
            self.preview.clone()
        } else {
            Weeks::default()
        };
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cell {
    harness: String,
    model: String,
    language: String,
    turns: u32,
    hits: u32,
}

#[derive(Clone, Deserialize, Serialize)]
struct Slot {
    revision: u32,
    cells: Vec<Cell>,
    acked: bool,
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct State {
    since: Option<i64>,
    tokens: BTreeMap<i64, String>,
    slots: BTreeMap<i64, Slot>,
    last_sync: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    protocol: u32,
    ruleset: String,
    language_version: String,
    season: i64,
    week: i64,
    contributor: String,
    revision: u32,
    cells: Vec<Cell>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    Enable { history: bool },
    Disable,
    Status,
    Open,
}

#[derive(Serialize)]
pub struct Status {
    enabled: bool,
    sending: bool,
    ready: bool,
    message: String,
}

pub struct Controller {
    path: PathBuf,
    state: State,
    pub message: String,
}

fn random_token() -> io::Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

impl Controller {
    pub fn load(home: &Path) -> Self {
        let path = home
            .join("Library/Application Support/Malema")
            .join("contributions.json");
        let mut message = String::new();
        let mut settings_missing = false;
        let state = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
                message = i18n::pick(
                    "贡献设置无法读取，请重新确认后开启。",
                    "the contribution settings could not be read; confirm them again to enable it.",
                )
                .to_owned();
                State::default()
            }),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                settings_missing = true;
                State::default()
            }
            Err(_) => {
                message = i18n::pick(
                    "贡献设置暂时不可读。",
                    "the contribution settings are temporarily unreadable.",
                )
                .to_owned();
                State::default()
            }
        };
        let mut controller = Self {
            path,
            state,
            message,
        };
        if settings_missing && controller.set_first_launch_default(home).is_err() {
            controller.message = i18n::pick(
                "无法保存上报设置。",
                "could not save the contribution setting.",
            )
            .to_owned();
        }
        controller
    }

    fn set_first_launch_default(&mut self, home: &Path) -> io::Result<()> {
        let directory = self.path.parent().expect("contribution settings parent");
        // Older releases used an implicit off setting until the user opted in.
        if directory.try_exists()? || home.join("Library/Caches/Malema").try_exists()? {
            return Ok(());
        }
        fs::create_dir_all(directory.parent().expect("application support directory"))?;
        match fs::create_dir(directory) {
            Ok(()) => self.enable(now()),
            // A second app instance may have initialized settings since the check.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn since(&self) -> Option<i64> {
        self.state.since
    }

    fn save(&mut self, state: State) -> io::Result<()> {
        let directory = self.path.parent().expect("contribution settings parent");
        fs::create_dir_all(directory)?;
        crate::private_write(&self.path, &serde_json::to_vec(&state)?)?;
        self.state = state;
        Ok(())
    }

    pub fn enable(&mut self, since: i64) -> io::Result<()> {
        let mut state = self.state.clone();
        state.since = Some(since);
        self.save(state)?;
        self.message.clear();
        Ok(())
    }

    pub fn disable(&mut self) -> io::Result<()> {
        let mut state = self.state.clone();
        state.since = None;
        self.save(state)?;
        self.message.clear();
        Ok(())
    }

    pub fn status(&self, ready: bool, sending: bool) -> Status {
        Status {
            enabled: self.state.since.is_some(),
            sending,
            ready,
            message: self.message.clone(),
        }
    }

    pub fn prepare(&mut self, counts: &Weeks, now: i64) -> io::Result<Vec<Batch>> {
        if self.state.since.is_none() {
            return Ok(Vec::new());
        }
        let mut state = self.state.clone();
        let latest_season = bucket(day(now), 28);
        state
            .tokens
            .retain(|&season, _| season >= latest_season - 28);
        state.slots.retain(|&week, _| open(week, now));
        let mut changed = state.tokens.len() != self.state.tokens.len()
            || state.slots.len() != self.state.slots.len();
        for &week in counts.0.keys().filter(|&&week| open(week, now)) {
            let cells = counts.cells(week);
            let revision = match state.slots.get(&week) {
                Some(slot) if slot.cells == cells => continue,
                Some(slot) => slot.revision + 1,
                None => 1,
            };
            state.slots.insert(
                week,
                Slot {
                    revision,
                    cells,
                    acked: false,
                },
            );
            changed = true;
        }
        let mut batches = Vec::new();
        let spec = contract();
        if spec.ruleset != crate::detect::RULESET {
            return Err(io::Error::other(
                "leaderboard ruleset differs from local detection",
            ));
        }
        for (&week, slot) in state
            .slots
            .iter()
            .filter(|(week, slot)| !slot.acked && counts.0.contains_key(week))
        {
            let season = bucket(week, 28);
            if let std::collections::btree_map::Entry::Vacant(entry) = state.tokens.entry(season) {
                entry.insert(random_token()?);
                changed = true;
            }
            batches.push(Batch {
                protocol: spec.protocol,
                ruleset: spec.ruleset.clone(),
                language_version: spec.language_version.clone(),
                season,
                week,
                contributor: state.tokens[&season].clone(),
                revision: slot.revision,
                cells: slot.cells.clone(),
            });
        }
        if changed {
            self.save(state)?;
        }
        Ok(batches)
    }

    pub fn finish(&mut self, result: SyncResult) -> io::Result<()> {
        let mut state = self.state.clone();
        for (week, revision) in &result.acked {
            if let Some(slot) = state
                .slots
                .get_mut(week)
                .filter(|slot| slot.revision == *revision)
            {
                slot.acked = true;
            }
        }
        if !result.acked.is_empty() {
            state.last_sync = Some(now());
            self.save(state)?;
        }
        self.message = if self.since().is_none() {
            String::new()
        } else {
            result.error.unwrap_or_default()
        };
        Ok(())
    }
}

pub struct SyncResult {
    acked: Vec<(i64, u32)>,
    error: Option<String>,
}

pub fn sync(batches: Vec<Batch>, cancelled: &AtomicBool) -> SyncResult {
    let mut result = SyncResult {
        acked: Vec::new(),
        error: None,
    };
    for batch in batches {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        if let Err(error) = post(&batch) {
            result.error = Some(error);
            break;
        }
        result.acked.push((batch.week, batch.revision));
    }
    result
}

fn post(batch: &Batch) -> Result<(), String> {
    let url = format!("{LEADERBOARD_URL}/api/v1/contributions");
    let mut command = Command::new("/usr/bin/curl");
    command.args([
        "--disable",
        "--silent",
        "--max-time",
        "8",
        "--request",
        "POST",
        "--write-out",
        "\n%{http_code}",
    ]);
    let mut config = format!(
        "url = {}\nheader = \"Content-Type: application/json\"\n",
        quoted(&url)
    );
    config.push_str(&format!(
        "data-binary = {}\n",
        quoted(&serde_json::to_string(batch).expect("batch serialization"))
    ));
    let fallback = || {
        i18n::pick(
            "暂时无法连接榜单服务。应用下次会重试。",
            "cannot reach the leaderboard service right now; the app will retry later.",
        )
        .to_owned()
    };
    let mut child = command
        .args(["--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| fallback())?;
    let wrote = child
        .stdin
        .take()
        .expect("curl stdin")
        .write_all(config.as_bytes());
    let output = child.wait_with_output().map_err(|_| fallback())?;
    if wrote.is_err() || !output.status.success() {
        return Err(fallback());
    }
    let response = String::from_utf8_lossy(&output.stdout);
    let (body, status) = response.rsplit_once('\n').ok_or_else(fallback)?;
    let body: serde_json::Value = serde_json::from_str(body).map_err(|_| fallback())?;
    if status == "200"
        && matches!(body["status"].as_str(), Some("accepted" | "duplicate"))
        && body["revision"].as_u64() == Some(u64::from(batch.revision))
    {
        return Ok(());
    }
    Err(match status {
        "409" => i18n::pick(
            "这周的贡献版本有冲突，请检查是否同时运行了两个副本。",
            "this week's contribution revision conflicts; check whether two copies are running.",
        )
        .to_owned(),
        "400" => i18n::pick(
            "榜单拒绝了这批计数。请确认客户端和服务端版本一致。",
            "the leaderboard rejected these counts; make sure the client and server versions match.",
        )
        .to_owned(),
        "429" => i18n::pick(
            "上报暂时限流。应用下次会重试。",
            "uploads are rate limited right now; the app will retry later.",
        )
        .to_owned(),
        _ => fallback(),
    })
}

fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporting_defaults_on_for_a_new_install_and_remembers_opt_out() {
        let home = std::env::temp_dir().join(format!("malema-test-{}", random_token().unwrap()));
        let before = now();
        let mut controller = Controller::load(&home);
        let since = controller.since().unwrap();
        assert!(since >= before && since <= now());
        assert_eq!(Controller::load(&home).since(), Some(since));
        controller.disable().unwrap();
        assert_eq!(Controller::load(&home).since(), None);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn upgrade_preserves_the_previous_default_off_setting() {
        let home = std::env::temp_dir().join(format!("malema-test-{}", random_token().unwrap()));
        fs::create_dir_all(home.join("Library/Caches/Malema")).unwrap();
        assert_eq!(Controller::load(&home).since(), None);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn only_eligible_utc_messages_enter_the_contribution() {
        let utc = (EPOCH_MONDAY + 28 * 700 + 1) * DAY;
        let mut counts = Collection::new(utc, Some(utc - DAY));
        let turn = |timestamp, model: &str, text: &str| Turn {
            inferred_day: None,
            model: Some(model.to_owned()),
            harness_version: None,
            date: Some("1999-01-01".to_owned()),
            timestamp,
            text: text.to_owned(),
            session: "private-session".to_owned(),
            line: 11,
        };
        for (time, model, message) in [
            (Some(utc - 1), "gpt-5.6-luna", "你能看看这段代码吗"),
            (Some(utc - 2), "gpt-5.6-luna", "You are completely useless"),
            (
                Some(utc - 3),
                "secret-private-name",
                "Please check this code",
            ),
            (None, "gpt-5.6-luna", "Please check this code"),
            (Some(utc + 1), "gpt-5.6-luna", "Please check this code"),
        ] {
            counts.collect(
                Source::Codex,
                &turn(time, model, message),
                time == Some(utc - 2),
            );
        }
        let week = bucket(day(utc), 7);
        let payload = serde_json::to_value(counts.live.cells(week)).unwrap();
        assert_eq!(
            payload,
            serde_json::json!([
                {"harness":"codex","model":"gpt-5.6-luna","language":"en","turns":1,"hits":1},
                {"harness":"codex","model":"gpt-5.6-luna","language":"zh","turns":1,"hits":0},
                {"harness":"codex","model":"secret-private-name","language":"en","turns":1,"hits":0}
            ])
        );
        assert_eq!(counts.preview.counts(), (3, 1));
        counts.start(utc, false);
        assert_eq!(counts.live.counts(), (0, 0));
    }

    #[test]
    fn unreleased_model_names_survive_the_contract() {
        let utc = (EPOCH_MONDAY + 28 * 700 + 1) * DAY;
        let mut counts = Collection::new(utc, Some(utc - DAY));
        for name in [
            Some("gpt-7-experimental"),
            Some("Some Vendor/Model:2026"),
            Some("  MiXeD  "),
            None,
        ] {
            counts.collect(
                Source::Claude,
                &Turn {
                    inferred_day: None,
                    model: name.map(str::to_owned),
                    harness_version: None,
                    date: None,
                    timestamp: Some(utc - 1),
                    text: "Please look at this code".to_owned(),
                    session: "s".to_owned(),
                    line: 1,
                },
                false,
            );
        }
        let week = bucket(day(utc), 7);
        let models: Vec<String> = counts
            .live
            .cells(week)
            .into_iter()
            .map(|c| c.model)
            .collect();
        assert_eq!(
            models,
            vec![
                "gpt-7-experimental".to_owned(),
                "mixed".to_owned(),
                "some vendor/model:2026".to_owned(),
                UNKNOWN_MODEL.to_owned()
            ]
        );
    }

    #[test]
    fn sample_limit_applies_to_the_numerator_and_denominator() {
        let mut weeks = Weeks::default();
        let key = ("codex", "gpt-5.6-luna".to_owned(), "zh");
        for _ in 0..MAX_TURNS {
            weeks.add(4, key.clone(), false);
        }
        weeks.add(4, key, true);
        assert_eq!(weeks.counts(), (u64::from(MAX_TURNS), 0));
    }
}
