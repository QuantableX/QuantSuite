//! Review-queue proposals for memories that outgrew their shape: append-only
//! logs whose current state is buried under dated history, and near-duplicates
//! by embedding similarity. Detection and proposal text only: nothing here
//! writes, merges or deletes a memory. Applying a proposal is a separate,
//! human-reviewed write.
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use std::collections::{BTreeMap, HashMap};

/// A memory reads as a log once this many blocks open with an ISO date
/// (`## 2026-09-08 — round 3`, `2026-09-22: release failed …`, `**Round 3
/// (2026-09-08)**`). On the real vault (2026-09-29) this separates the
/// appended logs (6–28 dated entries) from structured notes with a few dates.
pub const LOG_MIN_DATED_ENTRIES: usize = 5;
/// …and has at least this many words: a short note with a few dated lines is
/// still readable in one go.
pub const LOG_MIN_WORDS: i64 = 800;
/// From this size on, the proposal is a split rather than a section on top.
pub const LOG_SPLIT_WORDS: i64 = 5000;
/// Only this many leading characters of a paragraph count as its lead; a date
/// deep inside running text is not an entry marker. Headings count in full.
pub const ENTRY_LEAD_CHARS: usize = 80;
/// The section a consolidation puts under the title, dated:
/// `## Current state (YYYY-MM-DD)`. Entries dated after it count again.
pub const CURRENT_STATE_HEADING: &str = "Current state";
/// Two memories are near-duplicates when the chunks of the smaller one match
/// the other one's chunks at this mean best cosine similarity or above.
pub const NEAR_DUPLICATE_SIMILARITY: f64 = 0.92;
/// Pairwise chunk comparison is quadratic; a scope and model with more
/// embedded chunks than this is skipped instead of stalling the queue.
pub const NEAR_DUPLICATE_MAX_CHUNKS: usize = 1500;

/// The dated-entry structure of a memory body.
#[derive(Debug, Default, PartialEq)]
pub struct LogShape {
    /// Dated entries, after the current-state date when there is one.
    pub entries: usize,
    pub first: Option<NaiveDate>,
    pub last: Option<NaiveDate>,
    /// The date of a `## Current state (YYYY-MM-DD)` heading.
    pub consolidated: Option<NaiveDate>,
}

/// Every valid `YYYY-MM-DD` in `text` that is not part of a longer number.
fn iso_dates(text: &str) -> Vec<NaiveDate> {
    let bytes = text.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    let mut out = vec![];
    for start in 0..bytes.len().saturating_sub(9) {
        let shape = (0..10).all(|k| {
            if k == 4 || k == 7 {
                bytes[start + k] == b'-'
            } else {
                digit(start + k)
            }
        });
        let bounded =
            (start == 0 || !bytes[start - 1].is_ascii_alphanumeric()) && !digit(start + 10);
        if shape && bounded {
            if let Ok(date) = NaiveDate::parse_from_str(&text[start..start + 10], "%Y-%m-%d") {
                out.push(date);
            }
        }
    }
    out
}

/// `## …` to `###### …`; the H1 title is not an entry.
fn sub_heading(line: &str) -> Option<&str> {
    let hashes = line.len() - line.trim_start_matches('#').len();
    (2..=6)
        .contains(&hashes)
        .then(|| line[hashes..].strip_prefix(' '))
        .flatten()
}

/// Count the blocks that open with a date: headings and paragraph leads,
/// outside fenced code.
pub fn log_shape(body: &str) -> LogShape {
    let mut shape = LogShape::default();
    let mut dated: Vec<NaiveDate> = vec![];
    let (mut fenced, mut previous_blank) = (false, true);
    for raw in body.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            previous_blank = false;
            continue;
        }
        if fenced {
            continue;
        }
        if line.is_empty() {
            previous_blank = true;
            continue;
        }
        let heading = sub_heading(line);
        let starts_block = previous_blank || heading.is_some();
        previous_blank = false;
        if !starts_block {
            continue;
        }
        let lead: String = match heading {
            Some(text) => text.to_string(),
            None if line.starts_with("# ") => continue,
            None => line.chars().take(ENTRY_LEAD_CHARS).collect(),
        };
        let Some(date) = iso_dates(&lead).into_iter().max() else {
            continue;
        };
        if heading.is_some_and(|h| {
            h.get(..CURRENT_STATE_HEADING.len())
                .is_some_and(|p| p.eq_ignore_ascii_case(CURRENT_STATE_HEADING))
        }) {
            shape.consolidated = shape.consolidated.max(Some(date));
            continue;
        }
        dated.push(date);
    }
    dated.retain(|d| shape.consolidated.map_or(true, |c| *d > c));
    shape.entries = dated.len();
    shape.first = dated.iter().min().copied();
    shape.last = dated.iter().max().copied();
    shape
}

