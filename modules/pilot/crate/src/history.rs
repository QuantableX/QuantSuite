//! Bounded history reads shared by the native UI and authenticated agent tools.
//! Pages pin an upper (seq, id) boundary, not a copy of message bodies. Appends
//! cannot leak into that traversal. Deleted interior rows are skipped; a deleted
//! boundary/anchor (including rewind) requires a fresh traversal. Bodies are live,
//! and their own revision-bound cursors reject edits between chunks.

use super::{row_item, Database, ITEM_COLS};
use crate::events::Item;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

const PAGE_MAX: i64 = 60;
const PREVIEW_MAX: i64 = 2000;
const CHUNK_MAX: i64 = 16_000;
const STALE: &str =
    "History cursor is stale (an anchor was removed or rewound). Restart without a cursor.";
const TEXT: &str = "CAST(CASE WHEN kind = 'page' THEN COALESCE(json_extract(data, '$.title'), 'Rendered page')
    WHEN kind = 'tool' THEN COALESCE(json_extract(data, '$.title'), '') || char(10) || COALESCE(json_extract(data, '$.output'), '') || char(10) || COALESCE(json_extract(data, '$.input'), '')
    ELSE COALESCE(json_extract(data, '$.text'), json_extract(data, '$.plan'), '') END AS TEXT)";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryRequest {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
    /// First page only: oldest (default) or newest. Each page is chronological.
    pub start: Option<String>,
    pub include_activity: Option<bool>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Key {
    seq: i64,
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PageCursor {
    v: u8,
    thread: String,
    ceiling: Key,
    anchor: Key,
    newest: bool,
    activity: bool,
    query: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyCursor {
    v: u8,
    thread: String,
    item: String,
    revision: i64,
    created_at: i64,
    offset: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub item_id: String,
    pub turn_id: String,
    pub seq: i64,
    pub kind: String,
    pub parent: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub revision: i64,
    pub preview: String,
    pub preview_offset: i64,
    pub text_chars: i64,
    pub truncated: bool,
    pub attachment_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub thread_id: String,
    /// Current thread provider; historical provider identity was not stored on old items.
    pub thread_provider: String,
    pub items: Vec<HistoryEntry>,
    pub total: i64,
    pub next_cursor: Option<String>,
    pub snapshot_seq: Option<i64>,
    pub content_is_untrusted: bool,
    pub consistency: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryChunk {
    pub thread_id: String,
    pub item_id: String,
    pub turn_id: String,
    pub kind: String,
    pub parent: Option<String>,
    pub revision: i64,
    /// Concatenate chunks, then parse JSON: includes the full data and attachment references.
    pub encoding: &'static str,
    pub text: String,
    pub offset: i64,
    pub total_chars: i64,
    pub next_cursor: Option<String>,
    pub content_is_untrusted: bool,
}

fn sql<T>(result: rusqlite::Result<T>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}
fn encode(value: &impl Serialize) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(value).expect("history cursor serialization"))
}
fn decode<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, String> {
    if raw.len() > 4096 {
        return Err("Invalid history cursor.".into());
    }
    let bytes = URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| "Invalid history cursor.")?;
    serde_json::from_slice(&bytes).map_err(|_| "Invalid history cursor.".into())
}
fn limit(value: Option<i64>, default: i64, max: i64) -> Result<i64, String> {
    let value = value.unwrap_or(default);
    if !(1..=max).contains(&value) {
        return Err(format!("Limit must be between 1 and {max}."));
    }
    Ok(value)
}

