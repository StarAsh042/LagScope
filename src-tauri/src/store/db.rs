//! SQLite 事件库:卡顿事件持久化(表 lag_events)。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::engine::LagEvent;

pub struct Db {
    conn: Mutex<Connection>,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS lag_events (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts_ms INTEGER NOT NULL,
                 reasons_json TEXT NOT NULL,
                 counters_json TEXT NOT NULL,
                 top_cpu_json TEXT NOT NULL,
                 top_mem_json TEXT NOT NULL,
                 top_disk_json TEXT NOT NULL,
                 hung_windows_json TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_lag_events_ts ON lag_events(ts_ms DESC);",
        )
        .map_err(|e| format!("初始化表失败: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_event(&self, e: &LagEvent) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO lag_events
             (ts_ms, reasons_json, counters_json, top_cpu_json, top_mem_json, top_disk_json, hung_windows_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                e.ts_ms as i64,
                serde_json::to_string(&e.reasons).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.counters).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_cpu).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_mem).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_disk).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.hung_windows).map_err(|err| err.to_string())?,
            ],
        )
        .map_err(|err| format!("插入事件失败: {err}"))?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_events(&self, limit: u32) -> Result<Vec<LagEvent>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, ts_ms, reasons_json, counters_json,
                        top_cpu_json, top_mem_json, top_disk_json, hung_windows_json
                 FROM lag_events ORDER BY ts_ms DESC LIMIT ?1",
            )
            .map_err(|err| err.to_string())?;

        let rows = stmt
            .query_map(rusqlite::params![limit], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|err| err.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            let (id, ts_ms, reasons, counters, top_cpu, top_mem, top_disk, hung) =
                row.map_err(|err| err.to_string())?;
            events.push(LagEvent {
                id: Some(id),
                ts_ms: ts_ms.max(0) as u64,
                reasons: serde_json::from_str(&reasons).unwrap_or_default(),
                counters: serde_json::from_str(&counters).unwrap_or_default(),
                top_cpu: serde_json::from_str(&top_cpu).unwrap_or_default(),
                top_mem: serde_json::from_str(&top_mem).unwrap_or_default(),
                top_disk: serde_json::from_str(&top_disk).unwrap_or_default(),
                hung_windows: serde_json::from_str(&hung).unwrap_or_default(),
            });
        }
        Ok(events)
    }
}
