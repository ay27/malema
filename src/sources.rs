use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::SystemTime;
use walkdir::WalkDir;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Source {
    Codex,
    Claude,
    Dsh,
    WorkBuddy,
    Cursor,
    Pi,
}

impl Source {
    pub const ALL: [Self; 6] = [
        Self::Codex,
        Self::Claude,
        Self::Dsh,
        Self::WorkBuddy,
        Self::Cursor,
        Self::Pi,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude Code",
            Self::Dsh => "DeepSeek Harness",
            Self::WorkBuddy => "WorkBuddy",
            Self::Cursor => "Cursor",
            Self::Pi => "pi",
        }
    }

    fn roots(self, home: &Path, use_env: bool) -> Vec<PathBuf> {
        let root = |name, default: PathBuf| {
            if use_env {
                std::env::var_os(name)
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .unwrap_or(default)
            } else {
                default
            }
        };
        match self {
            Self::Codex => {
                let base = root("CODEX_HOME", home.join(".codex"));
                vec![base.join("sessions"), base.join("archived_sessions")]
            }
            Self::Claude => vec![root("CLAUDE_CONFIG_DIR", home.join(".claude")).join("projects")],
            Self::Dsh => vec![home.join(".dsh/sessions")],
            Self::WorkBuddy => vec![home.join(".workbuddy/projects")],
            Self::Cursor => vec![home.join(".cursor/projects")],
            Self::Pi => vec![root(
                "PI_CODING_AGENT_SESSION_DIR",
                root("PI_CODING_AGENT_DIR", home.join(".pi/agent")).join("sessions"),
            )],
        }
    }

    fn is_file(self, root: &Path, path: &Path) -> bool {
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let components: Vec<_> = relative.iter().collect();
        if self == Self::Dsh {
            let versions = [
                "session.v4.jsonl.zstd",
                "session.v4.jsonl",
                "session.jsonl.zstd",
                "session.jsonl",
            ];
            return versions
                .iter()
                .position(|v| *v == name)
                .is_some_and(|index| {
                    versions[..index]
                        .iter()
                        .all(|preferred| !path.with_file_name(preferred).is_file())
                });
        }
        if self == Self::Codex && codex_packed(name) {
            return true;
        }
        if !name.ends_with(".jsonl") || components.iter().any(|part| *part == "subagents") {
            return false;
        }
        match self {
            Self::Claude | Self::WorkBuddy => components.len() == 2,
            Self::Cursor => {
                matches!(components.as_slice(), [_, dir, _, _] | [_, dir, _] if *dir == "agent-transcripts")
            }
            _ => true,
        }
    }
}

fn codex_packed(name: &str) -> bool {
    name.ends_with(".jsonl.zst") || name.ends_with(".jsonl.zstd")
}

fn codex_filename(name: &str) -> &str {
    name.strip_suffix(".zst")
        .or_else(|| name.strip_suffix(".zstd"))
        .unwrap_or(name)
}

#[derive(Debug)]
pub struct Turn {
    pub inferred_day: Option<InferredDay>,
    pub model: Option<String>,
    pub harness_version: Option<String>,
    pub date: Option<String>,
    pub timestamp: Option<i64>,
    pub text: String,
    pub session: String,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InferredDay {
    SameDayNeighbors,
    FileMetadata,
}

#[derive(Serialize)]
pub struct SourceStats {
    pub source: Source,
    pub found: bool,
    pub files: usize,
    pub compressed: usize,
    pub archived: usize,
    pub missing_days: usize,
    pub inferred_neighbors: usize,
    pub inferred_file_days: usize,
    pub history_turns: usize,
    pub duplicates: usize,
    pub ignored: usize,
    pub errors: usize,
    pub malformed: usize,
    pub turns: usize,
    pub attributed: usize,
    pub versioned_turns: usize,
    pub hits: usize,
}

fn get<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer)?.as_str().filter(|s| !s.is_empty())
}

fn first<'a>(value: &'a Value, pointers: &[&str]) -> Option<&'a str> {
    pointers.iter().find_map(|p| get(value, p))
}

