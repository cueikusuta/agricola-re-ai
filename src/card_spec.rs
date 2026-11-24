//! Spec JSON reading layer for Review O1 Stage 1.
//!
//! Reads `data/bga-card-specs/*.json` and extracts only the fields needed for
//! spec x Rust reconciliation: card id, per-player-count eligibility, cost
//! shape (free / unconfirmed / items), and implementation status.
//!
//! Card names, effects, prerequisites, original text and reference URLs are
//! deliberately NOT modeled (AGENTS.md rule 6). Unknown fields are ignored by
//! serde.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;

/// Per-player-count eligibility as declared by the spec JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpecElig {
    #[default]
    Unknown,
    Yes,
    No,
}

/// Cost shape declared by `play.cost` in the spec JSON.
///
/// `null` (unconfirmed) and `[]` (confirmed free) are distinct and must not
/// be conflated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpecCost {
    Unconfirmed,               // play.cost == null
    Free,                      // play.cost == []
    Items(Vec<(String, u32)>), // play.cost == [{ good, amount }, ...]
}

/// One spec JSON's reconciled fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CardSpec {
    pub card_id: String,
    pub eligible_2: SpecElig,
    pub eligible_3: SpecElig,
    pub eligible_4: SpecElig,
    pub cost: SpecCost,
    pub implementation_status: String,
}

impl CardSpec {
    pub fn eligible_at(&self, players: usize) -> Option<SpecElig> {
        match players {
            2 => Some(self.eligible_2),
            3 => Some(self.eligible_3),
            4 => Some(self.eligible_4),
            _ => None,
        }
    }
}

/// Error while loading spec JSONs. Any parse error or missing required field
/// fails the whole load; nothing is silently skipped.
#[derive(Debug)]
pub enum SpecLoadError {
    Io(std::io::Error),
    Json { file: String, message: String },
    MissingField { file: String, field: String },
}

impl std::fmt::Display for SpecLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecLoadError::Io(e) => write!(f, "io error: {e}"),
            SpecLoadError::Json { file, message } => write!(f, "json error in {file}: {message}"),
            SpecLoadError::MissingField { file, field } => {
                write!(f, "missing field {field} in {file}")
            }
        }
    }
}

impl std::error::Error for SpecLoadError {}

impl From<std::io::Error> for SpecLoadError {
    fn from(e: std::io::Error) -> Self {
        SpecLoadError::Io(e)
    }
}

#[derive(Deserialize)]
struct SpecFile {
    card_id: String,
    bga: Bga,
    play: Play,
    review: Review,
}

#[derive(Deserialize)]
struct Bga {
    eligibility: BTreeMap<String, SpecEligStr>,
}

#[derive(Deserialize)]
struct Play {
    cost: Option<Vec<CostItem>>,
}

#[derive(Deserialize)]
struct Review {
    implementation_status: String,
}

#[derive(Deserialize)]
struct CostItem {
    good: String,
    amount: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SpecEligStr {
    Yes,
    No,
    Unknown,
}

/// List and parse every `*.json` under `dir` (sorted by file name).
/// Any parse error or missing required field fails the whole load
/// (no silent skip).
pub fn load_card_specs(dir: &Path) -> Result<Vec<CardSpec>, SpecLoadError> {
    let mut entries: Vec<(String, CardSpec)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file = path.to_string_lossy().to_string();
        let text = fs::read_to_string(&path).map_err(|e| SpecLoadError::Json {
            file: file.clone(),
            message: e.to_string(),
        })?;
        let parsed: SpecFile = serde_json::from_str(&text).map_err(|e| SpecLoadError::Json {
            file: file.clone(),
            message: e.to_string(),
        })?;
        let eligibility = &parsed.bga.eligibility;
        let eligible = |key: &str| -> SpecElig {
            match eligibility.get(key).copied() {
                Some(SpecEligStr::Yes) => SpecElig::Yes,
                Some(SpecEligStr::No) => SpecElig::No,
                Some(SpecEligStr::Unknown) | None => SpecElig::Unknown,
            }
        };
        let cost = match parsed.play.cost {
            None => SpecCost::Unconfirmed,
            Some(items) if items.is_empty() => SpecCost::Free,
            Some(items) => SpecCost::Items(items.into_iter().map(|i| (i.good, i.amount)).collect()),
        };
        entries.push((
            parsed.card_id.clone(),
            CardSpec {
                card_id: parsed.card_id,
                eligible_2: eligible("2"),
                eligible_3: eligible("3"),
                eligible_4: eligible("4"),
                cost,
                implementation_status: parsed.review.implementation_status,
            },
        ));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(entries.into_iter().map(|(_, s)| s).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_elig_unknown_is_default() {
        assert_eq!(SpecElig::default(), SpecElig::Unknown);
    }
}
