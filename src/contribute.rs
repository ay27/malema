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

/// Model names travel verbatim, so the wire format needs a shape both ends can
/// check without knowing the model. Keep this in sync with `cloudflare/src/index.ts`.
const MAX_NAME: usize = 64;
/// Messages whose tool never logged a model are still counted. The board folds
/// this bucket into "others" instead of hiding those tools entirely.
const UNKNOWN_MODEL: &str = "unknown";
pub const LEADERBOARD_URL: &str = "https://malema.app";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
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
pub struct Counts(BTreeMap<(&'static str, String, &'static str), (u64, u64)>);

impl Counts {
    fn add(&mut self, key: (&'static str, String, &'static str), hit: bool) {
        let (turns, hits) = self.0.entry(key).or_default();
        *turns += 1;
        *hits += u64::from(hit);
    }

    fn cells(&self) -> Vec<Cell> {
        self.0
            .iter()
            .map(|((harness, model, language), &(turns, hits))| Cell {
                harness: (*harness).to_owned(),
                model: model.clone(),
                language: (*language).to_owned(),
                turns,
                hits,
            })
            .collect()
    }
}

pub struct Collection {
    pub live: Counts,
    since: Option<i64>,
}

impl Collection {
    pub fn new(_now: i64, since: Option<i64>) -> Self {
        Self {
            live: Counts::default(),
            since,
        }
    }

    pub fn collect(&mut self, source: Source, turn: &Turn, hit: bool) {
        self.live.add(
            (
                harness(source),
                model(turn.model.as_deref()),
                language(&turn.text).unwrap_or("other"),
            ),
            hit,
        );
    }

