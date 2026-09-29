//! Lexical retrieval for memory_context, and the query cleanup `search` shares with it.
//!
//! Queries come in any language. A query's own function words are dropped once it clearly
//! is in that language (two or more of the language's stopwords): in a mostly English vault
//! "die" and "im" are rare, so BM25 would weigh them above every content word and pull a
//! German question onto whichever German notes exist. Memories are ranked by BM25 over the
//! remaining words, and those containing all of them count twice in the fusion. Parts of
//! compounds and scripts written without spaces (Chinese, Japanese, Thai, ...) are found
//! through a trigram index.
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Stopword lists of the npm package `stopword` 3.1.5, one per language (licenses in
/// data/STOPWORDS-THIRD-PARTY.txt). Breton, Malay and Brazilian Portuguese are left out:
/// their lists hold common English words ("stop", "global", "local").
const STOPWORDS: &str = include_str!("../data/stopwords.json");

/// Content words the German list counts as stopwords.
const NOT_STOPWORDS: &[&str] = &[
    "beispiel",
    "ende",
    "gott",
    "gross",
    "grosse",
    "grossen",
    "grosser",
    "grosses",
    "groß",
    "große",
    "großen",
    "großer",
    "großes",
    "gut",
    "gute",
    "guter",
    "gutes",
    "heute",
    "hoch",
    "jahr",
    "jahre",
    "jahren",
    "kleine",
    "kleinen",
    "kleiner",
    "kleines",
    "kurz",
    "lang",
    "lange",
    "leicht",
    "mann",
    "mensch",
    "menschen",
    "mittel",
    "morgen",
    "möglich",
    "neue",
    "neuen",
    "oben",
    "offen",
    "ordnung",
    "recht",
    "rechte",
    "rechten",
    "rechter",
    "rechtes",
    "richtig",
    "sache",
    "schlecht",
    "schluss",
    "startseite",
    "suche",
    "tag",
    "tage",
    "tagen",
    "teil",
    "uhr",
    "weg",
    "wissen",
    "zeit",
];

/// A language counts as the query's language from this many of its stopwords on.
const MIN_LANGUAGE_HITS: usize = 2;
/// Words this long also match longer words that start with them ("backtest" → "backtests",
/// "gedächtnis" → "gedächtnissuche"); shorter ones match only themselves.
const MIN_PREFIX_CHARS: usize = 4;
/// Words this long also search the trigram index ("suche" → "gedächtnissuche").
const MIN_FRAGMENT_CHARS: usize = 4;
/// Trigram fragments per query; a run without spaces yields one per character.
const MAX_FRAGMENTS: usize = 48;
/// Candidates read per signal before fusion.
const MAX_HITS: i64 = 1000;

pub const ALL_TERMS: &str = "lexical: all terms";
pub const SOME_TERMS: &str = "lexical: some terms";
pub const SUBSTRING: &str = "substring match";

fn stopwords() -> &'static Vec<HashSet<String>> {
    static LISTS: OnceLock<Vec<HashSet<String>>> = OnceLock::new();
    LISTS.get_or_init(|| {
        let lists: HashMap<String, Vec<String>> =
            serde_json::from_str(STOPWORDS).expect("bundled stopwords.json is valid");
        lists
            .into_values()
            .map(|words| {
                words
                    .into_iter()
                    .filter(|w| !NOT_STOPWORDS.contains(&w.as_str()))
                    .collect()
            })
            .collect()
    })
}

/// Scripts written without spaces between words.
fn unsegmented(c: char) -> bool {
    matches!(u32::from(c),
        0x0E00..=0x0EFF      // Thai, Lao
        | 0x1000..=0x109F    // Myanmar
        | 0x1780..=0x17FF    // Khmer
        | 0x3040..=0x30FF    // Hiragana, Katakana
        | 0x31F0..=0x31FF    // Katakana extensions
        | 0x3400..=0x4DBF    // CJK extension A
        | 0x4E00..=0x9FFF    // CJK unified ideographs
        | 0xF900..=0xFAFF    // CJK compatibility ideographs
        | 0xFF66..=0xFF9F    // halfwidth Katakana
        | 0x20000..=0x3134F) // CJK extensions B–G
}

