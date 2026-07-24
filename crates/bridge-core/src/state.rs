use std::{fs, path::Path, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};
use thiserror::Error;

const SCHEMA: &str = "PRAGMA journal_mode=WAL;
 CREATE TABLE IF NOT EXISTS processed_messages (
   platform TEXT NOT NULL,
   message_id TEXT NOT NULL,
   processed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
   PRIMARY KEY (platform, message_id)
 );
 CREATE TABLE IF NOT EXISTS conversation_threads (
   conversation_key TEXT PRIMARY KEY,
   thread_id TEXT NOT NULL,
   updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
 );
 CREATE TABLE IF NOT EXISTS thread_references (
   platform TEXT NOT NULL,
   chat_id TEXT NOT NULL,
   reference TEXT NOT NULL,
   thread_id TEXT NOT NULL,
   updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
   PRIMARY KEY (platform, chat_id, reference)
 );";

#[derive(Debug, Error)]
pub enum StateError {
    #[error("failed to create state directory: {0}")]
    Directory(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("state lock is poisoned")]
    Poisoned,
}

pub struct StateStore {
    connection: Mutex<Connection>,
}

impl StateStore {
    pub fn open(path: &Path) -> Result<Self, StateError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path)?;
        connection.execute_batch(SCHEMA)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn claim_message(&self, platform: &str, message_id: &str) -> Result<bool, StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        let changed = connection.execute(
            "INSERT OR IGNORE INTO processed_messages(platform, message_id) VALUES (?1, ?2)",
            params![platform, message_id],
        )?;
        Ok(changed == 1)
    }

    pub fn thread_for(&self, key: &str) -> Result<Option<String>, StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        connection
            .query_row(
                "SELECT thread_id FROM conversation_threads WHERE conversation_key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(StateError::from)
    }

    pub fn set_thread(&self, key: &str, thread_id: &str) -> Result<(), StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        connection.execute(
            "INSERT INTO conversation_threads(conversation_key, thread_id)
             VALUES (?1, ?2)
             ON CONFLICT(conversation_key) DO UPDATE
             SET thread_id = excluded.thread_id, updated_at = CURRENT_TIMESTAMP",
            params![key, thread_id],
        )?;
        Ok(())
    }

    pub fn clear_thread(&self, key: &str) -> Result<(), StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        connection.execute(
            "DELETE FROM conversation_threads WHERE conversation_key = ?1",
            [key],
        )?;
        Ok(())
    }

    pub fn thread_for_reference(
        &self,
        platform: &str,
        chat_id: &str,
        reference: &str,
    ) -> Result<Option<String>, StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        connection
            .query_row(
                "SELECT thread_id FROM thread_references
                 WHERE platform = ?1 AND chat_id = ?2 AND reference = ?3",
                params![platform, chat_id, reference],
                |row| row.get(0),
            )
            .optional()
            .map_err(StateError::from)
    }

    pub fn set_thread_reference(
        &self,
        platform: &str,
        chat_id: &str,
        reference: &str,
        thread_id: &str,
    ) -> Result<(), StateError> {
        let connection = self.connection.lock().map_err(|_| StateError::Poisoned)?;
        connection.execute(
            "INSERT INTO thread_references(platform, chat_id, reference, thread_id)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(platform, chat_id, reference) DO UPDATE
             SET thread_id = excluded.thread_id, updated_at = CURRENT_TIMESTAMP",
            params![platform, chat_id, reference, thread_id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> StateStore {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(SCHEMA).unwrap();
        StateStore {
            connection: Mutex::new(connection),
        }
    }

    #[test]
    fn direct_thread_can_be_cleared_for_new_command() {
        let state = store();
        state.set_thread("wecom:direct:u1", "thread-1").unwrap();
        assert_eq!(
            state.thread_for("wecom:direct:u1").unwrap().as_deref(),
            Some("thread-1")
        );
        state.clear_thread("wecom:direct:u1").unwrap();
        assert_eq!(state.thread_for("wecom:direct:u1").unwrap(), None);
    }

    #[test]
    fn group_task_references_are_scoped_to_the_chat() {
        let state = store();
        state
            .set_thread_reference("wecom", "group-1", "TASK1234", "thread-1")
            .unwrap();
        assert_eq!(
            state
                .thread_for_reference("wecom", "group-1", "TASK1234")
                .unwrap()
                .as_deref(),
            Some("thread-1")
        );
        assert_eq!(
            state
                .thread_for_reference("wecom", "group-2", "TASK1234")
                .unwrap(),
            None
        );
    }
}
