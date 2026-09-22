//! The vault's file format, free of Tauri and of SQLite so it can be unit
//! tested on its own.
//!
//! A memory is one markdown file: optional YAML frontmatter, an H1 title, a
//! body that connects to other memories with `[[Title]]`, `[[Title|alias]]`
//! or `[[Title#Heading]]`. Files are the source of truth — everything in
//! here must survive a round trip through Obsidian or any plain editor.
//!
//! Two rules keep external vaults safe: **reads never mutate** (a file
//! without frontmatter is indexed as-is, under an id derived from its path),
//! and **writes canonicalize** (the first write through QuantMemory adds the
//! frontmatter, carrying the id the index already knew the file by).

use serde_json::{Map, Value};

// ─── Slugs and ids ────────────────────────────────────────────────────────

/// Title → filename stem. Lowercase, ASCII-ish, hyphen-separated. German
/// umlauts transliterate rather than vanish so `Über Fehler` and `Uber
/// Fehler` stay distinguishable as files.
pub fn slugify(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_hyphen = true;
    for c in title.chars() {
        let mapped: &str = match c {
            'ä' | 'Ä' => "ae",
            'ö' | 'Ö' => "oe",
            'ü' | 'Ü' => "ue",
            'ß' => "ss",
            _ => {
                if c.is_ascii_alphanumeric() {
                    out.extend(c.to_lowercase());
                    last_hyphen = false;
                    continue;
                }
                if !last_hyphen {
                    out.push('-');
                    last_hyphen = true;
                }
                continue;
            }
        };
        out.push_str(mapped);
        last_hyphen = false;
    }
    let trimmed = out.trim_matches('-');
    let mut slug: String = trimmed.chars().take(80).collect();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "memory".into()
    } else {
        slug
    }
}

/// First slug (base, base-2, base-3, …) for which `taken` says no.
pub fn unique_slug(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_string();
    }
    for n in 2..10_000 {
        let candidate = format!("{base}-{n}");
        if !taken(&candidate) {
            return candidate;
        }
    }
    // Ten thousand collisions is not a naming problem anymore.
    format!("{base}-{}", uuid_suffix())
}

fn uuid_suffix() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

/// FNV-1a, 64 bit — a stable content stamp for change detection and for the
/// path-derived ids of files the vault only adopted. Not security-relevant.
pub fn fnv1a64(data: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Id for a file that was never written through QuantMemory: derived from the
/// vault-relative path, so re-indexing does not re-identify it. The first
/// write through the module persists this id into the frontmatter.
pub fn external_id(rel_path: &str) -> String {
    format!("ext-{:016x}", fnv1a64(&rel_path.to_lowercase().replace('\\', "/")))
}

// ─── Frontmatter ──────────────────────────────────────────────────────────

/// Split `---\n…\n---\n` off the front. Returns `(yaml, body)`; `yaml` is
/// `None` when the file has no frontmatter block. Tolerates CRLF.
pub fn split_frontmatter(text: &str) -> (Option<&str>, &str) {
    let rest = text.strip_prefix("\u{feff}").unwrap_or(text);
    let after_open = match rest.strip_prefix("---\r\n").or_else(|| rest.strip_prefix("---\n")) {
        Some(after) => after,
        None => return (None, text),
    };
    let mut offset = 0;
    for line in after_open.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let yaml = &after_open[..offset];
            let body = &after_open[offset + line.len()..];
            return (Some(yaml), body.trim_start_matches(['\r', '\n']));
        }
        offset += line.len();
    }
    // An opening fence with no closing fence is body, not frontmatter.
    (None, text)
}

/// Parse a frontmatter block into a JSON object. Anything that is not a
/// mapping — or does not parse — is treated as "no frontmatter" rather than
/// as an error, because a broken header must never make a memory unreadable.
pub fn parse_frontmatter(yaml: &str) -> Option<Map<String, Value>> {
    let parsed: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).ok()?;
    match serde_json::to_value(parsed).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// A scalar that can stand unquoted in YAML. Anything else — spaces, colons,