/// The review reason and consolidation proposal for a log-like memory.
pub fn log_like(body: &str, words: i64) -> Option<(String, String)> {
    let shape = log_shape(body);
    if shape.entries < LOG_MIN_DATED_ENTRIES || words < LOG_MIN_WORDS {
        return None;
    }
    let span = match (shape.first, shape.last) {
        (Some(first), Some(last)) if first != last => format!("{first} to {last}"),
        (Some(first), _) => first.to_string(),
        _ => String::new(),
    };
    let since = shape
        .consolidated
        .map(|date| format!(" since the current state of {date}"))
        .unwrap_or_default();
    let reason = format!(
        "log-like: {} dated entries{since} ({span}), {words} words",
        shape.entries
    );
    let proposal = if let Some(date) = shape.consolidated {
        format!("Consolidation proposal: fold the entries since {date} into '## {CURRENT_STATE_HEADING} (YYYY-MM-DD)' and re-date it; the entries stay below as history.")
    } else if words >= LOG_SPLIT_WORDS {
        "Consolidation proposal: split — a new memory holds what is true today; this one keeps the history and gets supersededBy the new one.".to_string()
    } else {
        format!("Consolidation proposal: put '## {CURRENT_STATE_HEADING} (YYYY-MM-DD)' under the title with what is true today; keep the dated entries below under '## History'.")
    };
    Some((reason, proposal))
}

fn unit(vector: Vec<f32>) -> Option<Vec<f32>> {
    let norm = vector
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    (norm > 0.0 && norm.is_finite()).then(|| {
        vector
            .iter()
            .map(|v| (f64::from(*v) / norm) as f32)
            .collect()
    })
}

/// Mean over the chunks of `small` of their best cosine match in `large`.
fn coverage(small: &[Vec<f32>], large: &[Vec<f32>]) -> Option<f64> {
    let mut total = 0.0;
    for a in small {
        let best = large
            .iter()
            .filter(|b| b.len() == a.len())
            .map(|b| {
                a.iter()
                    .zip(b)
                    .map(|(x, y)| f64::from(*x) * f64::from(*y))
                    .sum::<f64>()
            })
            .max_by(f64::total_cmp)?;
        total += best;
    }
    (!small.is_empty()).then(|| total / small.len() as f64)
}