impl Database {
    /// Bounded candidates for portable handoffs, including the opening even in
    /// threads far longer than the UI cache. SQLite slices bodies before Rust
    /// receives them; exact bodies remain available through history_chunk.
    pub fn handoff_items(&self, thread: &str) -> Result<(Vec<Item>, usize), String> {
        let mut entries = self.history_page(thread, HistoryRequest {
            limit: Some(10), ..Default::default()
        })?.items;
        entries.extend(self.history_page(thread, HistoryRequest {
            limit: Some(20), start: Some("newest".into()), ..Default::default()
        })?.items);
        let mut request = HistoryRequest {
            limit: Some(60), start: Some("newest".into()), include_activity: Some(true), ..Default::default()
        };
        let mut remaining = 200;
        let mut total = 0;
        while remaining > 0 {
            request.limit = Some(remaining.min(60));
            let page = self.history_page(thread, request.clone())?;
            total = page.total as usize;
            remaining -= page.items.len() as i64;
            entries.extend(page.items);
            if page.next_cursor.is_none() { break; }
            request.cursor = page.next_cursor;
        }
        entries.sort_by(|a,b| a.seq.cmp(&b.seq).then_with(|| a.item_id.cmp(&b.item_id)));
        entries.dedup_by(|a,b| a.item_id == b.item_id);
        let conn = self.lock();
        let mut items = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(kind) = crate::events::ItemKind::parse(&entry.kind) else { continue; };
            let tail_len = (entry.text_chars - PREVIEW_MAX).clamp(0, 500);
            let (tail, provider): (String, Option<String>) = sql(conn.query_row(
                &format!("SELECT CASE WHEN ?3=0 THEN '' ELSE substr({TEXT}, -?3) END, json_extract(data, '$.provider') FROM timeline WHERE thread_id=?1 AND id=?2"),
                params![thread,entry.item_id,tail_len], |r| Ok((r.get(0)?,r.get(1)?))
            ))?;
            let shortened = entry.text_chars > PREVIEW_MAX + 500;
            let text = format!("{}{}{}",entry.preview,if shortened { " …[omitted]… " } else { "" },tail);
            items.push(Item {
                id:entry.item_id,thread_id:thread.into(),turn_id:entry.turn_id,seq:entry.seq,kind,parent:entry.parent,
                data:serde_json::json!({"text":text,"originalChars":entry.text_chars,"shortened":shortened,"attachmentCount":entry.attachment_count,"provider":provider}),
                created_at:entry.created_at,updated_at:entry.updated_at
            });
        }
        Ok((items,total))
    }

    pub fn history_page(
        &self,
        thread: &str,
        request: HistoryRequest,
    ) -> Result<HistoryPage, String> {
        let count = limit(request.limit, 20, PAGE_MAX)?;
        if request
            .query
            .as_ref()
            .is_some_and(|q| q.trim().is_empty() || q.chars().count() > 256)
        {
            return Err("Search query must contain 1–256 characters.".into());
        }
        if request
            .start
            .as_deref()
            .is_some_and(|s| s != "oldest" && s != "newest")
        {
            return Err("Start must be oldest or newest.".into());
        }
        let conn = self.lock();
        let provider: String = sql(conn
            .query_row(
                "SELECT provider FROM threads WHERE id = ?1",
                [thread],
                |r| r.get(0),
            )
            .optional())?
        .ok_or("Thread was deleted.")?;
        let mut cursor = if let Some(raw) = &request.cursor {
            let c: PageCursor = decode(raw)?;
            if c.v != 1
                || c.thread != thread
                || c.anchor.seq < 0
                || c.ceiling.seq < 0
                || c.query.chars().count() > 256
            {
                return Err("Invalid history cursor for this thread.".into());
            }
            if request
                .start
                .as_ref()
                .is_some_and(|s| (s == "newest") != c.newest)
                || request.include_activity.is_some_and(|a| a != c.activity)
                || request.query.as_ref().is_some_and(|q| q.trim() != c.query)
            {
                return Err("Cursor filters cannot change; restart without a cursor.".into());
            }
            for key in [&c.ceiling, &c.anchor] {
                if !sql(conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM timeline WHERE thread_id=?1 AND seq=?2 AND id=?3)",
                    params![thread, key.seq, key.id],
                    |r| r.get::<_, bool>(0),
                ))? {
                    return Err(STALE.into());
                }
            }
            if (c.anchor.seq, &c.anchor.id) > (c.ceiling.seq, &c.ceiling.id) {
                return Err("Invalid history cursor boundary.".into());
            }
            c
        } else {
            let ceiling = sql(conn.query_row("SELECT seq, id FROM timeline WHERE thread_id=?1 ORDER BY seq DESC, id DESC LIMIT 1", [thread], |r| Ok(Key { seq: r.get(0)?, id: r.get(1)? })).optional())?;
            let ceiling = ceiling.unwrap_or(Key {
                seq: 0,
                id: String::new(),
            });
            let newest = request.start.as_deref() == Some("newest");
            PageCursor {
                v: 1,
                thread: thread.into(),
                anchor: if newest {
                    ceiling.clone()
                } else {
                    Key {
                        seq: -1,
                        id: String::new(),
                    }
                },
                ceiling,
                newest,
                activity: request.include_activity.unwrap_or(false),
                query: request.query.as_deref().unwrap_or("").trim().into(),
            }
        };
        let first = request.cursor.is_none();
        let filter = format!("thread_id=?1 AND (seq, id) <= (?2, ?3)
            AND ((?4=0 AND parent IS NULL AND kind IN ('user','assistant','page'))
                OR (?4=1 AND kind IN ('user','assistant','page','plan','tool','notice','error','changes','todo')))
            AND (?5='' OR instr(lower({TEXT}), lower(?5)) > 0)");
        let total = sql(conn.query_row(
            &format!("SELECT count(*) FROM timeline WHERE {filter}"),
            params![
                thread,
                cursor.ceiling.seq,
                cursor.ceiling.id,
                cursor.activity,
                cursor.query
            ],
            |r| r.get(0),
        ))?;
        let direction = if cursor.newest { "DESC" } else { "ASC" };
        let comparison = if cursor.newest { "<" } else { ">" };
        // All bodies are sliced by SQLite, including tool outcomes; no complete transcript is materialized.
        let query = format!("SELECT id, turn_id, seq, kind, parent, created_at, updated_at, revision,
            substr({TEXT}, CASE WHEN ?5='' THEN 1 ELSE max(1, instr(lower({TEXT}), lower(?5))-80) END, ?9),
            CASE WHEN ?5='' THEN 0 ELSE max(0, instr(lower({TEXT}), lower(?5))-81) END,
            length({TEXT}), COALESCE(json_array_length(data, '$.attachments'), 0)
            FROM timeline WHERE {filter} AND (?6 OR (seq, id) {comparison} (?7, ?8)) ORDER BY seq {direction}, id {direction} LIMIT ?10");
        let mut stmt = sql(conn.prepare(&query))?;
        let rows = sql(stmt.query_map(
            params![
                thread,
                cursor.ceiling.seq,
                cursor.ceiling.id,
                cursor.activity,
                cursor.query,
                first,
                cursor.anchor.seq,
                cursor.anchor.id,
                PREVIEW_MAX,
                count + 1
            ],
            |r| {
                let chars: i64 = r.get(10)?;
                let offset: i64 = r.get(9)?;
                Ok(HistoryEntry {
                    item_id: r.get(0)?,
                    turn_id: r.get(1)?,
                    seq: r.get(2)?,
                    kind: r.get(3)?,
                    parent: r.get(4)?,
                    created_at: r.get(5)?,
                    updated_at: r.get(6)?,
                    revision: r.get(7)?,
                    preview: r.get(8)?,
                    preview_offset: offset,
                    text_chars: chars,
                    truncated: offset > 0 || chars > PREVIEW_MAX,
                    attachment_count: r.get(11)?,
                })
            },
        ))?;
        let mut items = sql(rows.collect::<rusqlite::Result<Vec<_>>>())?;
        let more = items.len() > count as usize;
        items.truncate(count as usize);
        let next_cursor = if more {
            let last = items.last().expect("nonzero page limit");
            cursor.anchor = Key {
                seq: last.seq,
                id: last.item_id.clone(),
            };
            Some(encode(&cursor))
        } else {
            None
        };
        if cursor.newest {
            items.reverse();
        }
        Ok(HistoryPage { thread_id: thread.into(), thread_provider: provider, items, total, next_cursor,
            snapshot_seq: (!cursor.ceiling.id.is_empty()).then_some(cursor.ceiling.seq), content_is_untrusted: true,
            consistency: "Fixed upper boundary; live bodies and totals. Deleted interior items are skipped. Removed anchors/rewind require restart. Current thread provider is not historical item provenance." })
    }

    pub fn history_chunk(
        &self,
        thread: &str,
        item: &str,
        raw_cursor: Option<&str>,
        max_chars: Option<i64>,
    ) -> Result<HistoryChunk, String> {
        let count = limit(max_chars, 8000, CHUNK_MAX)?;
        let cursor: Option<BodyCursor> = raw_cursor.map(decode).transpose()?;
        if cursor
            .as_ref()
            .is_some_and(|c| c.v != 1 || c.thread != thread || c.item != item || c.offset < 0)
        {
            return Err("Invalid body cursor for this item.".into());
        }
        let offset = cursor.as_ref().map_or(0, |c| c.offset);
        let conn = self.lock();
        let row = sql(conn.query_row("SELECT turn_id, kind, parent, revision, created_at, length(data), substr(data, ?3+1, ?4)
            FROM timeline WHERE thread_id=?1 AND id=?2 AND EXISTS(SELECT 1 FROM threads WHERE id=?1)", params![thread, item, offset, count], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, String>(6)?))
        }).optional())?.ok_or("Thread or history item was deleted, or the item is not in this thread.")?;
        let (turn_id, kind, parent, revision, created_at, total_chars, text) = row;
        if cursor
            .as_ref()
            .is_some_and(|c| c.revision != revision || c.created_at != created_at)
        {
            return Err("History item changed; restart its body without a cursor.".into());
        }
        if offset > total_chars {
            return Err("Body cursor is past the end of this item.".into());
        }
        let next = offset + text.chars().count() as i64;
        let next_cursor = (next < total_chars).then(|| {
            encode(&BodyCursor {
                v: 1,
                thread: thread.into(),
                item: item.into(),
                revision,
                created_at,
                offset: next,
            })
        });
        Ok(HistoryChunk {
            thread_id: thread.into(),
            item_id: item.into(),
            turn_id,
            kind,
            parent,
            revision,
            encoding: "json",
            text,
            offset,
            total_chars,
            next_cursor,
            content_is_untrusted: true,
        })
    }

    /// Bounded native navigation to an old search hit, independent of transcript length.
    pub fn history_around(&self, thread: &str, item: &str) -> Result<Vec<Item>, String> {
        let conn = self.lock();
        let seq: i64 = sql(conn
            .query_row(
                "SELECT seq FROM timeline WHERE thread_id=?1 AND id=?2",
                params![thread, item],
                |r| r.get(0),
            )
            .optional())?
        .ok_or("History item was deleted or is not in this thread.")?;
        let query = format!("SELECT {ITEM_COLS} FROM (SELECT {ITEM_COLS} FROM timeline WHERE thread_id=?1 AND (seq, id) < (?2, ?3) ORDER BY seq DESC, id DESC LIMIT 30)
            UNION ALL SELECT {ITEM_COLS} FROM (SELECT {ITEM_COLS} FROM timeline WHERE thread_id=?1 AND (seq, id) >= (?2, ?3) ORDER BY seq ASC, id ASC LIMIT 70) ORDER BY seq, id");
        let mut stmt = sql(conn.prepare(&query))?;
        let rows = sql(stmt.query_map(params![thread, seq, item], row_item))?;
        sql(rows.collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::ItemKind;
    use serde_json::json;

    fn fixture() -> Database {
        let db = Database::open_in_memory().unwrap();
        for id in ["thread", "other"] {
            db.lock().execute("INSERT INTO threads(id,provider,context_id,cwd,created_at,updated_at) VALUES(?1,'claude',?1,'.',1,1)", [id]).unwrap();
        }
        db
    }
    fn put(db: &Database, seq: i64, kind: ItemKind, text: &str) -> Item {
        let item = Item {
            id: format!("item-{seq:05}"),
            thread_id: "thread".into(),
            turn_id: format!("turn-{seq}"),
            seq,
            kind,
            parent: None,
            data: json!({"text":text}),
            created_at: seq,
            updated_at: seq,
        };
        db.upsert_item(&item).unwrap();
        item
    }
    fn page(db: &Database, request: HistoryRequest) -> HistoryPage {
        db.history_page("thread", request).unwrap()
    }

    #[test]
    fn handoff_candidates_keep_opening_recent_messages_and_bounded_unicode_ends() {
        let db = fixture();
        let mut opening = put(&db, 1, ItemKind::User, &format!("OBJECTIVE{}CONSTRAINT", "世界🚀".repeat(30_000)));
        opening.data["attachments"] = json!([{"path":"saved.png"}]);
        opening.data["provider"] = json!("claude");
        db.upsert_item(&opening).unwrap();
        for n in 2..=5100 { put(&db, n, ItemKind::Assistant, "message"); }
        put(&db, 5101, ItemKind::User, "recent request");
        for n in 5102..=5400 { put(&db, n, ItemKind::Tool, "activity"); }
        put(&db, 5401, ItemKind::Reasoning, "private reasoning");
        let (items,total) = db.handoff_items("thread").unwrap();
        assert!(items.len() <= 230);
        assert_eq!(total, 5400);
        assert_eq!(items[0].id, opening.id);
        let text = items[0].data["text"].as_str().unwrap();
        assert!(text.starts_with("OBJECTIVE") && text.ends_with("CONSTRAINT"));
        assert!(text.chars().count() < 2600);
        assert_eq!(items[0].data["attachmentCount"],1);
        assert_eq!(items[0].data["provider"],"claude");
        assert!(items.iter().any(|i| i.seq==5101));
        assert!(items.iter().all(|i| i.kind!=ItemKind::Reasoning));
        let packed = crate::lifecycle::pack_history("thread", &items,total,crate::lifecycle::history_budget("continue",0,None,None)).unwrap();
        assert!(packed.json.contains("recent request") && packed.json.contains("CONSTRAINT"));
    }

    #[test]
    fn traverses_all_old_messages_and_excludes_concurrent_appends() {
        let db = fixture();
        for n in 1..=905 {
            put(&db, n, ItemKind::User, &format!("constraint {n}"));
        }
        let mut request = HistoryRequest {
            limit: Some(60),
            ..Default::default()
        };
        let mut seen = Vec::new();
        loop {
            let result = page(&db, request.clone());
            assert_eq!(result.total, 905);
            assert!(result.items.len() <= 60);
            seen.extend(result.items.iter().map(|item| item.seq));
            put(&db, 906, ItemKind::Assistant, "new live message");
            if result.next_cursor.is_none() {
                break;
            }
            request.cursor = result.next_cursor;
        }
        assert_eq!(seen, (1..=905).collect::<Vec<_>>());
        let newest = page(
            &db,
            HistoryRequest {
                limit: Some(2),
                start: Some("newest".into()),
                ..Default::default()
            },
        );
        assert_eq!(
            newest.items.iter().map(|i| i.seq).collect::<Vec<_>>(),
            vec![905, 906]
        );
        let older = page(
            &db,
            HistoryRequest {
                limit: Some(2),
                cursor: newest.next_cursor,
                ..Default::default()
            },
        );
        assert_eq!(
            older.items.iter().map(|i| i.seq).collect::<Vec<_>>(),
            vec![903, 904]
        );
    }

    #[test]
    fn messages_are_not_hidden_behind_six_hundred_tool_items() {
        let db = fixture();
        put(&db, 1, ItemKind::User, "old constraint");
        for n in 2..=750 {
            put(&db, n, ItemKind::Tool, "activity");
        }
        let result = page(
            &db,
            HistoryRequest {
                start: Some("newest".into()),
                ..Default::default()
            },
        );
        assert_eq!(result.total, 1);
        assert_eq!(result.items[0].preview, "old constraint");
    }

    #[test]
    fn unicode_json_body_round_trips_with_attachments_and_detects_same_timestamp_edits() {
        let db = fixture();
        let mut item = put(
            &db,
            1,
            ItemKind::User,
            &"😀 café 文 e\u{301}\n\0".repeat(10_000),
        );
        item.data["attachments"] = json!([{ "path":"saved/image.png", "name":"图.png" }]);
        db.upsert_item(&item).unwrap();
        let mut cursor = None;
        let mut data = String::new();
        loop {
            let chunk = db
                .history_chunk("thread", &item.id, cursor.as_deref(), Some(503))
                .unwrap();
            assert!(chunk.text.chars().count() <= 503);
            data.push_str(&chunk.text);
            cursor = chunk.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&data).unwrap(),
            item.data
        );
        let first = db
            .history_chunk("thread", &item.id, None, Some(20))
            .unwrap();
        item.data["text"] = json!("changed while reading");
        db.upsert_item(&item).unwrap();
        assert!(db
            .history_chunk("thread", &item.id, first.next_cursor.as_deref(), None)
            .unwrap_err()
            .contains("changed"));
        assert!(db.history_chunk("other", &item.id, None, None).is_err());
    }

    #[test]
    fn search_returns_old_constraints_tool_outcomes_and_parent_attribution() {
        let db = fixture();
        put(&db, 1, ItemKind::User, "literal 20%_ constraint");
        for n in 2..=650 {
            put(&db, n, ItemKind::Assistant, "later");
        }
        let mut tool = put(&db, 651, ItemKind::Tool, "");
        tool.parent = Some("subagent-call".into());
        tool.data = json!({"title":"compiler", "input":{"command":"build"}, "output":"old build failed: missing symbol"});
        db.upsert_item(&tool).unwrap();
        let found = page(
            &db,
            HistoryRequest {
                query: Some("20%_".into()),
                ..Default::default()
            },
        );
        assert_eq!(found.items[0].seq, 1);
        assert_eq!(
            page(
                &db,
                HistoryRequest {
                    query: Some("missing symbol".into()),
                    ..Default::default()
                }
            )
            .total,
            0
        );
        let found = page(
            &db,
            HistoryRequest {
                query: Some("missing symbol".into()),
                include_activity: Some(true),
                ..Default::default()
            },
        );
        assert_eq!(found.items[0].parent.as_deref(), Some("subagent-call"));
        assert_eq!(found.items[0].turn_id, "turn-651");
        assert!(found.items[0].preview.contains("missing symbol"));
        let nearby = db
            .history_around("thread", &found.items[0].item_id)
            .unwrap();
        assert!(nearby.len() <= 100 && nearby.iter().any(|i| i.id == tool.id));
    }

    #[test]
    fn deleted_interior_rows_are_skipped_and_removed_anchors_require_restart() {
        let db = fixture();
        for n in 1..=10 {
            put(&db, n, ItemKind::User, "x");
        }
        let first = page(
            &db,
            HistoryRequest {
                limit: Some(2),
                ..Default::default()
            },
        );
        db.lock()
            .execute("DELETE FROM timeline WHERE seq=5", [])
            .unwrap();
        let remaining = page(
            &db,
            HistoryRequest {
                cursor: first.next_cursor.clone(),
                ..Default::default()
            },
        );
        assert_eq!(remaining.total, 9);
        assert!(!remaining.items.iter().any(|i| i.seq == 5));
        db.lock()
            .execute("DELETE FROM timeline WHERE seq=10", [])
            .unwrap();
        assert!(db
            .history_page(
                "thread",
                HistoryRequest {
                    cursor: first.next_cursor,
                    ..Default::default()
                }
            )
            .unwrap_err()
            .contains("stale"));
        let first = page(
            &db,
            HistoryRequest {
                limit: Some(2),
                ..Default::default()
            },
        );
        db.lock()
            .execute("DELETE FROM timeline WHERE seq=2", [])
            .unwrap();
        assert!(db
            .history_page(
                "thread",
                HistoryRequest {
                    cursor: first.next_cursor,
                    ..Default::default()
                }
            )
            .is_err());
    }

    #[test]
    fn invalid_limits_cursors_filters_and_cross_thread_continuations_are_rejected() {
        let db = fixture();
        put(&db, 1, ItemKind::User, "first");
        put(&db, 2, ItemKind::User, "second");
        for count in [0, -1, 61, i64::MAX] {
            assert!(db
                .history_page(
                    "thread",
                    HistoryRequest {
                        limit: Some(count),
                        ..Default::default()
                    }
                )
                .is_err());
        }
        for raw in ["not-a-cursor".into(), "x".repeat(5000)] {
            assert!(db
                .history_page(
                    "thread",
                    HistoryRequest {
                        cursor: Some(raw),
                        ..Default::default()
                    }
                )
                .is_err());
        }
        let first = page(
            &db,
            HistoryRequest {
                limit: Some(1),
                ..Default::default()
            },
        );
        assert!(db
            .history_page(
                "other",
                HistoryRequest {
                    cursor: first.next_cursor.clone(),
                    ..Default::default()
                }
            )
            .is_err());
        assert!(db
            .history_page(
                "thread",
                HistoryRequest {
                    cursor: first.next_cursor,
                    include_activity: Some(true),
                    ..Default::default()
                }
            )
            .is_err());
        assert!(db
            .history_page(
                "thread",
                HistoryRequest {
                    query: Some("x".repeat(257)),
                    ..Default::default()
                }
            )
            .is_err());
        assert!(db
            .history_chunk("thread", "item-00001", None, Some(16_001))
            .is_err());
        db.delete_session("thread").unwrap();
        assert!(db
            .history_page("thread", HistoryRequest::default())
            .unwrap_err()
            .contains("deleted"));
    }

    #[test]
    fn schema_eleven_migration_keeps_items_and_installs_revision_tracking() {
        let db = fixture();
        let mut item = put(&db, 1, ItemKind::User, "preserve me");
        {
            let conn = db.lock();
            conn.execute_batch("DROP TRIGGER timeline_body_revision; ALTER TABLE timeline DROP COLUMN revision; PRAGMA user_version=11;").unwrap();
            super::super::migrate(&conn).unwrap();
        }
        assert_eq!(db.get_item(&item.id).unwrap().unwrap(), item);
        assert_eq!(
            db.history_chunk("thread", &item.id, None, None)
                .unwrap()
                .revision,
            0
        );
        item.data["text"] = json!("edited");
        db.upsert_item(&item).unwrap();
        assert_eq!(
            db.history_chunk("thread", &item.id, None, None)
                .unwrap()
                .revision,
            1
        );
        let conn = db.lock();
        let mut stmt = conn.prepare("EXPLAIN QUERY PLAN SELECT id FROM timeline WHERE thread_id='thread' AND (seq,id)>(1,'') ORDER BY seq,id LIMIT 20").unwrap();
        let detail = stmt
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
            .join(" ");
        assert!(
            detail.contains("timeline_history_order") && detail.contains("SEARCH"),
            "{detail}"
        );
    }
}