/// leading specials, things that would read as bool/number — gets quoted.
fn yaml_scalar(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '+'))
        && !s.starts_with(['-', '?', ':', '.'])
        && !matches!(s.to_ascii_lowercase().as_str(), "true" | "false" | "null" | "yes" | "no")
        && s.parse::<f64>().is_err();
    if plain {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn value_to_yaml_inline(value: &Value) -> String {
    match value {
        Value::String(s) => yaml_scalar(s),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "null".into(),
        other => {
            // Nested structures are rare in frontmatter; JSON is valid YAML.
            serde_json::to_string(other).unwrap_or_else(|_| "null".into())
        }
    }
}

/// The canonical keys, in the order they are written. Everything else a file
/// carries (Obsidian `aliases:`, custom fields) is preserved after them —
/// a QuantMemory write must never eat another tool's metadata.
const CANONICAL_KEYS: [&str; 6] = ["id", "type", "tags", "author", "created", "updated"];

pub fn render_frontmatter(map: &Map<String, Value>) -> String {
    let mut out = String::from("---\n");
    for key in CANONICAL_KEYS {
        let Some(value) = map.get(key) else { continue };
        if value.is_null() {
            continue;
        }
        if key == "tags" {
            let tags: Vec<String> = tags_from_value(value);
            out.push_str(&format!(
                "tags: [{}]\n",
                tags.iter().map(|t| yaml_scalar(t)).collect::<Vec<_>>().join(", ")
            ));
        } else {
            out.push_str(&format!("{key}: {}\n", value_to_yaml_inline(value)));
        }
    }
    let mut extras: Vec<(&String, &Value)> =
        map.iter().filter(|(k, _)| !CANONICAL_KEYS.contains(&k.as_str())).collect();
    extras.sort_by_key(|(k, _)| k.as_str());
    for (key, value) in extras {
        out.push_str(&format!("{key}: {}\n", value_to_yaml_inline(value)));
    }
    out.push_str("---\n");
    out
}

/// `tags:` as written by hand comes in many shapes — a list, one string,
/// a comma-joined string. All of them become a clean Vec.
pub fn tags_from_value(value: &Value) -> Vec<String> {
    let raw: Vec<String> = match value {
        Value::Array(items) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
            .collect(),
        Value::String(s) => s.split(',').map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let mut tags: Vec<String> = raw
        .iter()
        .map(|t| t.trim().trim_start_matches('#').to_string())
        .filter(|t| !t.is_empty())
        .collect();
    tags.dedup();
    tags
}

/// Frontmatter + body → the file's full text, normalized to exactly one
/// blank line between the two and a trailing newline.
pub fn compose(frontmatter: &Map<String, Value>, body: &str) -> String {
    let mut out = render_frontmatter(frontmatter);
    out.push('\n');
    out.push_str(body.trim_start_matches(['\r', '\n']).trim_end());
    out.push('\n');
    out
}

// ─── Body structure ───────────────────────────────────────────────────────

/// The first `# ` heading outside a code fence — the memory's display title.
pub fn first_h1(body: &str) -> Option<String> {
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some(rest) = line.strip_prefix("# ") {
                let title = rest.trim().to_string();
                if !title.is_empty() {
                    return Some(title);
                }
            }
        }
    }
    None
}

/// Replace the first H1 with `# new_title`, or prepend one when the body has
/// none. Used by rename so the file and its filename keep telling the same
/// story.
pub fn replace_h1(body: &str, new_title: &str) -> String {
    let mut in_fence = false;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        } else if !in_fence && content.starts_with("# ") {
            let mut out = String::with_capacity(body.len() + new_title.len());
            out.push_str(&body[..offset]);
            out.push_str("# ");
            out.push_str(new_title);
            out.push_str(&body[offset + content.len()..]);
            return out;
        }
        offset += line.len();
    }
    format!("# {new_title}\n\n{body}")
}

pub fn word_count(body: &str) -> i64 {
    body.split_whitespace().count() as i64
}

// ─── Wikilinks ────────────────────────────────────────────────────────────

/// One `[[…]]` occurrence, with enough structure to resolve it and enough
/// position to rewrite it.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkRef {
    /// Everything between the brackets, exactly as written.
    pub raw: String,
    /// The target with `#heading` and `|alias` stripped.
    pub target: String,
    pub target_lc: String,
    pub heading: Option<String>,
    pub alias: Option<String>,
    /// Byte span of the whole `[[…]]` in the body.
    pub span: (usize, usize),
}

/// Extract every wikilink, skipping fenced code blocks and inline code —
/// a link inside a code sample is quotation, not connection.
pub fn extract_links(body: &str) -> Vec<LinkRef> {
    let mut links = Vec::new();
    let mut in_fence = false;
    let mut line_start = 0;
    for line in body.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
        } else if !in_fence {
            scan_line(content, line_start, &mut links);
        }
        line_start += line.len();
    }
    links
}

