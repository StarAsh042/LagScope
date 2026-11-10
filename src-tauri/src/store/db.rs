//! SQLite 事件库:卡顿事件持久化(表 lag_events)。

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use rusqlite::Connection;

use crate::engine::LagEvent;

/// 事件库保留上限(条)
pub const RETAIN_ROWS: u32 = 500;
/// 每写入多少条触发一次保留清理(open 时另有一次)
pub const PRUNE_EVERY_INSERTS: u32 = 50;

pub struct Db {
    conn: Mutex<Connection>,
    /// 保留上限(条);设置页可热更新,故用原子量而不是普通字段
    retain_rows: AtomicU32,
    /// 每写入多少条触发一次清理
    prune_every: u32,
    /// 自上次清理以来成功写入的条数(Relaxed 足够:只做阈值判断)
    inserts_since_prune: AtomicU64,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self, String> {
        Self::open_with_retention(path, RETAIN_ROWS, PRUNE_EVERY_INSERTS)
    }

    /// 自定义保留参数(测试用小上限,避免为验证裁剪逻辑写上千条记录)
    pub fn open_with_retention(
        path: &Path,
        retain_rows: u32,
        prune_every: u32,
    ) -> Result<Self, String> {
        let retain_rows = retain_rows.max(1);
        let prune_every = prune_every.max(1);
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
        // 保留策略:启动时先清一次,长跑期间再按写入条数定期清(见 insert_event)
        prune_inner(&conn, retain_rows)?;
        Ok(Self {
            conn: Mutex::new(conn),
            retain_rows: AtomicU32::new(retain_rows),
            prune_every,
            inserts_since_prune: AtomicU64::new(0),
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
        let id = conn.last_insert_rowid();

        // 长跑期间的保留清理:只在计数达到阈值的那一条上做,失败不影响写入结果
        let seen = self.inserts_since_prune.fetch_add(1, Ordering::Relaxed) + 1;
        if seen >= self.prune_every as u64 {
            self.inserts_since_prune.store(0, Ordering::Relaxed);
            let _ = prune_inner(&conn, self.retain_rows.load(Ordering::Relaxed));
        }
        Ok(id)
    }

    /// 当前保留上限(条);导出等需要"全库"的调用方用它作默认上限
    pub fn retain_rows(&self) -> u32 {
        self.retain_rows.load(Ordering::Relaxed)
    }

    /// 调整保留上限并立即清理一次,返回被删除的条数。
    ///
    /// 设置页把上限改小时,如果只更新字段,超出部分要等到下一条事件写入
    /// (可能几天后)才会被裁掉;这里顺手清一次,用户能立刻看到库变小。
    /// 改大时不删任何行,清理自然是 no-op。
    pub fn set_retain_rows(&self, retain_rows: u32) -> Result<usize, String> {
        let retain_rows = retain_rows.max(1);
        self.retain_rows.store(retain_rows, Ordering::Relaxed);
        let conn = self.conn.lock().unwrap();
        prune_inner(&conn, retain_rows)
    }

    /// 立即执行一次保留清理,返回删除行数(仅测试用:长跑由 insert_event 自动触发)
    #[cfg(test)]
    pub fn prune(&self) -> Result<usize, String> {
        let conn = self.conn.lock().unwrap();
        prune_inner(&conn, self.retain_rows.load(Ordering::Relaxed))
    }

    /// 当前事件条数(仅测试用,故不进 lib 构建)
    #[cfg(test)]
    pub fn count_events(&self) -> Result<u32, String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM lag_events", [], |row| row.get(0))
            .map_err(|err| format!("统计事件条数失败: {err}"))
    }

    /// 按 id 删除事件(用户手动清理),返回实际删除条数。
    ///
    /// 逐条删除而不是拼一个 IN (?1, ?2, …):调用方的条数很小,逐条能复用同一条
    /// 语句缓存,也不用为了占位符数量拼 SQL 字符串。
    pub fn delete_events(&self, ids: &[i64]) -> Result<usize, String> {
        if ids.is_empty() {
            return Ok(0);
        }
        let conn = self.conn.lock().unwrap();
        let mut removed = 0;
        for id in ids {
            removed += conn
                .execute(
                    "DELETE FROM lag_events WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|err| format!("删除事件失败: {err}"))?;
        }
        Ok(removed)
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

/// 只保留 `retain_rows` 条最新事件,返回被删除的条数。
fn prune_inner(conn: &Connection, retain_rows: u32) -> Result<usize, String> {
    conn.execute(
        "DELETE FROM lag_events WHERE id NOT IN
         (SELECT id FROM lag_events ORDER BY ts_ms DESC, id DESC LIMIT ?1)",
        rusqlite::params![retain_rows],
    )
    .map_err(|e| format!("清理历史事件失败: {e}"))
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
            "tsMs",
            "reasons",
            "counters",
            "topCpu",
            "topMem",
            "topDisk",
            "hungWindows",
            "game",
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

    /// 测试用的保留上限:小到能在毫秒级跑完,又足以区分"裁剪过"与"没裁剪"
    const T_RETAIN: u32 = 8;
    const T_EVERY: u32 = 5;

    fn cleanup(p: &std::path::Path) {
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{}", p.display(), suffix));
        }
    }

    fn insert_n(db: &Db, n: u32, base_ts: u64) {
        for i in 0..n {
            let mut ev = fixture(None, None);
            ev.ts_ms = base_ts + i as u64;
            db.insert_event(&ev).expect("insert_event 必须成功");
        }
    }

    #[test]
    fn retention_prunes_automatically_without_restart() {
        let p = temp_db_path();
        let db = Db::open_with_retention(&p, T_RETAIN, T_EVERY).unwrap();

        // 第 1 个周期内(5 条)尚未超过上限 → 不该删
        insert_n(&db, T_EVERY, 1_700_000_000_000);
        assert_eq!(db.count_events().unwrap(), T_EVERY);

        // 写到上限 + 一个周期:区间内会有一次清理,条数不会超过 retain + every
        insert_n(&db, T_RETAIN, 1_700_000_000_000 + T_EVERY as u64);
        let mid = db.count_events().unwrap();
        assert!(
            mid <= T_RETAIN + T_EVERY,
            "两次清理之间最多堆积一个周期的量,实测 {mid}"
        );

        // 再跨过一个完整周期,清理必然落到超出上限的那一刻
        let total = T_EVERY * 3;
        insert_n(
            &db,
            total - (T_EVERY + T_RETAIN),
            1_700_000_000_000 + (T_EVERY + T_RETAIN) as u64,
        );
        assert_eq!(
            db.count_events().unwrap(),
            T_RETAIN,
            "跨过清理周期应自动裁剪到保留上限(否则长跑会无限增长)"
        );

        // 留下的必须是最新的那批:总共 inserted 条,最新的一条是最后写入的
        let rows = db.list_events(1000).unwrap();
        assert_eq!(rows.len() as u32, T_RETAIN);
        let last_ts = 1_700_000_000_000 + (total - 1) as u64;
        assert_eq!(rows.first().unwrap().ts_ms, last_ts, "顶部应是最新事件");
        assert_eq!(
            rows.last().unwrap().ts_ms,
            1_700_000_000_000 + (total - T_RETAIN) as u64,
            "底部应是保留窗口的左端"
        );
        drop(db);
        cleanup(&p);
    }

    #[test]
    fn manual_prune_reports_removed_rows() {
        let p = temp_db_path();
        // 清理周期调到很大,确保只有显式 prune 才生效
        let db = Db::open_with_retention(&p, T_RETAIN, 1000).unwrap();
        insert_n(&db, T_RETAIN + 4, 1_700_000_000_000);
        assert_eq!(db.count_events().unwrap(), T_RETAIN + 4);

        let removed = db.prune().expect("prune 必须成功");
        assert_eq!(removed, 4, "prune 返回的删除条数应等于超出的部分");
        assert_eq!(db.count_events().unwrap(), T_RETAIN);
        // 留下的必须是最新的 5..11 号
        let rows = db.list_events(100).unwrap();
        assert_eq!(rows.last().unwrap().ts_ms, 1_700_000_000_000 + 4);
        drop(db);
        cleanup(&p);
    }

    #[test]
    fn prune_is_noop_below_limit() {
        let p = temp_db_path();
        let db = Db::open_with_retention(&p, T_RETAIN, T_EVERY).unwrap();
        insert_n(&db, 3, 1_700_000_000_000);
        assert_eq!(db.prune().unwrap(), 0);
        assert_eq!(db.count_events().unwrap(), 3);
        drop(db);
        cleanup(&p);
    }

    #[test]
    fn shrinking_retention_prunes_immediately() {
        let p = temp_db_path();
        // 清理周期调到很大:只有显式改上限才该发生裁剪
        let db = Db::open_with_retention(&p, 100, 1000).unwrap();
        insert_n(&db, 20, 1_700_000_000_000);
        assert_eq!(db.count_events().unwrap(), 20);
        assert_eq!(db.retain_rows(), 100, "初始上限应来自 open 参数");

        // 调小上限:同一瞬间就该裁到新上限,而不是等下一条事件写入
        let removed = db.set_retain_rows(T_RETAIN).expect("改上限必须成功");
        assert_eq!(removed, (20 - T_RETAIN) as usize, "返回的应是裁掉的条数");
        assert_eq!(db.retain_rows(), T_RETAIN);
        assert_eq!(db.count_events().unwrap(), T_RETAIN);
        // 留下的是最新的那批
        let rows = db.list_events(100).unwrap();
        assert_eq!(rows.first().unwrap().ts_ms, 1_700_000_000_000 + 19);
        assert_eq!(
            rows.last().unwrap().ts_ms,
            1_700_000_000_000 + (20 - T_RETAIN) as u64
        );

        // 调大上限不删任何行,后续写入也不应被旧上限卡住
        assert_eq!(db.set_retain_rows(50).unwrap(), 0);
        assert_eq!(db.retain_rows(), 50);
        insert_n(&db, 10, 1_700_000_100_000);
        assert_eq!(db.count_events().unwrap(), T_RETAIN + 10);

        // 0 会让保留策略恒删全表,必须夹到至少 1
        assert_eq!(db.set_retain_rows(0).unwrap(), (T_RETAIN + 9) as usize);
        assert_eq!(db.retain_rows(), 1);
        assert_eq!(db.count_events().unwrap(), 1);
        drop(db);
        cleanup(&p);
    }

    #[test]
    fn reopen_applies_retention_to_existing_rows() {
        let p = temp_db_path();
        {
            let db = Db::open_with_retention(&p, 100, 1000).unwrap();
            insert_n(&db, 12, 1_700_000_000_000);
            assert_eq!(db.count_events().unwrap(), 12);
        }
        // 用更小的上限重开:旧库里超出的部分应在 open 时被裁掉
        {
            let db = Db::open_with_retention(&p, T_RETAIN, T_EVERY).unwrap();
            assert_eq!(db.count_events().unwrap(), T_RETAIN, "open 应清一次历史");
            let rows = db.list_events(100).unwrap();
            assert_eq!(
                rows.first().unwrap().ts_ms,
                1_700_000_000_000 + 11,
                "留下的必须是最新的"
            );
        }
        cleanup(&p);
    }

    #[test]
    fn delete_events_removes_only_the_given_ids() {
        let p = temp_db_path();
        let db = Db::open_with_retention(&p, T_RETAIN, 1000).unwrap();
        insert_n(&db, 5, 1_700_000_000_000);
        let rows = db.list_events(100).unwrap();
        assert_eq!(rows.len(), 5);

        // 删掉中间两条:剩下的应该是原 #1 #3 #5(按时间正序)
        let victims = vec![rows[3].id.unwrap(), rows[1].id.unwrap()];
        let removed = db.delete_events(&victims).expect("delete_events 必须成功");
        assert_eq!(removed, 2, "返回的删除条数应等于实际命中条数");

        let left = db.list_events(100).unwrap();
        assert_eq!(left.len(), 3);
        let ids: Vec<i64> = left.iter().map(|e| e.id.unwrap()).collect();
        assert!(!ids.contains(&victims[0]), "被删的 id 不该再出现");
        assert!(!ids.contains(&victims[1]), "被删的 id 不该再出现");

        drop(db);
        cleanup(&p);
    }

    #[test]
    fn delete_events_ignores_unknown_ids() {
        let p = temp_db_path();
        let db = Db::open_with_retention(&p, T_RETAIN, 1000).unwrap();
        insert_n(&db, 3, 1_700_000_000_000);

        // 已经不存在的 id(重复删除 / 界面里的陈旧条目)不该报错,也不该删掉别的行
        let removed = db.delete_events(&[999_999]).expect("未知 id 不该报错");
        assert_eq!(removed, 0);
        assert_eq!(db.count_events().unwrap(), 3);

        // 空列表是合法的 no-op
        assert_eq!(db.delete_events(&[]).unwrap(), 0);
        assert_eq!(db.count_events().unwrap(), 3);

        drop(db);
        cleanup(&p);
    }
}