fn any_text(content: &Value, types: &[&str]) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    content
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| {
            item["type"]
                .as_str()
                .is_some_and(|kind| types.contains(&kind))
        })
        .filter_map(|item| get(item, "/text"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn clean_user(text: &str) -> Option<&str> {
    let mut text = text.trim();
    loop {
        let before = text;
        if let Some(rest) = text.strip_prefix("<timestamp>") {
            text = rest.split_once("</timestamp>")?.1.trim();
        }
        for tag in [
            "system-reminder",
            "environment_context",
            "openviking-context",
            "ide_context",
        ] {
            let opening = format!("<{tag}>");
            let closing = format!("</{tag}>");
            if let Some(rest) = text.strip_prefix(&opening) {
                text = rest.split_once(&closing)?.1.trim();
            }
            if let Some(rest) = text.strip_suffix(&closing) {
                text = rest.rsplit_once(&opening)?.0.trim();
            }
        }
        if text == before {
            break;
        }
    }
    if let Some(rest) = text.strip_prefix("<user_query>") {
        text = rest.split_once("</user_query>")?.0.trim();
    }
    let lower = text.chars().take(240).collect::<String>().to_lowercase();
    let wrappers = [
        "# agents.md instructions",
        "you are a memory relevance compressor",
        "this session is being continued",
        "side conversation boundary",
        "the following is the codex agent history",
        "base directory for this skill",
        "re-invocation of /",
        "skill /",
        "update config skill",
        "【角色】",
    ];
    if text.is_empty()
        || text.starts_with('<')
        || text.starts_with('/')
        || wrappers.iter().any(|prefix| lower.starts_with(prefix))
    {
        return None;
    }
    Some(text)
}

fn timestamp(value: &Value) -> Option<i64> {
    if let Some(input) = value.as_str() {
        return time::OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
            .ok()
            .map(|instant| instant.unix_timestamp());
    }
    let mut seconds = value.as_i64()?;
    if seconds >= 10_000_000_000 {
        seconds /= 1000;
    }
    (946_684_800..4_102_444_800)
        .contains(&seconds)
        .then_some(seconds)
}

fn civil_date(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str().and_then(|s| s.get(..10)) {
        let b = s.as_bytes();
        if b[4] == b'-'
            && b[7] == b'-'
            && b.iter()
                .enumerate()
                .all(|(i, v)| i == 4 || i == 7 || v.is_ascii_digit())
        {
            return Some(s.to_owned());
        }
    }
    let t = timestamp(value)?;
    // Gregorian calendar date from UTC days since 1970.
    let z = t / 86_400 + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let day = doy - (153 * mp + 2) / 5 + 1;
    let year = era * 400 + yoe + i64::from(month <= 2);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn cursor_date(text: &str) -> Option<String> {
    let (value, _) = text
        .trim_start()
        .strip_prefix("<timestamp>")?
        .split_once("</timestamp>")?;
    let value = value.trim();
    if let Some(date) = civil_date(&Value::String(value.to_owned())) {
        return Some(date);
    }
    // Cursor also writes the local calendar date in forms such as "Monday, Jul 13, 2026, …".
    let mut fields = value.split(',');
    fields.next()?;
    let mut month_day = fields.next()?.split_whitespace();
    let month_name = month_day.next()?.get(..3)?;
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|month| *month == month_name)?
        + 1;
    let day: u32 = month_day.next()?.parse().ok()?;
    let year: u32 = fields.next()?.trim().parse().ok()?;
    if !(2000..2100).contains(&year) || !(1..=31).contains(&day) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn cursor_prompt(text: &str) -> Option<(&str, Option<String>, Option<i64>)> {
    const CONTEXT: &[&str] = &[
        "attached_files",
        "code_selection",
        "terminal_selection",
        "manually_attached_skills",
        "uploaded_documents",
        "image_files",
        "external_links",
        "system-reminder",
        "timestamp",
    ];
    let mut text = text.trim();
    let mut date = None;
    let mut instant = None;
    loop {
        if let Some(query) = text.strip_prefix("<user_query>") {
            return Some((
                clean_user(query.split_once("</user_query>")?.0)?,
                date,
                instant,
            ));
        }
        let Some(tag) = text
            .strip_prefix('<')
            .and_then(|rest| rest.split_once('>'))
            .map(|(tag, _)| tag)
            .filter(|tag| CONTEXT.contains(tag))
        else {
            return clean_user(text).map(|s| (s, date, instant));
        };
        if tag == "timestamp" {
            date = date.or_else(|| cursor_date(text));
            if let Some(raw) = text
                .strip_prefix("<timestamp>")
                .and_then(|s| s.split_once("</timestamp>"))
            {
                instant = instant.or_else(|| timestamp(&Value::String(raw.0.trim().to_owned())));
            }
        }
        text = text.split_once(&format!("</{tag}>"))?.1.trim_start();
    }
}

fn file_day(time: SystemTime) -> Option<String> {
    let instant = time::OffsetDateTime::from(time);
    let offset = time::UtcOffset::local_offset_at(instant).ok()?;
    Some(instant.to_offset(offset).date().to_string())
}

fn fill_cursor_days(turns: &mut [Turn], same_file_day: Option<String>) {
    let mut earlier = None;
    let same_file_day = same_file_day.filter(|day| {
        turns
            .iter()
            .filter_map(|turn| turn.date.as_ref())
            .all(|date| date == day)
    });
    let mut first = 0;
    while first < turns.len() {
        if let Some(date) = &turns[first].date {
            earlier = Some(date.clone());
            first += 1;
            continue;
        }
        let end = turns[first..]
            .iter()
            .position(|turn| turn.date.is_some())
            .map_or(turns.len(), |n| first + n);
        let later = turns.get(end).and_then(|turn| turn.date.as_ref());
        let inferred = match (earlier.as_ref(), later) {
            (Some(a), Some(b)) if a == b => Some((a, InferredDay::SameDayNeighbors)),
            _ => same_file_day
                .as_ref()
                .map(|day| (day, InferredDay::FileMetadata)),
        };
        if let Some((day, origin)) = inferred {
            let day = day.clone();
            for turn in &mut turns[first..end] {
                turn.date = Some(day.clone());
                turn.inferred_day = Some(origin);
            }
        }
        first = end;
    }
}

fn accepted_model(v: Option<&str>) -> Option<String> {
    v.map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('<') && *s != "unknown")
        .map(str::to_owned)
}

fn harness_version(value: Option<&str>) -> Option<String> {
    static VERSION: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"\A[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?\z")
            .expect("valid version pattern")
    });
    let value = value?.trim();
    let value = value.strip_prefix('v').unwrap_or(value);
    (value.len() <= 64 && VERSION.is_match(value)).then(|| value.to_owned())
}

