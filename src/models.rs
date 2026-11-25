use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Catalog {
    models: BTreeMap<String, Vec<String>>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../model-catalog.json")).expect("public model catalog")
    })
}

/// Return only a constant from the public catalog. Never return input text.
pub fn normalize(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "other".to_owned();
    };
    if raw.len() > 1024 || !raw.is_ascii() || raw.chars().any(char::is_control) {
        return "other".to_owned();
    }
    let value = raw.trim().to_ascii_lowercase().replace(['_', ' '], "-");
    if catalog().models.contains_key(&value) {
        return value;
    }
    let bytes = value.as_bytes();
    let mut matches = Vec::new();
    for (canonical, aliases) in &catalog().models {
        for alias in aliases {
            for (start, _) in value.match_indices(alias) {
                let end = start + alias.len();
                let before = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
                let after = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
                // A public version must not match the prefix of a private version.
                let longer_version = end + 1 < bytes.len()
                    && matches!(bytes[end], b'.' | b'-')
                    && bytes[end + 1].is_ascii_digit();
                // Dated snapshots are ordinary aliases, not a new version number.
                let dated = end < bytes.len()
                    && bytes[end] == b'-'
                    && value[end + 1..].split('-').next().is_some_and(|s| {
                        matches!(s.len(), 4 | 8) && s.bytes().all(|b| b.is_ascii_digit())
                    });
                if before && after && (!longer_version || dated) {
                    matches.push((start, end, canonical));
                }
            }
        }
    }
    let candidates: BTreeSet<_> = matches
        .iter()
        .filter(|&&(start, end, _)| {
            !matches.iter().any(|&(left, right, _)| {
                left <= start && right >= end && right - left > end - start
            })
        })
        .map(|(_, _, canonical)| canonical.as_str())
        .collect();
    if candidates.len() == 1 {
        candidates.first().unwrap().to_string()
    } else {
        "other".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_matches_shared_privacy_cases() {
        let cases: Vec<(Option<String>, String)> =
            serde_json::from_str(include_str!("../model-normalization-cases.json")).unwrap();
        for (input, expected) in cases {
            assert_eq!(normalize(input.as_deref()), expected, "input={input:?}");
        }
        for canonical in catalog().models.keys() {
            assert_eq!(normalize(Some(canonical)), *canonical);
        }
        assert_eq!(normalize(Some(&"x".repeat(1025))), "other");
    }
}
