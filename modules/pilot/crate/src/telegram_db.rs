//! Telegram bot storage (round 4, card 2): the one linked chat, how far the bot
//! has read, the thread the chat talks to and the threads it follows. Schema 26.
//! The token is not here: it lives in the vault.

use super::Database;
use rusqlite::{params, OptionalExtension};

pub(crate) const TELEGRAM_TABLES: &str = "
    CREATE TABLE IF NOT EXISTS telegram_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        chat_id INTEGER,
        chat_name TEXT NOT NULL DEFAULT '',
        update_offset INTEGER NOT NULL DEFAULT 0,
        current_thread TEXT,
        followed TEXT NOT NULL DEFAULT '[]'
    );
    INSERT OR IGNORE INTO telegram_state (id) VALUES (1);
";

/// Threads the chat hears the end of; the oldest drops out past this.
const FOLLOWED_MAX: usize = 50;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TelegramState {
    pub chat_id: Option<i64>,
    pub chat_name: String,
    pub update_offset: i64,
    pub current_thread: Option<String>,
    pub followed: Vec<String>,
}

impl Database {
    pub fn telegram_state(&self) -> TelegramState {
        self.lock()
            .query_row(
                "SELECT chat_id, chat_name, update_offset, current_thread, followed FROM telegram_state WHERE id = 1",
                [],
                |r| {
                    Ok(TelegramState {
                        chat_id: r.get(0)?,
                        chat_name: r.get(1)?,
                        update_offset: r.get(2)?,
                        current_thread: r.get(3)?,
                        followed: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default(),
                    })
                },
            )
            .optional()
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn telegram_link(&self, chat_id: Option<i64>, chat_name: &str) -> rusqlite::Result<()> {
        self.lock().execute(
            "UPDATE telegram_state SET chat_id = ?1, chat_name = ?2, current_thread = NULL, followed = '[]' WHERE id = 1",
            params![chat_id, chat_name],
        )?;
        Ok(())
    }

    /// Saved before an update is acted on: a crash loses one update at most,
    /// and an update is never acted on twice.
    pub fn telegram_set_offset(&self, offset: i64) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE telegram_state SET update_offset = ?1 WHERE id = 1", params![offset])?;
        Ok(())
    }

    pub fn telegram_set_current(&self, thread: Option<&str>) -> rusqlite::Result<()> {
        self.lock().execute("UPDATE telegram_state SET current_thread = ?1 WHERE id = 1", params![thread])?;
        Ok(())
    }

    pub fn telegram_follow(&self, thread: &str) -> rusqlite::Result<()> {
        let mut followed = self.telegram_state().followed;
        followed.retain(|t| t != thread);
        followed.push(thread.to_string());
        if followed.len() > FOLLOWED_MAX {
            followed.remove(0);
        }
        let json = serde_json::to_string(&followed).unwrap_or_else(|_| "[]".into());
        self.lock().execute("UPDATE telegram_state SET followed = ?1 WHERE id = 1", params![json])?;
        Ok(())
    }
}