#[derive(Default)]
struct Transcript {
    request_model: Option<String>,
    last_reply: Option<String>,
    codex_version: Option<String>,
    seen: HashSet<String>,
    skip: bool,
}

impl Transcript {
    fn reply(&mut self, model: Option<&str>) {
        self.last_reply = accepted_model(model);
    }

    fn human(
        &mut self,
        node: &Value,
        content: String,
        date: Option<String>,
        session: &str,
        line: usize,
    ) -> Option<Turn> {
        let text = clean_user(&content)?;
        if let Some(id) = first(node, &["/id", "/uuid"]) {
            if !self.seen.insert(id.to_owned()) {
                return None;
            }
        }
        Some(Turn {
            inferred_day: None,
            model: self.last_reply.clone(),
            harness_version: None,
            date,
            timestamp: None,
            text: text.to_owned(),
            session: session.to_owned(),
            line,
        })
    }

    fn event(&mut self, source: Source, event: &Value, session: &str, line: usize) -> Vec<Turn> {
        let kind = event["type"].as_str().unwrap_or("");
        let date = civil_date(&event["timestamp"]).or_else(|| civil_date(&event["time"]));
        let message = &event["message"];
        let mut result = Vec::new();
        match source {
            Source::Codex => {
                let p = &event["payload"];
                match kind {
                    "session_meta" => {
                        self.codex_version = harness_version(get(p, "/cli_version"));
                        self.skip = !p["source"]["subagent"].is_null()
                            || get(p, "/thread_source")
                                .is_some_and(|s| matches!(s, "guardian_review" | "subagent"))
                            || p["parent_thread_id"].is_string();
                    }
                    "turn_context" => self.request_model = accepted_model(get(p, "/model")),
                    "response_item" if get(p, "/type") == Some("message") => {
                        match get(p, "/role") {
                            Some("assistant")
                                if !any_text(&p["content"], &["output_text", "text"])
                                    .is_empty() =>
                            {
                                self.last_reply = accepted_model(get(p, "/model"))
                                    .or_else(|| self.request_model.clone());
                            }
                            Some("user") => {
                                let md = &p["internal_chat_message_metadata_passthrough"];
                                let kinds_ok =
                                    md["content_item_kinds"].as_array().is_none_or(|k| {
                                        k.iter().all(|item| {
                                            matches!(
                                                item.as_str(),
                                                Some("user.text" | "user.image" | "user.file")
                                            )
                                        })
                                    });
                                let provenance_ok =
                                    md["content_item_metadata"].as_array().is_none_or(|items| {
                                        items.iter().all(|item| {
                                            get(item, "/provenance/type")
                                                .is_none_or(|p| p == "user")
                                        })
                                    });
                                if kinds_ok && provenance_ok {
                                    result.extend(self.human(
                                        p,
                                        any_text(&p["content"], &["input_text", "text"]),
                                        date,
                                        session,
                                        line,
                                    ));
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            Source::Claude => {
                if event["isSidechain"].as_bool() == Some(true) {
                    return result;
                }
                if kind == "assistant"
                    && !any_text(&message["content"], &["text"]).is_empty()
                    && get(message, "/model") != Some("<synthetic>")
                {
                    self.reply(get(message, "/model"));
                } else if kind == "user"
                    && get(message, "/role") == Some("user")
                    && !matches!(get(event, "/promptSource"), Some("system" | "hook"))
                    && event["isMeta"].as_bool() != Some(true)
                    && !has_block(&message["content"], "tool_result")
                {
                    let text = any_text(&message["content"], &["text"]);
                    if !text.starts_with("Set model") {
                        result.extend(self.human(event, text, date, session, line));
                    }
                }
            }
            Source::Dsh => {
                let data = &event["data"];
                match kind {
                    "session" if event["delegationDepth"].as_u64().unwrap_or(0) > 0 => {
                        self.skip = true
                    }
                    "request/header" | "request/context" | "model/selection" => {
                        self.request_model =
                            accepted_model(first(data, &["/header/config/model", "/model"]));
                    }
                    "assistant/message" => {
                        let answer = &data["message"];
                        if !any_text(&answer["content"], &["text", "output_text"]).is_empty()
                            || get(answer, "/text").is_some()
                        {
                            self.last_reply =
                                accepted_model(first(answer, &["/model", "/providerData/model"]))
                                    .or_else(|| self.request_model.clone());
                        }
                    }
                    "agent/inbox/spliced" | "user/message" => {
                        let inserted = data["inserted"]
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or_else(|| std::slice::from_ref(data));
                        for node in inserted {
                            if get(node, "/role") == Some("user")
                                && get(node, "/source/kind") == Some("user")
                            {
                                result.extend(self.human(
                                    node,
                                    any_text(&node["content"], &["text", "input_text"]),
                                    date.clone(),
                                    session,
                                    line,
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Source::WorkBuddy
                if kind == "message"
                    && event["providerData"]["isSubAgent"].as_bool() != Some(true) =>
            {
                let content = &event["content"];
                match get(event, "/role") {
                    Some("assistant")
                        if !any_text(content, &["output_text", "text"]).is_empty() =>
                    {
                        self.reply(first(
                            event,
                            &[
                                "/providerData/model",
                                "/providerData/requestModelId",
                                "/providerData/requestModelName",
                                "/model",
                            ],
                        ));
                    }
                    Some("user") if !has_block(content, "tool_result") => {
                        result.extend(self.human(
                            event,
                            any_text(content, &["input_text", "text"]),
                            date,
                            session,
                            line,
                        ));
                    }
                    _ => {}
                }
            }
            Source::Cursor => match get(event, "/role") {
                Some("assistant")
                    if !any_text(&message["content"], &["text", "output_text"]).is_empty() =>
                {
                    self.reply(first(event, &["/message/model", "/model"]));
                }
                Some("user") if !has_block(&message["content"], "tool_result") => {
                    let input = any_text(&message["content"], &["text", "input_text"]);
                    if let Some((text, cursor_day, cursor_timestamp)) = cursor_prompt(&input) {
                        if let Some(mut turn) =
                            self.human(event, text.to_owned(), date.or(cursor_day), session, line)
                        {
                            turn.timestamp = cursor_timestamp;
                            result.push(turn);
                        }
                    }
                }
                _ => {}
            },
            Source::Pi if kind == "message" => match get(message, "/role") {
                Some("assistant") if !any_text(&message["content"], &["text"]).is_empty() => {
                    self.reply(get(message, "/model"))
                }
                Some("user") => result.extend(self.human(
                    event,
                    any_text(&message["content"], &["text"]),
                    date.or_else(|| civil_date(&message["timestamp"])),
                    session,
                    line,
                )),
                _ => {}
            },
            _ => {}
        }
        let instant = timestamp(&event["timestamp"])
            .or_else(|| timestamp(&event["time"]))
            .or_else(|| {
                (source == Source::Pi)
                    .then(|| timestamp(&message["timestamp"]))
                    .flatten()
            });
        let version = match source {
            Source::Codex if !result.is_empty() => self.codex_version.clone(),
            Source::Claude if !result.is_empty() => harness_version(get(event, "/version")),
            _ => None,
        };
        for turn in &mut result {
            turn.timestamp = instant.or(turn.timestamp);
            turn.harness_version = version.clone();
        }
        result
    }
}

fn has_block(content: &Value, kind: &str) -> bool {
    content
        .as_array()
        .is_some_and(|items| items.iter().any(|item| get(item, "/type") == Some(kind)))
}

#[derive(Deserialize)]
struct CodexEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(borrow)]
    payload: Option<&'a RawValue>,
}

#[derive(Deserialize)]
struct CodexPayload<'a> {
    #[serde(rename = "type")]
    kind: Option<&'a str>,
    role: Option<&'a str>,
}

fn is_codex_conversation_event(line: &[u8]) -> serde_json::Result<bool> {
    let event: CodexEnvelope<'_> = serde_json::from_slice(line)?;
    match event.kind {
        "session_meta" | "turn_context" => Ok(true),
        "response_item" => {
            let Some(payload) = event.payload else {
                return Ok(false);
            };
            let payload: CodexPayload<'_> = serde_json::from_str(payload.get())?;
            Ok(payload.kind == Some("message")
                && matches!(payload.role, Some("assistant" | "user")))
        }
        _ => Ok(false),
    }
}

fn read_transcript(
    source: Source,
    path: &Path,
    stats: &mut SourceStats,
    report: &mut impl FnMut(&mut SourceStats, Turn),
) -> io::Result<()> {
    let file = File::open(path)?;
    let same_file_day = if source == Source::Cursor {
        file.metadata().ok().and_then(|info| {
            let created = info.created().ok().and_then(file_day)?;
            let modified = info.modified().ok().and_then(file_day)?;
            (created == modified).then_some(created)
        })
    } else {
        None
    };
    let reader: Box<dyn Read> = if path.extension().is_some_and(|s| s == "zstd" || s == "zst") {
        Box::new(zstd::stream::read::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    let session = if source == Source::Dsh {
        path.parent().and_then(Path::file_name)
    } else {
        path.file_stem()
    }
    .and_then(|s| s.to_str())
    .unwrap_or("unknown");

    let session = if source == Source::Codex {
        session.trim_end_matches(".jsonl")
    } else {
        session
    };
    let mut transcript = Transcript::default();
    let mut cursor_turns = Vec::new();
    let mut reader = BufReader::with_capacity(128 * 1024, reader);
    let mut buf = Vec::new();
    let mut line = 0;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        line += 1;
        if buf.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if source == Source::Codex {
            match is_codex_conversation_event(&buf) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(_) => {
                    stats.malformed += 1;
                    continue;
                }
            }
        }
        let Ok(event) = serde_json::from_slice::<Value>(&buf) else {
            stats.malformed += 1;
            continue;
        };
        let turns = transcript.event(source, &event, session, line);
        if transcript.skip {
            stats.ignored += 1;
            break;
        }
        if source == Source::Cursor {
            cursor_turns.extend(turns);
        } else {
            for turn in turns {
                report(stats, turn);
            }
        }
    }
    if source == Source::Cursor {
        fill_cursor_days(&mut cursor_turns, same_file_day);
        for turn in cursor_turns {
            report(stats, turn);
        }
    }
    Ok(())
}

fn read_claude_history(
    path: &Path,
    transcript_ids: &HashSet<String>,
    stats: &mut SourceStats,
    report: &mut impl FnMut(&mut SourceStats, Turn),
) -> io::Result<()> {
    for (line, text) in BufReader::new(File::open(path)?).lines().enumerate() {
        let text = text?;
        if text.trim().is_empty() {
            continue;
        }
        let Ok(entry) = serde_json::from_str::<Value>(&text) else {
            stats.malformed += 1;
            continue;
        };
        let Some(session) = get(&entry, "/sessionId") else {
            continue;
        };
        if transcript_ids.contains(session) {
            continue;
        }
        let Some(text) = get(&entry, "/display").and_then(clean_user) else {
            continue;
        };
        stats.history_turns += 1;
        report(
            stats,
            Turn {
                inferred_day: None,
                model: None,
                harness_version: None,
                date: civil_date(&entry["timestamp"]),
                timestamp: timestamp(&entry["timestamp"]),
                text: text.to_owned(),
                session: session.to_owned(),
                line: line + 1,
            },
        );
    }
    Ok(())
}

fn cursor_db(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = home.join("Library/Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = home.join(".config");
    base.join("Cursor/User/globalStorage/state.vscdb")
}

fn read_cursor_db(
    path: &Path,
    already: &HashSet<String>,
    stats: &mut SourceStats,
    report: &mut impl FnMut(&mut SourceStats, Turn),
) -> rusqlite::Result<()> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut stmt = db.prepare("SELECT key, CAST(value AS BLOB) FROM cursorDiskKV WHERE key >= 'bubbleId:' AND key < 'bubbleId;' ORDER BY key")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let key: String = row.get(0)?;
        let Some((session, _)) = key
            .strip_prefix("bubbleId:")
            .and_then(|s| s.split_once(':'))
        else {
            continue;
        };
        if already.contains(session) {
            continue;
        }
        let Some(blob) = row.get::<_, Option<Vec<u8>>>(1)? else {
            continue;
        };
        let Ok(bubble) = serde_json::from_slice::<Value>(&blob) else {
            stats.malformed += 1;
            continue;
        };
        if bubble["type"].as_i64() != Some(1) {
            continue;
        }
        let Some(text) = first(&bubble, &["/text"]).and_then(clean_user) else {
            continue;
        };
        report(
            stats,
            Turn {
                inferred_day: None,
                model: None,
                harness_version: None,
                date: civil_date(&bubble["createdAt"]).or_else(|| civil_date(&bubble["timestamp"])),
                timestamp: timestamp(&bubble["createdAt"])
                    .or_else(|| timestamp(&bubble["timestamp"])),
                text: text.to_owned(),
                session: session.to_owned(),
                line: 0,
            },
        );
    }
    Ok(())
}

fn keep_source_entry(name: Option<&str>) -> bool {
    !matches!(name, Some("subagents" | "tool-results" | "tool_outputs"))
}

pub fn scan(
    home: &Path,
    use_env: bool,
    mut report: impl FnMut(&mut SourceStats, Turn),
) -> Vec<SourceStats> {
    let mut summaries = Vec::new();
    for source in Source::ALL {
        let mut stats = SourceStats {
            source,
            found: false,
            files: 0,
            compressed: 0,
            archived: 0,
            missing_days: 0,
            inferred_neighbors: 0,
            inferred_file_days: 0,
            history_turns: 0,
            duplicates: 0,
            ignored: 0,
            errors: 0,
            malformed: 0,
            turns: 0,
            attributed: 0,
            versioned_turns: 0,
            hits: 0,
        };
        let mut cursor_sessions = HashSet::new();
        let mut codex_sessions = HashSet::new();
        let mut claude_sessions = HashSet::new();
        let roots = source.roots(home, use_env);
        let mut count = |stats: &mut SourceStats, turn: Turn| {
            if source == Source::Cursor && turn.line != 0 {
                cursor_sessions.insert(turn.session.clone());
            }
            stats.turns += 1;
            stats.attributed += usize::from(turn.model.is_some());
            stats.versioned_turns += usize::from(turn.harness_version.is_some());
            stats.missing_days += usize::from(turn.date.is_none());
            stats.inferred_neighbors +=
                usize::from(turn.inferred_day == Some(InferredDay::SameDayNeighbors));
            stats.inferred_file_days +=
                usize::from(turn.inferred_day == Some(InferredDay::FileMetadata));
            report(stats, turn);
        };
        for root in &roots {
            if !root.is_dir() {
                continue;
            }
            stats.found = true;
            for entry in WalkDir::new(root)
                .sort_by_file_name()
                .into_iter()
                .filter_entry(|entry| keep_source_entry(entry.file_name().to_str()))
            {
                let entry = match entry {
                    Ok(v) => v,
                    Err(_) => {
                        stats.errors += 1;
                        continue;
                    }
                };
                if !entry.file_type().is_file() || !source.is_file(root, entry.path()) {
                    continue;
                }
                if source == Source::Codex {
                    let Some(name) = entry.file_name().to_str() else {
                        continue;
                    };
                    let logical_name = codex_filename(name);
                    if codex_packed(name) && entry.path().with_file_name(logical_name).is_file() {
                        continue;
                    }
                    if !codex_sessions.insert(logical_name.to_owned()) {
                        stats.duplicates += 1;
                        continue;
                    }
                    stats.compressed += usize::from(codex_packed(name));
                    stats.archived +=
                        usize::from(root.file_name().is_some_and(|p| p == "archived_sessions"));
                }
                if source == Source::Claude {
                    if let Some(id) = entry.path().file_stem().and_then(|name| name.to_str()) {
                        claude_sessions.insert(id.to_owned());
                    }
                }
                if source == Source::Dsh
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|ext| ext == "zstd" || ext == "zst")
                {
                    stats.compressed += 1;
                }
                stats.files += 1;
                if read_transcript(source, entry.path(), &mut stats, &mut count).is_err() {
                    stats.errors += 1;
                }
            }
        }
        if source == Source::Claude {
            let history = roots
                .first()
                .and_then(|p| p.parent())
                .map(|p| p.join("history.jsonl"));
            if let Some(path) = history.filter(|p| p.is_file()) {
                stats.found = true;
                if read_claude_history(&path, &claude_sessions, &mut stats, &mut count).is_err() {
                    stats.errors += 1;
                }
            }
        }
        if source == Source::Cursor {
            let path = cursor_db(home);
            if path.is_file() {
                stats.found = true;
                stats.files += 1;
                if read_cursor_db(&path, &cursor_sessions, &mut stats, &mut |stats, turn| {
                    stats.turns += 1;
                    stats.missing_days += usize::from(turn.date.is_none());
                    report(stats, turn);
                })
                .is_err()
                {
                    stats.errors += 1;
                }
            }
        }
        summaries.push(stats);
    }
    summaries
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_versions_from_the_log_that_produced_each_message() {
        let mut codex = Transcript::default();
        let user = json!({"type":"response_item","payload":{
            "type":"message","role":"user","content":[{"type":"input_text","text":"Check this."}]
        }});
        codex.event(
            Source::Codex,
            &json!({"type":"session_meta","payload":{"cli_version":"0.171.0-alpha.13"}}),
            "s",
            1,
        );
        let first = codex.event(Source::Codex, &user, "s", 2);
        assert_eq!(
            first[0].harness_version.as_deref(),
            Some("0.171.0-alpha.13")
        );
        codex.event(
            Source::Codex,
            &json!({"type":"session_meta","payload":{"cli_version":"0.172.0"}}),
            "s",
            3,
        );
        let resumed = codex.event(Source::Codex, &user, "s", 4);
        assert_eq!(resumed[0].harness_version.as_deref(), Some("0.172.0"));

        let mut claude = Transcript::default();
        let mut user = json!({"type":"user","version":"2.1.330","message":{"role":"user","content":"Check this."}});
        let turns = claude.event(Source::Claude, &user, "s", 1);
        assert_eq!(turns[0].harness_version.as_deref(), Some("2.1.330"));
        user["version"] = json!(4);
        let turns = claude.event(Source::Claude, &user, "s", 2);
        assert_eq!(turns[0].harness_version, None);
    }

    #[test]
    fn response_model_survives_followup_model_switch() {
        let mut session = Transcript::default();
        let response = |role, text| {
            json!({
                "type":"response_item",
                "payload":{"type":"message","role":role,"content":[{"type":"text","text":text}]}
            })
        };
        session.event(
            Source::Codex,
            &json!({"type":"turn_context","payload":{"model":"A"}}),
            "s",
            1,
        );
        session.event(
            Source::Codex,
            &response("assistant", "previous reply"),
            "s",
            2,
        );
        session.event(
            Source::Codex,
            &json!({"type":"turn_context","payload":{"model":"B"}}),
            "s",
            3,
        );
        let turns = session.event(Source::Codex, &response("user", "follow up"), "s", 4);
        assert_eq!(turns[0].model.as_deref(), Some("A"));
    }

    #[test]
    fn extracts_human_text_from_cursor_without_wrapper() {
        let event = json!({"role":"user","message":{"content":[{"type":"text",
            "text":"<timestamp>2026-11-24T03:00:00Z</timestamp>\n<user_query>你就是个废物</user_query>"
        }]}});
        let turns = Transcript::default().event(Source::Cursor, &event, "s", 1);
        assert_eq!(turns[0].text, "你就是个废物");
        assert_eq!(turns[0].date.as_deref(), Some("2026-11-24"));
        assert_eq!(
            cursor_date("<timestamp>Monday, Jul 13, 2026, 7:50 PM (UTC+8)</timestamp>"),
            Some("2026-07-13".into())
        );
        assert_eq!(
            cursor_date("<timestamp>Monday, Jun 1, 2026, 7:50 PM (UTC+8)</timestamp>"),
            Some("2026-06-01".into())
        );
    }

    #[test]
    fn cursor_extracts_the_actual_user_request_after_attachments() {
        let input = "<attached_files>Ignore quoted <user_query>fake</user_query></attached_files>\n<timestamp>Monday, Jul 13, 2026, 7:50 PM (UTC+8)</timestamp>\n<user_query>actual user input</user_query>";
        let (text, date, _) = cursor_prompt(input).unwrap();
        assert_eq!(text, "actual user input");
        assert_eq!(date.as_deref(), Some("2026-07-13"));
        assert!(
            cursor_prompt("<hooks_context><user_query>not from user</user_query></hooks_context>")
                .is_none()
        );
    }

    #[test]
    fn cursor_date_inference_preserves_uncertain_multiday_records() {
        let make_turn = |date: Option<&str>| Turn {
            inferred_day: None,
            model: None,
            harness_version: None,
            date: date.map(str::to_owned),
            timestamp: None,
            text: String::new(),
            session: String::new(),
            line: 0,
        };
        let mut turns = [
            make_turn(Some("2026-09-01")),
            make_turn(None),
            make_turn(Some("2026-09-01")),
            make_turn(None),
            make_turn(Some("2026-09-02")),
        ];
        fill_cursor_days(&mut turns, Some("2026-09-01".to_owned()));
        assert_eq!(turns[1].inferred_day, Some(InferredDay::SameDayNeighbors));
        assert!(turns[3].date.is_none());
        let mut single_day = [make_turn(None)];
        fill_cursor_days(&mut single_day, Some("2026-09-02".to_owned()));
        assert_eq!(single_day[0].date.as_deref(), Some("2026-09-02"));
        assert_eq!(single_day[0].inferred_day, Some(InferredDay::FileMetadata));
    }

    #[test]
    fn codex_accepts_live_and_archived_compressed_rollouts() {
        let root = Path::new("/example");
        for name in [
            "rollout-1.jsonl",
            "rollout-1.jsonl.zst",
            "rollout-1.jsonl.zstd",
        ] {
            assert!(Source::Codex.is_file(root, &root.join(name)));
            assert_eq!(codex_filename(name), "rollout-1.jsonl");
        }
        assert!(!Source::Codex.is_file(root, &root.join("random.zst")));
    }
}