/// Near-duplicate pairs from the stored chunk vectors of current content,
/// compared only within one scope and one embedding model. Empty when no
/// vectors exist. Returns memory id → [(other id, similarity)].
pub fn near_duplicates(
    conn: &Connection,
    scope: Option<&str>,
) -> Result<HashMap<String, Vec<(String, f64)>>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT m.scope, e.model_key, e.memory_id, e.vector_json FROM memory_embeddings e
             JOIN memories m ON m.id=e.memory_id AND m.content_hash=e.content_hash
             WHERE ?1 IS NULL OR m.scope=?1 ORDER BY m.scope, e.model_key, e.memory_id, e.chunk_no",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params![scope], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    type Group = BTreeMap<String, Vec<Vec<f32>>>;
    let mut groups: BTreeMap<(String, String), (usize, Group)> = BTreeMap::new();
    for row in rows {
        let (scope, key, id, json) = row.map_err(|e| e.to_string())?;
        let Some(vector) = serde_json::from_str::<Vec<f32>>(&json).ok().and_then(unit) else {
            continue;
        };
        let group = groups.entry((scope, key)).or_default();
        group.0 += 1;
        group.1.entry(id).or_default().push(vector);
    }
    let mut out: HashMap<String, Vec<(String, f64)>> = HashMap::new();
    for (chunks, memories) in groups.into_values() {
        if chunks > NEAR_DUPLICATE_MAX_CHUNKS {
            continue;
        }
        let memories: Vec<_> = memories.into_iter().collect();
        for (i, (a_id, a)) in memories.iter().enumerate() {
            for (b_id, b) in &memories[i + 1..] {
                let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
                let Some(similarity) = coverage(small, large) else {
                    continue;
                };
                if similarity >= NEAR_DUPLICATE_SIMILARITY {
                    for (from, to) in [(a_id, b_id), (b_id, a_id)] {
                        let list = out.entry(from.clone()).or_default();
                        match list.iter_mut().find(|(other, _)| other == to) {
                            Some(existing) => existing.1 = existing.1.max(similarity),
                            None => list.push((to.clone(), similarity)),
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn log(entries: usize) -> String {
        let mut body = "# Synthetic forge log 2026-01-01\n\nIntro without a date.\n".to_string();
        for i in 1..=entries {
            body.push_str(&format!(
                "\n## 2026-02-{i:02} — round {i}\n\nResult text for round {i}.\n"
            ));
        }
        body
    }

    #[test]
    fn dated_blocks_are_entries_and_the_title_is_not() {
        let body = "# Title 2026-01-01\n\n## 2026-01-02 — first\ntext 2026-01-09 in the same block\n\n2026-01-03: a paragraph entry\n\n**Round 3 (2026-01-04)** bold lead\n\nThis paragraph mentions a date only far into its running text, well beyond the lead: 2026-01-05.\n\n```\n2026-01-06 inside a fence\n\n2026-01-07 still inside\n```\n\n- 2026-01-08 a list item block\n- 2026-01-10 same list, not a new block\n\nNot a date: 12026-01-01 or 2026-13-40 or v2026-01-02.";
        let shape = log_shape(body);
        assert_eq!(shape.entries, 4);
        assert_eq!(shape.first, Some(date("2026-01-02")));
        assert_eq!(shape.last, Some(date("2026-01-08")));
        assert_eq!(shape.consolidated, None);
    }

    #[test]
    fn log_like_needs_entries_and_words_and_proposes_by_size() {
        let words = LOG_MIN_WORDS;
        assert!(log_like(&log(LOG_MIN_DATED_ENTRIES - 1), words).is_none());
        assert!(log_like(&log(LOG_MIN_DATED_ENTRIES), words - 1).is_none());
        let (reason, proposal) = log_like(&log(LOG_MIN_DATED_ENTRIES), words).unwrap();
        assert_eq!(
            reason,
            format!("log-like: 5 dated entries (2026-02-01 to 2026-02-05), {words} words")
        );
        assert!(proposal.contains("'## Current state (YYYY-MM-DD)' under the title"));
        assert!(proposal.contains("'## History'"));
        let (_, split) = log_like(&log(9), LOG_SPLIT_WORDS).unwrap();
        assert!(split.contains("split") && split.contains("supersededBy"));
    }

    #[test]
    fn a_dated_current_state_resets_the_count_until_new_entries_pile_up() {
        let consolidated = log(8).replacen(
            "Intro without a date.",
            "## Current state (2026-02-08)\n\nWhat holds today.\n\n## History",
            1,
        );
        assert_eq!(log_shape(&consolidated).entries, 0);
        assert!(log_like(&consolidated, LOG_SPLIT_WORDS).is_none());
        let mut grown = consolidated.clone();
        for day in 10..10 + LOG_MIN_DATED_ENTRIES {
            grown.push_str(&format!(
                "\n2026-03-{day}: appended after the consolidation\n"
            ));
        }
        let (reason, proposal) = log_like(&grown, LOG_MIN_WORDS).unwrap();
        assert!(
            reason.starts_with("log-like: 5 dated entries since the current state of 2026-02-08")
        );
        assert!(proposal.contains("fold the entries since 2026-02-08"));
        // Undated: nothing marks where the history ends, so everything counts.
        let undated = consolidated.replace("## Current state (2026-02-08)", "## Current state");
        assert_eq!(log_shape(&undated).entries, 8);
    }

    #[test]
    fn coverage_is_mean_best_match_of_the_smaller_memory() {
        let a = vec![unit(vec![1.0, 0.0]).unwrap()];
        let b = vec![
            unit(vec![0.0, 1.0]).unwrap(),
            unit(vec![1.0, 0.01]).unwrap(),
        ];
        assert!(coverage(&a, &b).unwrap() > 0.99);
        assert!(coverage(&b, &a).unwrap() < 0.6);
        assert!(coverage(&a, &[vec![1.0, 0.0, 0.0]]).is_none());
        assert!(unit(vec![0.0, 0.0]).is_none());
    }
}