fn scan_line(line: &str, line_start: usize, links: &mut Vec<LinkRef>) {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut in_code = false;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code && bytes[i] == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            if let Some(end) = line[i + 2..].find("]]") {
                let inner = &line[i + 2..i + 2 + end];
                if let Some(link) = parse_link(inner, (line_start + i, line_start + i + 2 + end + 2)) {
                    links.push(link);
                }
                i += 2 + end + 2;
                continue;
            }
        }
        i += 1;
    }
}

fn parse_link(inner: &str, span: (usize, usize)) -> Option<LinkRef> {
    if inner.trim().is_empty() || inner.contains("[[") {
        return None;
    }
    let (target_part, alias) = match inner.split_once('|') {
        Some((t, a)) => (t, Some(a.trim().to_string()).filter(|a| !a.is_empty())),
        None => (inner, None),
    };
    let (target, heading) = match target_part.split_once('#') {
        Some((t, h)) => (t, Some(h.trim().to_string()).filter(|h| !h.is_empty())),
        None => (target_part, None),
    };
    let target = target.trim();
    if target.is_empty() {
        return None;
    }
    Some(LinkRef {
        raw: inner.to_string(),
        target: target.to_string(),
        target_lc: target.to_lowercase(),
        heading,
        alias,
        span,
    })
}

