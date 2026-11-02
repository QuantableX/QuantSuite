//! Durable drafts and delivery claims. A row survives preparation/launch errors
//! and is consumed only after its associated native turn completes. An app exit
//! during delivery requires an explicit retry instead of an automatic replay.
use super::{now, Database};
use crate::context::{AttachmentArg, Chip};
use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueuePayload {
    pub version: u32,
    pub text: String,
    #[serde(default)]
    pub chips: Vec<Chip>,
    #[serde(default)]
    pub attachments: Vec<AttachmentArg>,
}

impl QueuePayload {
    pub fn text(text: &str) -> Self {
        Self { version: 1, text: text.into(), chips: Vec::new(), attachments: Vec::new() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub thread_id: String,
    pub position: i64,
    /// Compatibility/display convenience; always the unexpanded draft text.
    pub text: String,
    pub payload: QueuePayload,
    pub state: String,
    pub error: Option<String>,
    pub revision: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

const COLS: &str = "id, thread_id, position, text, created_at, updated_at, payload, state, error, revision";
fn row(r: &Row<'_>) -> rusqlite::Result<QueueItem> {
    let text: String = r.get(3)?;
    let payload = match r.get::<_, Option<String>>(6)? {
        Some(raw) => serde_json::from_str(&raw).map_err(|e| rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e)))?,
        None => QueuePayload::text(&text),
    };
    Ok(QueueItem { id:r.get(0)?,thread_id:r.get(1)?,position:r.get(2)?,text,payload,
        created_at:r.get(4)?,updated_at:r.get(5)?,state:r.get(7)?,error:r.get(8)?,revision:r.get(9)? })
}

impl Database {
    pub fn queue(&self, thread: &str) -> rusqlite::Result<Vec<QueueItem>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM queue WHERE thread_id=?1 ORDER BY position,created_at,id"))?;
        let rows = stmt.query_map([thread], row)?;
        rows.collect()
    }

    pub fn queue_item(&self, id: &str) -> rusqlite::Result<Option<QueueItem>> {
        self.lock().query_row(&format!("SELECT {COLS} FROM queue WHERE id=?1"), [id], row).optional()
    }

    pub fn queue_add_payload(&self, thread: &str, payload: &QueuePayload) -> rusqlite::Result<QueueItem> {
        let conn = self.lock();
        let position: i64 = conn.query_row("SELECT COALESCE(MAX(position),0)+1 FROM queue WHERE thread_id=?1", [thread], |r| r.get(0))?;
        let id = uuid::Uuid::new_v4().to_string();
        let stamp = now();
        let raw = serde_json::to_string(payload).expect("queue payload serializes");
        conn.execute("INSERT INTO queue(id,thread_id,position,text,payload,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?6)",
            params![id,thread,position,payload.text,raw,stamp])?;
        conn.query_row(&format!("SELECT {COLS} FROM queue WHERE id=?1"), [id], row)
    }

    pub fn queue_thread(&self, id: &str) -> rusqlite::Result<Option<String>> {
        self.lock().query_row("SELECT thread_id FROM queue WHERE id=?1", [id], |r| r.get(0)).optional()
    }

    pub fn queue_update_payload(&self, id: &str, payload: &QueuePayload, revision: Option<i64>) -> rusqlite::Result<bool> {
        let raw = serde_json::to_string(payload).expect("queue payload serializes");
        Ok(self.lock().execute("UPDATE queue SET text=?2,payload=?3,error=NULL,revision=revision+1,updated_at=?4
            WHERE id=?1 AND state NOT IN ('sending','running','awaiting') AND (?5 IS NULL OR revision=?5)",
            params![id,payload.text,raw,now(),revision])? > 0)
    }

    /// At most one delivery per thread. Keeping the row claimed closes the
    /// enqueue/turn-end/send-now race without deleting and recreating drafts.
    pub fn queue_claim(&self, id: &str, explicit_retry: bool) -> rusqlite::Result<Option<QueueItem>> {
        let conn = self.lock();
        let n = conn.execute("UPDATE queue SET state='sending',error=NULL,turn_id=NULL,updated_at=?3
            WHERE id=?1 AND (state='queued' OR (?2 AND state='uncertain'))
              AND NOT EXISTS(SELECT 1 FROM queue other WHERE other.thread_id=queue.thread_id AND other.state IN ('sending','running','awaiting'))",
            params![id,explicit_retry,now()])?;
        if n == 0 { return Ok(None); }
        conn.query_row(&format!("SELECT {COLS} FROM queue WHERE id=?1"), [id], row).map(Some)
    }