/// Characters inside words, close to what FTS5's unicode61 tokenizer keeps: letters,
/// digits, combining marks — not spaces, punctuation or symbols.
fn in_word(c: char) -> bool {
    if c.is_alphanumeric() || unsegmented(c) {
        return true;
    }
    !c.is_ascii()
        && !c.is_whitespace()
        && !matches!(u32::from(c),
            0x00A0..=0x00BF | 0x00D7 | 0x00F7 // Latin-1 punctuation and signs
            | 0x2000..=0x2BFF                 // general punctuation, arrows, math, shapes
            | 0x3000..=0x303F                 // CJK punctuation
            | 0xFE30..=0xFE4F                 // CJK compatibility forms
            | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20 | 0xFF3B..=0xFF40 | 0xFF5B..=0xFF65
            | 0x1F000..=0x1FAFF) // emoji and pictographs
}

/// One whole-word search term.
pub struct Term {
    text: String,
    prefix: bool,
}

impl Term {
    /// The term as an FTS5 query string.
    pub fn expr(&self) -> String {
        let star = if self.prefix { "*" } else { "" };
        format!("\"{}\"{star}", self.text)
    }
}

pub struct Analysis {
    /// Content words of spaced scripts, the query language's function words removed.
    pub terms: Vec<Term>,
    /// Runs of scripts written without spaces, as typed.
    pub runs: Vec<String>,
    /// Substrings for the trigram index: long content words and trigrams of the runs.
    pub fragments: Vec<String>,
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.contains(&value) {
        list.push(value);
    }
}

pub fn analyze(query: &str) -> Analysis {
    let (mut words, mut runs) = (Vec::new(), Vec::new());
    for token in query.split(|c: char| !in_word(c)).filter(|t| !t.is_empty()) {
        let mut part = String::new();
        let mut part_unsegmented = false;
        for c in token.to_lowercase().chars() {
            if !part.is_empty() && unsegmented(c) != part_unsegmented {
                let done = std::mem::take(&mut part);
                push_unique(
                    if part_unsegmented {
                        &mut runs
                    } else {
                        &mut words
                    },
                    done,
                );
            }
            part_unsegmented = unsegmented(c);
            part.push(c);
        }
        push_unique(
            if part_unsegmented {
                &mut runs
            } else {
                &mut words
            },
            part,
        );
    }
    let lists = stopwords();
    let hits: Vec<usize> = lists
        .iter()
        .map(|list| words.iter().filter(|w| list.contains(*w)).count())
        .collect();
    let best = hits.iter().copied().max().unwrap_or(0);
    let language: Vec<&HashSet<String>> = lists
        .iter()
        .zip(&hits)
        .filter(|(_, n)| best >= MIN_LANGUAGE_HITS && **n == best)
        .map(|(list, _)| list)
        .collect();
    let chars = |w: &str| w.chars().count();
    let mut content: Vec<&String> = words
        .iter()
        .filter(|w| chars(w) > 1 && !language.iter().any(|list| list.contains(*w)))
        .collect();
    if content.is_empty() {
        content = words.iter().filter(|w| chars(w) > 1).collect();
    }
    if content.is_empty() {
        content = words.iter().collect();
    }
    let mut fragments = Vec::new();
    for word in content.iter().filter(|w| chars(w) >= MIN_FRAGMENT_CHARS) {
        push_unique(&mut fragments, word.to_string());
    }
    for run in &runs {
        let run: Vec<char> = run.chars().collect();
        for gram in run.windows(3) {
            push_unique(&mut fragments, gram.iter().collect());
        }
    }
    fragments.truncate(MAX_FRAGMENTS);
    let terms = content
        .into_iter()
        .map(|w| Term {
            prefix: chars(w) >= MIN_PREFIX_CHARS,
            text: w.clone(),
        })
        .collect();
    Analysis {
        terms,
        runs,
        fragments,
    }
}