    pub fn since(&self) -> Option<i64> {
        self.since
    }
    pub fn start(&mut self, since: i64) {
        self.since = Some(since);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Cell {
    harness: String,
    model: String,
    language: String,
    turns: u64,
    hits: u64,
}

#[derive(Clone, Deserialize, Serialize)]
struct Slot {
    revision: u32,
    cells: Vec<Cell>,
    acked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    previous_confirmation: Option<ConfirmedCounts>,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
struct ConfirmedCounts {
    turns: u64,
    hits: u64,
}

impl Slot {
    fn confirmed_counts(&self) -> Option<ConfirmedCounts> {
        if self.acked {
            Some(ConfirmedCounts {
                turns: self.cells.iter().map(|cell| u64::from(cell.turns)).sum(),
                hits: self.cells.iter().map(|cell| u64::from(cell.hits)).sum(),
            })
        } else {
            self.previous_confirmation
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize)]
struct State {
    since: Option<i64>,
    tokens: BTreeMap<i64, String>,
    slots: BTreeMap<i64, Slot>,
    last_sync: Option<i64>,
    #[serde(default)]
    contributor: Option<String>,
    #[serde(default)]
    snapshot: Option<Slot>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch {
    protocol: u32,
    ruleset: String,
    language_version: String,
    replaces: Vec<String>,
    contributor: String,
    revision: u32,
    cells: Vec<Cell>,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Request {
    Enable,
    Disable,
    Status,
    Open,
}

#[derive(Serialize)]
pub struct Status {
    enabled: bool,
    sending: bool,
    ready: bool,
    confirmed: Option<ConfirmedCounts>,
    last_sync: Option<i64>,
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
        let confirmed = self
            .state
            .snapshot
            .as_ref()
            .and_then(Slot::confirmed_counts)
            .or_else(|| {
                let cells: Vec<_> = self
                    .state
                    .slots
                    .values()
                    .filter_map(Slot::confirmed_counts)
                    .collect();
                (!cells.is_empty()).then(|| ConfirmedCounts {
                    turns: cells.iter().map(|c| c.turns).sum(),
                    hits: cells.iter().map(|c| c.hits).sum(),
                })
            });
        Status {
            enabled: self.state.since.is_some(),
            sending,
            ready,
            confirmed,
            last_sync: self.state.last_sync,
            message: self.message.clone(),
        }
    }

    pub fn prepare(&mut self, counts: &Counts, _now: i64) -> io::Result<Vec<Batch>> {
        if self.state.since.is_none() {
            return Ok(Vec::new());
        }
        let mut state = self.state.clone();
        let cells = counts.cells();
        if state
            .snapshot
            .as_ref()
            .is_none_or(|slot| slot.cells != cells)
        {
            let revision = state.snapshot.as_ref().map_or(1, |slot| slot.revision + 1);
            let previous_confirmation = state.snapshot.as_ref().and_then(Slot::confirmed_counts);
            state.snapshot = Some(Slot {
                revision,
                cells,
                acked: false,
                previous_confirmation,
            });
        }
        if state.contributor.is_none() {
            state.contributor = Some(random_token()?);
        }
        let slot = state.snapshot.as_ref().expect("prepared snapshot");
        let batch = (!slot.acked).then(|| Batch {
            protocol: 3,
            ruleset: contract().ruleset.clone(),
            language_version: contract().language_version.clone(),
            contributor: state.contributor.clone().expect("installation token"),
            replaces: state.tokens.values().cloned().collect(),
            revision: slot.revision,
            cells: slot.cells.clone(),
        });
        self.save(state)?;
        Ok(batch.into_iter().collect())
    }

    pub fn finish(&mut self, result: SyncResult) -> io::Result<()> {
        let mut state = self.state.clone();
        if let Some(slot) = state.snapshot.as_mut()
            && result.acked.contains(&slot.revision)
        {
            slot.acked = true;
            slot.previous_confirmation = None;
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
    acked: Vec<u32>,
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
        result.acked.push(batch.revision);
    }
    result
}

fn post(batch: &Batch) -> Result<(), String> {
    let url = format!("{LEADERBOARD_URL}/api/v2/contributions");
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
            "统计版本有冲突，请检查是否同时运行了两个副本。",
            "the contribution revision conflicts; check whether two copies are running.",
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
    fn full_snapshot_includes_old_unknown_dates_and_other_languages() {
        let home = std::env::temp_dir().join(format!("malema-full-{}", random_token().unwrap()));
        let mut controller = Controller::load(&home);
        let now = now();
        let mut counts = Collection::new(now, controller.since());
        for timestamp in [Some(1), None, Some(now)] {
            counts.collect(
                Source::Codex,
                &Turn {
                    inferred_day: None,
                    model: None,
                    harness_version: None,
                    date: None,
                    timestamp,
                    text: "啊".to_owned(),
                    session: "test".to_owned(),
                    line: 1,
                },
                true,
            );
        }
        let batch = controller
            .prepare(&counts.live, now)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(batch.cells[0].turns, 3);
        assert_eq!(batch.cells[0].hits, 3);
        assert_eq!(batch.cells[0].language, "other");
        controller
            .finish(SyncResult {
                acked: vec![batch.revision],
                error: None,
            })
            .unwrap();
        assert!(controller.prepare(&counts.live, now).unwrap().is_empty());
        let mut reopened = Controller::load(&home);
        assert_eq!(reopened.status(true, false).confirmed.unwrap().hits, 3);
        reopened.disable().unwrap();
        assert!(reopened.prepare(&counts.live, now).unwrap().is_empty());
        assert!(Controller::load(&home).since().is_none());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn older_receipt_does_not_confirm_a_newer_snapshot_and_legacy_tokens_survive() {
        let home = std::env::temp_dir().join(format!("malema-full-{}", random_token().unwrap()));
        let mut controller = Controller::load(&home);
        controller
            .state
            .tokens
            .insert(4, "01234567890123456789012345678901".to_owned());
        let mut counts = Counts::default();
        counts.add(("codex", "test".to_owned(), "zh"), true);
        let first = controller.prepare(&counts, now()).unwrap().pop().unwrap();
        counts.add(("codex", "test".to_owned(), "zh"), true);
        let second = controller.prepare(&counts, now()).unwrap().pop().unwrap();
        assert_eq!(second.revision, first.revision + 1);
        assert_eq!(second.replaces.len(), 1);
        controller
            .finish(SyncResult {
                acked: vec![first.revision],
                error: None,
            })
            .unwrap();
        let mut reopened = Controller::load(&home);
        assert_eq!(
            reopened.prepare(&counts, now()).unwrap()[0].revision,
            second.revision
        );
        reopened
            .finish(SyncResult {
                acked: vec![second.revision],
                error: None,
            })
            .unwrap();
        counts = Counts::default();
        let empty = reopened.prepare(&counts, now()).unwrap().pop().unwrap();
        assert!(empty.cells.is_empty());
        assert_eq!(empty.contributor, first.contributor);
        assert_eq!(reopened.status(true, false).confirmed.unwrap().hits, 2);
        fs::remove_dir_all(home).unwrap();
    }
}