/// Rewrite every link that points at `old_title` (by title or by its slug) to
/// `new_title`, keeping heading and alias intact. Returns the new body and
/// how many links changed.
pub fn rewrite_links(body: &str, old_title: &str, new_title: &str) -> (String, usize) {
    let old_lc = old_title.to_lowercase();
    let old_slug = slugify(old_title);
    let hits: Vec<LinkRef> = extract_links(body)
        .into_iter()
        .filter(|l| l.target_lc == old_lc || l.target_lc == old_slug)
        .collect();
    if hits.is_empty() {
        return (body.to_string(), 0);
    }
    let mut out = body.to_string();
    for link in hits.iter().rev() {
        let mut inner = new_title.to_string();
        if let Some(h) = &link.heading {
            inner.push('#');
            inner.push_str(h);
        }
        if let Some(a) = &link.alias {
            inner.push('|');
            inner.push_str(a);
        }
        out.replace_range(link.span.0..link.span.1, &format!("[[{inner}]]"));
    }
    (out, hits.len())
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn slugs_are_lowercase_hyphenated_and_transliterated() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify("Über Fehler & Co."), "ueber-fehler-co");
        assert_eq!(slugify("  --Weird__ Title!!  "), "weird-title");
        assert_eq!(slugify("äöü ß"), "aeoeue-ss");
    }

    #[test]
    fn an_unusable_title_still_yields_a_filename() {
        assert_eq!(slugify("???"), "memory");
        assert_eq!(slugify(""), "memory");
    }

    #[test]
    fn colliding_slugs_count_up() {
        let existing = ["note", "note-2"];
        assert_eq!(unique_slug("note", |s| existing.contains(&s)), "note-3");
        assert_eq!(unique_slug("fresh", |s| existing.contains(&s)), "fresh");
    }

    #[test]
    fn frontmatter_splits_off_and_survives_crlf() {
        let text = "---\r\nid: abc\r\ntags: [a, b]\r\n---\r\n\r\n# Title\r\nBody";
        let (yaml, body) = split_frontmatter(text);
        let map = parse_frontmatter(yaml.expect("yaml")).expect("map");
        assert_eq!(map.get("id").and_then(|v| v.as_str()), Some("abc"));
        assert!(body.starts_with("# Title"));
    }

    #[test]
    fn a_file_without_frontmatter_is_all_body() {
        let (yaml, body) = split_frontmatter("# Just a note\n\nText");
        assert!(yaml.is_none());
        assert_eq!(body, "# Just a note\n\nText");
    }

    #[test]
    fn an_unclosed_fence_is_body_not_frontmatter() {
        let (yaml, body) = split_frontmatter("---\nid: abc\nno closing fence");
        assert!(yaml.is_none());
        assert!(body.starts_with("---"));
    }

    #[test]
    fn broken_yaml_never_makes_a_memory_unreadable() {
        assert!(parse_frontmatter(": : not yaml [").is_none());
        assert!(parse_frontmatter("just a scalar").is_none());
    }

    /// A write must never eat another tool's metadata: unknown keys survive
    /// the canonical rewrite.
    #[test]
    fn foreign_frontmatter_keys_survive_a_round_trip() {
        let mut map = Map::new();
        map.insert("id".into(), json!("01ABC"));
        map.insert("aliases".into(), json!(["old name"]));
        map.insert("tags".into(), json!(["quant", "mcp"]));
        let rendered = render_frontmatter(&map);
        let (yaml, _) = split_frontmatter(&rendered);
        let reparsed = parse_frontmatter(yaml.expect("fence")).expect("reparse");
        assert_eq!(reparsed.get("id"), map.get("id"));
        assert_eq!(reparsed.get("aliases"), map.get("aliases"));
        assert_eq!(reparsed.get("tags"), map.get("tags"));
    }

    #[test]
    fn scalars_that_would_confuse_yaml_get_quoted() {
        assert_eq!(yaml_scalar("2026-08-31T10:00:00Z"), "2026-08-31T10:00:00Z");
        assert_eq!(yaml_scalar("Claude Code"), "\"Claude Code\"");
        assert_eq!(yaml_scalar("true"), "\"true\"");
        assert_eq!(yaml_scalar("3.14"), "\"3.14\"");
    }

    #[test]
    fn tags_arrive_in_many_shapes() {
        assert_eq!(tags_from_value(&json!(["a", "#b"])), vec!["a", "b"]);
        assert_eq!(tags_from_value(&json!("a, b , ")), vec!["a", "b"]);
        assert_eq!(tags_from_value(&json!(42)), Vec::<String>::new());
    }

    #[test]
    fn links_parse_target_heading_and_alias() {
        let links = extract_links("See [[Other Note#Setup|the setup]] and [[Plain]].");
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].target, "Other Note");
        assert_eq!(links[0].heading.as_deref(), Some("Setup"));
        assert_eq!(links[0].alias.as_deref(), Some("the setup"));
        assert_eq!(links[1].target, "Plain");
        assert_eq!(links[1].alias, None);
    }

    /// A link inside a code sample is quotation, not connection.
    #[test]
    fn links_in_code_do_not_count() {
        let body = "Real: [[Yes]]\n```\n[[No Fence]]\n```\nAnd `[[No Inline]]` too.";
        let links = extract_links(body);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "Yes");
    }

    #[test]
    fn empty_and_nested_brackets_are_not_links() {
        assert!(extract_links("[[]] [[ ]] [[a[[b]]]]").iter().all(|l| l.target == "b"));
    }

    #[test]
    fn rename_rewrites_links_but_keeps_alias_and_heading() {
        let body = "See [[Old Title|the note]] and [[Old Title#Part]] and [[Unrelated]].";
        let (out, n) = rewrite_links(body, "Old Title", "New Title");
        assert_eq!(n, 2);
        assert_eq!(out, "See [[New Title|the note]] and [[New Title#Part]] and [[Unrelated]].");
    }

    #[test]
    fn rename_matches_by_slug_too() {
        let (out, n) = rewrite_links("[[old-title]]", "Old Title", "New Title");
        assert_eq!(n, 1);
        assert_eq!(out, "[[New Title]]");
    }

    #[test]
    fn h1_is_found_outside_fences_and_replaced_in_place() {
        let body = "```\n# not this\n```\n# Real Title\n\nText";
        assert_eq!(first_h1(body).as_deref(), Some("Real Title"));
        let renamed = replace_h1(body, "Better Title");
        assert!(renamed.contains("# Better Title\n"));
        assert!(renamed.contains("# not this"));
    }

    #[test]
    fn a_body_without_h1_gets_one_prepended() {
        assert!(replace_h1("just text", "Title").starts_with("# Title\n\n"));
    }

    #[test]
    fn compose_normalizes_spacing() {
        let mut map = Map::new();
        map.insert("id".into(), json!("x"));
        let text = compose(&map, "\n\n# T\n\nBody\n\n\n");
        assert!(text.starts_with("---\nid: x\n---\n\n# T"));
        assert!(text.ends_with("Body\n"));
    }

    #[test]
    fn external_ids_are_stable_across_separators_and_case() {
        assert_eq!(external_id("sub/Note.md"), external_id("Sub\\note.md"));
        assert_ne!(external_id("a.md"), external_id("b.md"));
    }
}
