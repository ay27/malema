use crate::detect::{self, Hit};
use crate::i18n;
use crate::sources::{Source, Turn};
use rusqlite::{Connection, OpenFlags, TransactionBehavior, params};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub type MessageKey = [u8; 32];
pub type Verdicts = HashMap<Vec<u8>, Verdict>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Excluded = 0,
    Direct = 1,
    Output = 2,
}

impl Verdict {
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Excluded),
            1 => Some(Self::Direct),
            2 => Some(Self::Output),
            _ => None,
        }
    }

    pub fn reason(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::Excluded => None,
            Self::Direct => Some((
                "ai.direct-insult",
                i18n::pick("明确辱骂当前 AI", "explicitly insults the AI"),
            )),
            Self::Output => Some((
                "ai.output-insult",
                i18n::pick(
                    "明确辱骂当前 AI 的产物或操作",
                    "explicitly insults what the AI produced",
                ),
            )),
        }
    }

    pub fn hit(self, rule_hit: Option<Hit>, text: &str) -> Option<Hit> {
        let (rule, reason) = self.reason()?;
        let evidence = rule_hit.map(|hit| hit.evidence).unwrap_or_else(|| {
            detect::visible_lines(text)
                .next()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(80)
                .collect()
        });
        Some(Hit {
            rule,
            reason: reason.to_owned(),
            evidence,
        })
    }
}

pub fn message_key(source: Source, turn: &Turn) -> MessageKey {
    // Use a stable digest so an edited message never inherits its old verdict.
    let mut hash = Sha256::new();
    for part in [source.label(), turn.session.as_str(), turn.text.as_str()] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part.as_bytes());
    }
    hash.update((turn.line as u64).to_le_bytes());
    hash.update(turn.timestamp.unwrap_or(i64::MIN).to_le_bytes());
    hash.finalize().into()
}

fn path(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Malema/review-verdicts-v1.sqlite3")
}

pub fn modified_at(home: &Path) -> Option<SystemTime> {
    fs::metadata(path(home)).ok()?.modified().ok()
}

pub fn load(home: &Path) -> Result<Verdicts, Box<dyn Error>> {
    let path = path(home);
    if !path.try_exists()? {
        return Ok(Verdicts::new());
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut select = db.prepare("SELECT message_key, verdict FROM review_verdicts")?;
    let mut results = Verdicts::new();
    for row in select.query_map([], |row| Ok((row.get(0)?, row.get::<_, u8>(1)?)))? {
        let (key, code) = row?;
        if let Some(verdict) = Verdict::from_code(code) {
            results.insert(key, verdict);
        }
    }
    Ok(results)
}

pub fn save(home: &Path, updates: &[(MessageKey, Verdict)]) -> Result<(), Box<dyn Error>> {
    if updates.is_empty() {
        return Ok(());
    }
    let path = path(home);
    fs::create_dir_all(path.parent().expect("review database parent directory"))?;
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(&path)?;
    let mut db = Connection::open(path)?;
    db.busy_timeout(Duration::from_secs(5))?;
    let transaction = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
    transaction.execute_batch(
        "CREATE TABLE IF NOT EXISTS review_verdicts (
            message_key BLOB PRIMARY KEY,
            verdict INTEGER NOT NULL CHECK (verdict IN (0, 1, 2))
        ) WITHOUT ROWID;",
    )?;
    {
        let mut upsert = transaction.prepare(
            "INSERT INTO review_verdicts (message_key, verdict) VALUES (?1, ?2)
             ON CONFLICT (message_key) DO UPDATE SET verdict = excluded.verdict",
        )?;
        for (key, verdict) in updates {
            upsert.execute(params![key.as_slice(), *verdict as u8])?;
        }
    }
    transaction.commit()?;
    Ok(())
}
