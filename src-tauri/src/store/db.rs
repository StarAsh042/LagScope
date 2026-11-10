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
        // 迁移:游戏帧字段(v0.2.0)、DPC 归因字段(v0.6.0 新增,旧行为 NULL)
        let _ = conn.execute_batch("ALTER TABLE lag_events ADD COLUMN game_json TEXT;");
        let _ = conn.execute_batch("ALTER TABLE lag_events ADD COLUMN dpc_json TEXT;");
        // 保留策略:只保留最近 500 条(事件库无限增长防护,启动时执行一次)
        conn.execute_batch(
            "DELETE FROM lag_events WHERE id NOT IN (SELECT id FROM lag_events ORDER BY ts_ms DESC LIMIT 500);",
        )
        .map_err(|e| format!("清理历史事件失败: {e}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_event(&self, e: &LagEvent) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO lag_events
             (ts_ms, reasons_json, counters_json, top_cpu_json, top_mem_json, top_disk_json, hung_windows_json, game_json, dpc_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                e.ts_ms as i64,
                serde_json::to_string(&e.reasons).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.counters).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_cpu).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_mem).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.top_disk).map_err(|err| err.to_string())?,
                serde_json::to_string(&e.hung_windows).map_err(|err| err.to_string())?,
                e.game
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|err| err.to_string())?,
                e.dpc_top
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|err| err.to_string())?,
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
                        top_cpu_json, top_mem_json, top_disk_json, hung_windows_json, game_json, dpc_json
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
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                ))
            })
            .map_err(|err| err.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            let (id, ts_ms, reasons, counters, top_cpu, top_mem, top_disk, hung, game, dpc) =
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
                game: game.and_then(|g| serde_json::from_str(&g).ok()),
                dpc_top: dpc.and_then(|d| serde_json::from_str(&d).ok()),
            });
        }
        Ok(events)
    }
}
#[cfg(test)]
mod tests {
    use super::Db;
    use crate::collectors::MetricSample;
    use crate::engine::lag_detector::{DpcSample, GameSample, LagEvent, LagReason, TopProc};

    fn temp_db_path() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        // 测试库落在工作区内 target/（沙箱可写），避免系统 TEMP 被拒导致误判红灯
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("roundtrip-dbs");
        std::fs::create_dir_all(&dir).expect("创建测试库目录失败");
        dir.join(format!(
            "lagscope_roundtrip_{}_{}_{}.sqlite",
            std::process::id(),
            nanos,
            seq
        ))
    }

    fn fixture(game: Option<GameSample>, dpc: Option<DpcSample>) -> LagEvent {
        LagEvent {
            id: None,
            ts_ms: 1_700_000_000_000,
            reasons: vec![LagReason {
                kind: "page_faults".into(),
                detail: "硬缺页".into(),
                value: 512.0,
            }],
            counters: MetricSample {
                page_input_per_sec: 512.0,
                ..Default::default()
            },
            top_cpu: vec![TopProc {
                pid: 1,
                name: "a.exe".into(),
                value: 99.0,
            }],
            top_mem: vec![],
            top_disk: vec![],
            hung_windows: vec!["卡死窗口".into()],
            game,
            dpc_top: dpc,
        }
    }

    fn assert_roundtrip(game: Option<GameSample>, dpc: Option<DpcSample>) {
        let p = temp_db_path();
        let db = Db::open(&p).unwrap();
        let mut ev = fixture(game, dpc);
        let id = db
            .insert_event(&ev)
            .expect("insert_event 必须成功（9 参 = 9 占位符）");
        assert!(id > 0, "AC-6: 入库后 id 必须为正整数");
        ev.id = Some(id);
        let rows = db.list_events(10).unwrap();
        let row = rows
            .iter()
            .find(|x| x.id == Some(id))
            .expect("AC-5: list_events 必须读回该条");
        // LagEvent 无 PartialEq/Deserialize；用 serde_json::to_value 逐字段比对，跳过 id
        let a = serde_json::to_value(&ev).unwrap();
        let b = serde_json::to_value(row).unwrap();
        for k in [
            "tsMs", "reasons", "counters", "topCpu", "topMem", "topDisk", "hungWindows", "game",
            "dpcTop",
        ] {
            assert_eq!(a.get(k), b.get(k), "字段 {k} 往返不一致（AC-5 逐字段相等）");
        }
        drop(db);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", p.display(), suffix));
        }
    }

    #[test]
    fn roundtrip_both_some() {
        assert_roundtrip(
            Some(GameSample {
                app: "game.exe".into(),
                fps: 42,
                ..Default::default()
            }),
            Some(DpcSample {
                total: 9000,
                drivers: vec![("dxgmms2.sys".into(), 7000)],
                ..Default::default()
            }),
        );
    }

    #[test]
    fn roundtrip_both_none() {
        assert_roundtrip(None, None);
    }

    #[test]
    fn roundtrip_only_game_some() {
        assert_roundtrip(
            Some(GameSample {
                app: "game.exe".into(),
                ..Default::default()
            }),
            None,
        );
    }

    #[test]
    fn roundtrip_only_dpc_some() {
        assert_roundtrip(
            None,
            Some(DpcSample {
                total: 9000,
                ..Default::default()
            }),
        );
    }
}