/// The trigram index over the same text as memories_fts, created and filled once.
pub fn init(conn: &Connection) -> Result<(), String> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = 'memories_tri')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("Check trigram index: {e}"))?;
    if exists {
        return Ok(());
    }
    conn.execute_batch(
        "BEGIN IMMEDIATE;
        CREATE VIRTUAL TABLE memories_tri USING fts5(
            title, body, tags,
            content='docs', content_rowid='rowid', tokenize='trigram'
        );
        CREATE TRIGGER docs_tri_ai AFTER INSERT ON docs BEGIN
            INSERT INTO memories_tri(rowid, title, body, tags)
            VALUES (new.rowid, new.title, new.body, new.tags);
        END;
        CREATE TRIGGER docs_tri_ad AFTER DELETE ON docs BEGIN
            INSERT INTO memories_tri(memories_tri, rowid, title, body, tags)
            VALUES ('delete', old.rowid, old.title, old.body, old.tags);
        END;
        CREATE TRIGGER docs_tri_au AFTER UPDATE ON docs BEGIN
            INSERT INTO memories_tri(memories_tri, rowid, title, body, tags)
            VALUES ('delete', old.rowid, old.title, old.body, old.tags);
            INSERT INTO memories_tri(rowid, title, body, tags)
            VALUES (new.rowid, new.title, new.body, new.tags);
        END;
        INSERT INTO memories_tri(memories_tri) VALUES ('rebuild');
        COMMIT;",
    )
    .map_err(|e| format!("Create trigram index: {e}"))
}