    /// Called before writing the provider prompt, including steering. The
    /// native turn's persisted status reconciles even an immediate completion.
    pub fn queue_bind_turn(&self, id: &str, turn: &str) -> rusqlite::Result<bool> {
        Ok(self.lock().execute("UPDATE queue SET state='running',turn_id=?2 WHERE id=?1 AND state='sending'
            AND NOT EXISTS(SELECT 1 FROM turns WHERE id=?2 AND status!='started')", params![id,turn])? > 0)
    }

    pub fn queue_fail(&self, id: &str, error: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        conn.execute("UPDATE threads SET queue_held=1 WHERE id=(SELECT thread_id FROM queue WHERE id=?1)", [id])?;
        conn.execute("UPDATE queue SET state='queued',turn_id=NULL,error=?2,updated_at=?3 WHERE id=?1", params![id,error,now()])?;
        Ok(())
    }

    pub fn queue_wait_ack(&self, id: &str) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE queue SET state='awaiting' WHERE id=?1 AND state='running'", [id])?;
        Ok(())
    }

    pub fn queue_acknowledge(&self, id: &str) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE queue SET state='running' WHERE id=?1 AND state='awaiting'", [id])?;
        Ok(())
    }

    /// Only the row's own completed turn consumes it. A late callback from an
    /// earlier turn cannot acknowledge a newer in-flight message.
    pub fn queue_settle(&self, thread: &str) -> rusqlite::Result<()> {
        let conn = self.lock();
        let terminal: Option<(String,String,Option<String>)> = conn.query_row(
            "SELECT q.id,t.status,t.error FROM queue q JOIN turns t ON t.id=q.turn_id
             WHERE q.thread_id=?1 AND q.state IN ('running','awaiting') AND t.status!='started'
               AND (q.state='running' OR t.status!='completed') LIMIT 1", [thread],
            |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((id,status,error)) = terminal {
            if status == "completed" {
                conn.execute("DELETE FROM queue WHERE id=?1", [id])?;
            } else {
                let error = error.unwrap_or_else(|| format!("Delivery {status}. Check the conversation before retrying this message."));
                conn.execute("UPDATE queue SET state='queued',turn_id=NULL,error=?2,updated_at=?3 WHERE id=?1",params![id,error,now()])?;
                conn.execute("UPDATE threads SET queue_held=1 WHERE id=?1", [thread])?;
            }
        }
        Ok(())
    }

    pub fn queue_remove(&self, id: &str) -> rusqlite::Result<bool> {
        Ok(self.lock().execute("DELETE FROM queue WHERE id=?1 AND state NOT IN ('sending','running','awaiting')", [id])? > 0)
    }

    #[cfg(test)]
    pub fn queue_add(&self, thread: &str, text: &str) -> rusqlite::Result<QueueItem> {
        self.queue_add_payload(thread, &QueuePayload::text(text))
    }
    #[cfg(test)]
    pub fn queue_update(&self, id: &str, text: &str) -> rusqlite::Result<bool> {
        self.queue_update_payload(id, &QueuePayload::text(text), None)
    }
    #[cfg(test)]
    pub fn queue_take(&self, id: &str) -> rusqlite::Result<Option<QueueItem>> {
        let item = self.queue_item(id)?;
        if !self.queue_remove(id)? { return Ok(None); }
        Ok(item)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn fixture() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.lock().execute("INSERT INTO threads(id,provider,context_id,cwd,created_at,updated_at) VALUES('t','claude','general','.',1,1)",[]).unwrap();
        db
    }
    fn payload() -> QueuePayload {
        QueuePayload { version:1,text:"Keep 世界 and \u{2068}skill\u{2069}".into(),
            chips:vec![Chip { kind:"skill".into(),label:"skill".into(),content:"review".into(),path:Some("skill.md".into()),comment:None }],
            attachments:vec![AttachmentArg { kind:"image".into(),name:"chart.png".into(),path:"saved.png".into(),thumb:Some("small preview".into()) }] }
    }
    fn turn(db: &Database, id: &str, status: &str) {
        db.lock().execute("INSERT INTO turns(id,thread_id,status,started_at) VALUES(?1,'t',?2,1)",params![id,status]).unwrap();
    }

    #[test]
    fn payload_edits_keep_identity_order_and_reject_stale_or_sending_updates() {
        let db=fixture();
        let a=db.queue_add_payload("t",&payload()).unwrap();
        let b=db.queue_add("t","second").unwrap();
        let mut edited=payload(); edited.text="edited 🚀".into();
        assert!(db.queue_update_payload(&a.id,&edited,Some(0)).unwrap());
        assert!(!db.queue_update_payload(&a.id,&payload(),Some(0)).unwrap());
        let claimed=db.queue_claim(&a.id,false).unwrap().unwrap();
        assert_eq!(claimed.payload,edited);
        assert!(!db.queue_update_payload(&a.id,&payload(),Some(1)).unwrap());
        assert!(!db.queue_remove(&a.id).unwrap());
        db.queue_fail(&a.id,"missing file").unwrap();
        let q=db.queue("t").unwrap();
        assert_eq!((q[0].id.as_str(),q[1].id.as_str()),(a.id.as_str(),b.id.as_str()));
        assert_eq!(q[0].payload,edited);
        assert_eq!(q[0].error.as_deref(),Some("missing file"));
        assert!(db.queue_held("t").unwrap());
    }

    #[test]
    fn stopping_before_steer_acknowledgement_keeps_an_editable_payload() {
        let db = fixture();
        let item = db.queue_add_payload("t", &payload()).unwrap();
        db.queue_claim(&item.id, false).unwrap().unwrap();
        turn(&db, "interrupted", "started");
        assert!(db.queue_bind_turn(&item.id, "interrupted").unwrap());
        db.queue_wait_ack(&item.id).unwrap();
        db.finish_turn("interrupted", "interrupted", None, None, None).unwrap();
        db.queue_settle("t").unwrap();
        db.queue_acknowledge(&item.id).unwrap();
        assert_eq!(db.queue_item(&item.id).unwrap().unwrap().state, "queued");
        assert!(db.queue_held("t").unwrap());
        assert!(db.queue_update_payload(&item.id, &payload(), Some(0)).unwrap());
    }

    #[test]
    fn a_late_steer_acknowledgement_or_rejection_controls_consumption() {
        let db = fixture();
        for (index, accepted) in [true, false].into_iter().enumerate() {
            let item = db.queue_add_payload("t", &payload()).unwrap();
            db.queue_claim(&item.id, false).unwrap().unwrap();
            let turn_id = format!("turn-{index}");
            turn(&db, &turn_id, "started");
            assert!(db.queue_bind_turn(&item.id, &turn_id).unwrap());
            db.queue_wait_ack(&item.id).unwrap();
            assert!(!db.queue_remove(&item.id).unwrap());
            assert!(!db.queue_update_payload(&item.id, &payload(), None).unwrap());
            db.finish_turn(&turn_id, "completed", None, None, None).unwrap();
            db.queue_settle("t").unwrap();
            assert_eq!(db.queue_item(&item.id).unwrap().unwrap().state, "awaiting");
            if accepted {
                db.queue_acknowledge(&item.id).unwrap();
                db.queue_settle("t").unwrap();
                assert!(db.queue_item(&item.id).unwrap().is_none());
            } else {
                db.queue_fail(&item.id, "Codex refused the steer").unwrap();
                db.queue_settle("t").unwrap();
                assert_eq!(db.queue_item(&item.id).unwrap().unwrap().payload, payload());
                assert!(db.queue_held("t").unwrap());
            }
        }
    }

    #[test]
    fn a_completed_turn_cannot_acknowledge_a_late_steer() {
        let db = fixture();
        let item = db.queue_add_payload("t", &payload()).unwrap();
        db.queue_claim(&item.id, false).unwrap().unwrap();
        turn(&db, "finished", "completed");
        assert!(!db.queue_bind_turn(&item.id, "finished").unwrap());
        db.queue_settle("t").unwrap();
        assert_eq!(db.queue_item(&item.id).unwrap().unwrap().state, "sending");
        db.queue_fail(&item.id, "Turn ended before delivery; retry the saved message.").unwrap();
        assert_eq!(db.queue_item(&item.id).unwrap().unwrap().payload, payload());
    }

    #[test]
    fn racing_send_now_and_turn_completion_claim_once() {
        let db=Arc::new(fixture());
        let item=db.queue_add_payload("t",&payload()).unwrap();
        let barrier=Arc::new(Barrier::new(3));
        let tasks:Vec<_>=(0..2).map(|_| {
            let (db,barrier,id)=(db.clone(),barrier.clone(),item.id.clone());
            std::thread::spawn(move || {barrier.wait();db.queue_claim(&id,false).unwrap().is_some()})
        }).collect();
        barrier.wait();
        assert_eq!(tasks.into_iter().map(|t| t.join().unwrap()).filter(|claimed| *claimed).count(),1);
        let next=db.queue_add("t","next").unwrap();
        assert!(db.queue_claim(&next.id,false).unwrap().is_none());
    }

    #[test]
    fn only_own_completed_turn_consumes_payload_and_failed_turn_remains_retryable() {
        let db=fixture();
        let a=db.queue_add_payload("t",&payload()).unwrap();
        db.queue_claim(&a.id,false).unwrap();
        turn(&db,"old","completed");
        turn(&db,"current","started");
        assert!(db.queue_bind_turn(&a.id,"current").unwrap());
        db.queue_settle("t").unwrap();
        assert!(db.queue_item(&a.id).unwrap().is_some(),"late old completion cannot consume current payload");
        db.lock().execute("UPDATE turns SET status='failed',error='unsupported media' WHERE id='current'",[]).unwrap();
        db.queue_settle("t").unwrap();
        assert_eq!(db.queue_item(&a.id).unwrap().unwrap().payload,payload());
        assert!(db.queue_held("t").unwrap());
        db.queue_claim(&a.id,true).unwrap().unwrap();
        turn(&db,"retry","started");
        assert!(db.queue_bind_turn(&a.id,"retry").unwrap());
        db.lock().execute("UPDATE turns SET status='completed' WHERE id='retry'", []).unwrap();
        db.queue_settle("t").unwrap();
        db.queue_settle("t").unwrap();
        assert!(db.queue_item(&a.id).unwrap().is_none());
        assert!(db.queue_claim(&a.id,true).unwrap().is_none());
    }

    #[test]
    fn legacy_rows_migrate_and_interrupted_delivery_needs_explicit_retry() {
        let dir=std::env::temp_dir().join(format!("qp-queue-payload-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path=dir.join("pilot.db");
        {
            let old=rusqlite::Connection::open(&path).unwrap();
            old.execute_batch("CREATE TABLE queue(id TEXT PRIMARY KEY,thread_id TEXT NOT NULL,position INTEGER NOT NULL,text TEXT NOT NULL,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL);
                INSERT INTO queue VALUES('legacy','t',7,'original 世界',1,1);").unwrap();
        }
        let id={
            let db=Database::open(&path).unwrap();
            db.lock().execute("INSERT INTO threads(id,provider,context_id,cwd,created_at,updated_at) VALUES('t','claude','general','.',1,1)",[]).unwrap();
            let legacy=db.queue("t").unwrap().remove(0);
            assert_eq!(legacy.payload,QueuePayload::text("original 世界"));
            assert_eq!(legacy.position,7);
            db.queue_remove("legacy").unwrap();
            let item=db.queue_add_payload("t",&payload()).unwrap();
            db.queue_claim(&item.id,false).unwrap().unwrap();
            item.id
        };
        let db=Database::open(&path).unwrap();
        assert!(db.queue_held("t").unwrap());
        let item=db.queue_item(&id).unwrap().unwrap();
        assert_eq!(item.state,"uncertain");
        assert_eq!(item.payload,payload());
        assert!(db.queue_claim(&id,false).unwrap().is_none(),"never automatically replay an ambiguous write");
        assert!(db.queue_claim(&id,true).unwrap().is_some());
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn attachment_reference_counts_include_shared_history_after_queue_removal() {
        let db=fixture();
        let a=db.queue_add_payload("t",&payload()).unwrap();
        for id in ["source","fork-copy"] {
            db.lock().execute("INSERT INTO timeline(id,thread_id,turn_id,seq,kind,data,created_at,updated_at) VALUES(?1,'t','turn',1,'user',?2,1,1)",
                params![id,serde_json::json!({"attachments":payload().attachments}).to_string()]).unwrap();
        }
        let refs=|| db.lock().query_row("SELECT refs FROM attachment_references WHERE path='saved.png'",[],|r|r.get::<_,i64>(0)).unwrap();
        assert_eq!(refs(),3);
        db.queue_remove(&a.id).unwrap();
        assert_eq!(refs(),2,"unsent removal must retain shared ownership");
    }
}