/// Memory ids in one scope matching an FTS5 query on `table`, best BM25 first.
fn hits(
    conn: &Connection,
    table: &str,
    expr: &str,
    scope: &str,
) -> Result<Vec<(String, f64)>, String> {
    let sql = format!(
        "SELECT m.id, bm25({table}, 5.0, 1.0, 3.0) AS score
           FROM {table}
           JOIN docs d ON d.rowid = {table}.rowid
           JOIN memories m ON m.id = d.memory_id
          WHERE {table} MATCH ?1 AND m.scope = ?2
          ORDER BY score, m.id
          LIMIT ?3"
    );
    let mut stmt = conn
        .prepare_cached(&sql)
        .map_err(|e| format!("Prepare {table}: {e}"))?;
    let rows = stmt
        .query_map(params![expr, scope, MAX_HITS], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .map_err(|e| format!("Query {table}: {e}"))?;
    rows.collect::<Result<_, _>>()
        .map_err(|e| format!("Read {table}: {e}"))
}

/// Ranked memory ids per lexical signal, best first, for reciprocal-rank fusion:
/// memories with every content word, memories with any, and memories only the trigram
/// index finds.
pub fn signals(
    conn: &Connection,
    query: &str,
    scopes: &[String],
    limit: usize,
) -> Result<Vec<(&'static str, Vec<String>)>, String> {
    let analysis = analyze(query);
    let mut out = Vec::new();
    let mut found: HashSet<String> = HashSet::new();
    if !analysis.terms.is_empty() {
        let any = analysis
            .terms
            .iter()
            .map(Term::expr)
            .collect::<Vec<_>>()
            .join(" OR ");
        // Per memory: how many content words it contains, and BM25 of the whole query.
        let mut coverage: HashMap<String, (usize, f64)> = HashMap::new();
        for scope in scopes {
            for (id, score) in hits(conn, "memories_fts", &any, scope)? {
                coverage.insert(id, (0, score));
            }
            for term in &analysis.terms {
                for (id, _) in hits(conn, "memories_fts", &term.expr(), scope)? {
                    if let Some(entry) = coverage.get_mut(&id) {
                        entry.0 += 1;
                    }
                }
            }
        }
        // Counting matched words first measured worse than BM25 alone (long memories
        // contain many words by chance), so counts only decide the all-terms list.
        let mut ranked: Vec<(String, usize, f64)> = coverage
            .into_iter()
            .map(|(id, (n, score))| (id, n, score))
            .collect();
        ranked.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        found = ranked.iter().map(|r| r.0.clone()).collect();
        let every = analysis.terms.len();
        if every > 1 {
            let all: Vec<String> = ranked
                .iter()
                .filter(|r| r.1 == every)
                .take(limit)
                .map(|r| r.0.clone())
                .collect();
            if !all.is_empty() {
                out.push((ALL_TERMS, all));
            }
        }
        if !ranked.is_empty() {
            out.push((
                SOME_TERMS,
                ranked.into_iter().take(limit).map(|r| r.0).collect(),
            ));
        }
    }
    if !analysis.fragments.is_empty() {
        let expr = analysis
            .fragments
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut extra = Vec::new();
        for scope in scopes {
            extra.extend(
                hits(conn, "memories_tri", &expr, scope)?
                    .into_iter()
                    .filter(|(id, _)| !found.contains(id)),
            );
        }
        extra.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
        if !extra.is_empty() {
            out.push((
                SUBSTRING,
                extra.into_iter().take(limit).map(|e| e.0).collect(),
            ));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(query: &str) -> Vec<String> {
        analyze(query).terms.into_iter().map(|t| t.expr()).collect()
    }

    #[test]
    fn the_query_language_loses_its_function_words() {
        assert_eq!(
            terms("Nach dem Update startet die App nur im Tray"),
            ["\"update\"*", "\"startet\"*", "\"app\"", "\"tray\"*"]
        );
        assert_eq!(
            terms("why did the backtest button not work"),
            [
                "\"why\"",
                "\"backtest\"*",
                "\"button\"*",
                "\"not\"",
                "\"work\"*"
            ]
        );
        // German content words the German list calls stopwords survive.
        assert!(terms("wie funktioniert die Suche im Gedächtnis").contains(&"\"suche\"*".into()));
    }

    #[test]
    fn one_foreign_stopword_does_not_make_a_language() {
        // "local" and "final" are stopwords in no kept list or only once: nothing is dropped.
        assert_eq!(
            terms("final local build"),
            ["\"final\"*", "\"local\"*", "\"build\"*"]
        );
        assert_eq!(terms("die"), ["\"die\""]);
        assert_eq!(terms("die im"), ["\"die\"", "\"im\""]);
    }

    #[test]
    fn short_words_match_exactly_and_single_letters_go() {
        assert_eq!(
            terms("version 1.0.6 of the app"),
            ["\"version\"*", "\"app\""]
        );
        assert_eq!(terms("port 1420"), ["\"port\"*", "\"1420\"*"]);
    }

    #[test]
    fn unspaced_scripts_become_trigrams() {
        let a = analyze("Codex のトークン使用量");
        assert_eq!(
            a.terms.iter().map(Term::expr).collect::<Vec<_>>(),
            ["\"codex\"*"]
        );
        assert_eq!(a.runs, ["のトークン使用量"]);
        assert!(a.fragments.contains(&"トーク".to_string()));
        assert!(a.fragments.contains(&"codex".to_string()));
        assert!(analyze("理由").fragments.is_empty());
        // Thai vowel and tone marks stay inside the run.
        assert_eq!(analyze("ภาษาไทย").runs, ["ภาษาไทย"]);
    }

    #[test]
    fn trigram_index_is_created_for_an_existing_database() {
        let conn = Connection::open_in_memory().unwrap();
        crate::index::init_schema(&conn).unwrap();
        conn.execute_batch(
            "DROP TRIGGER docs_tri_ai; DROP TRIGGER docs_tri_ad; DROP TRIGGER docs_tri_au;
             DROP TABLE memories_tri;",
        )
        .unwrap();
        let text = "---\nid: m1\n---\n# Gedächtnissuche\nDie Gedächtnissuche nutzt BM25.";
        let file = crate::index::FileRecord {
            scope: "a",
            rel_path: "m1.md",
            text,
            mtime_ms: 1,
        };
        crate::index::upsert_from_file(&conn, &file).unwrap();
        crate::index::init_schema(&conn).unwrap();
        let signals = signals(&conn, "Suche", &["a".into()], 10).unwrap();
        assert_eq!(signals, [(SUBSTRING, vec!["m1".to_string()])]);
    }
}
