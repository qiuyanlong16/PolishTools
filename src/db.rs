//! 本地 SQLite 记忆库：每次润色都落库，用于长期学习统计。

use std::path::PathBuf;

use rusqlite::{params, Connection};

use crate::model::{Issue, IssueStat, Record, Stats};

pub struct Store {
    pub path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn conn(&self) -> Result<Connection, String> {
        let conn = Connection::open(&self.path).map_err(|e| format!("打开数据库失败: {e}"))?;
        conn.busy_timeout(std::time::Duration::from_secs(3))
            .map_err(|e| e.to_string())?;
        Ok(conn)
    }

    pub fn init(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS corrections (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                ts          TEXT NOT NULL,
                app_title   TEXT NOT NULL DEFAULT '',
                mode        TEXT NOT NULL DEFAULT 'fulltext',
                original    TEXT NOT NULL,
                polished    TEXT NOT NULL,
                issues      TEXT NOT NULL DEFAULT '[]',
                model       TEXT NOT NULL DEFAULT '',
                latency_ms  INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_corrections_ts ON corrections(ts);
            CREATE TABLE IF NOT EXISTS issue_stats (
                kind      TEXT PRIMARY KEY,
                count     INTEGER NOT NULL DEFAULT 0,
                last_seen TEXT NOT NULL DEFAULT ''
            );
            "#,
        )
        .map_err(|e| format!("初始化数据库失败: {e}"))?;
        Ok(())
    }

    pub fn insert(
        &self,
        app_title: &str,
        mode: &str,
        original: &str,
        polished: &str,
        issues: &[Issue],
        model: &str,
        latency_ms: i64,
        ts: &str,
    ) -> Result<i64, String> {
        let mut conn = self.conn()?;
        let issues_json = serde_json::to_string(issues).unwrap_or_else(|_| "[]".to_string());

        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO corrections(ts, app_title, mode, original, polished, issues, model, latency_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![ts, app_title, mode, original, polished, issues_json, model, latency_ms],
        )
        .map_err(|e| format!("写入记录失败: {e}"))?;
        let id = tx.last_insert_rowid();

        for issue in issues {
            let kind = normalize_kind(&issue.r#type);
            tx.execute(
                "INSERT INTO issue_stats(kind, count, last_seen) VALUES(?1, 1, ?2)
                 ON CONFLICT(kind) DO UPDATE SET count = count + 1, last_seen = excluded.last_seen",
                params![kind, ts],
            )
            .map_err(|e| format!("更新统计失败: {e}"))?;
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(id)
    }

    pub fn recent(&self, limit: i64) -> Result<Vec<Record>, String> {
        let conn = self.conn()?;
        let mut stmt = conn
            .prepare(
                "SELECT id, ts, app_title, mode, original, polished, issues, model, latency_ms
                 FROM corrections ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let issues_json: String = row.get(6)?;
                Ok(Record {
                    id: row.get(0)?,
                    ts: row.get(1)?,
                    app_title: row.get(2)?,
                    mode: row.get(3)?,
                    original: row.get(4)?,
                    polished: row.get(5)?,
                    issues: serde_json::from_str(&issues_json).unwrap_or_default(),
                    model: row.get(7)?,
                    latency_ms: row.get(8)?,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(|e| e.to_string())?);
        }
        Ok(out)
    }

    pub fn stats(&self) -> Result<Stats, String> {
        let conn = self.conn()?;
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM corrections", [], |r| r.get(0))
            .unwrap_or(0);
        let today: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM corrections WHERE substr(ts,1,10) = date('now','localtime')",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let changed_total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM corrections WHERE trim(original) <> trim(polished)",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut issue_stats = Vec::new();
        if let Ok(mut stmt) =
            conn.prepare("SELECT kind, count, last_seen FROM issue_stats ORDER BY count DESC LIMIT 30")
        {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(IssueStat {
                    kind: row.get(0)?,
                    count: row.get(1)?,
                    last_seen: row.get(2)?,
                })
            }) {
                for r in rows.flatten() {
                    issue_stats.push(r);
                }
            }
        }

        let mut daily = Vec::new();
        if let Ok(mut stmt) = conn.prepare(
            "SELECT substr(ts,1,10) AS d, COUNT(*) FROM corrections
             WHERE substr(ts,1,10) >= date('now','localtime','-13 days')
             GROUP BY d ORDER BY d",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            }) {
                for r in rows.flatten() {
                    daily.push(r);
                }
            }
        }

        Ok(Stats {
            total,
            today,
            changed_total,
            issue_stats,
            daily,
        })
    }

    pub fn export_csv(&self, path: &std::path::Path) -> Result<usize, String> {
        let records = self.recent(100_000)?;
        let mut out = String::from("id,time,app,mode,original,polished,model,latency_ms\n");
        for r in &records {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                r.id,
                csv_escape(&r.ts),
                csv_escape(&r.app_title),
                csv_escape(&r.mode),
                csv_escape(&r.original),
                csv_escape(&r.polished),
                csv_escape(&r.model),
                r.latency_ms
            ));
        }
        std::fs::write(path, out).map_err(|e| format!("导出失败: {e}"))?;
        Ok(records.len())
    }

    pub fn top_mistakes(&self, limit: i64) -> Vec<(String, String, i64)> {
        let mut out = Vec::new();
        let Ok(conn) = self.conn() else {
            return out;
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT json_extract(j.value,'$.original') AS o,
                    json_extract(j.value,'$.corrected') AS c,
                    COUNT(*) AS n
             FROM corrections, json_each(corrections.issues) AS j
             GROUP BY o, c
             HAVING o IS NOT NULL AND o <> '' AND c IS NOT NULL AND c <> ''
             ORDER BY n DESC LIMIT ?1",
        ) else {
            return out;
        };
        if let Ok(rows) = stmt.query_map(params![limit], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    pub fn clear_all(&self) -> Result<(), String> {
        let conn = self.conn()?;
        conn.execute_batch(
            "DELETE FROM corrections; DELETE FROM issue_stats; VACUUM;",
        )
        .map_err(|e| format!("清空失败: {e}"))?;
        Ok(())
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

pub fn normalize_kind(kind: &str) -> String {
    let k = kind.trim().to_lowercase();
    match k.as_str() {
        "" => "other".to_string(),
        "grammar" | "spelling" | "article" | "preposition" | "tense" | "word-choice"
        | "punctuation" | "style" => k,
        _ => "other".to_string(),
    }
}

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "grammar" => "语法结构",
        "spelling" => "拼写",
        "article" => "冠词",
        "preposition" => "介词",
        "tense" => "时态",
        "word-choice" => "用词/搭配",
        "punctuation" => "标点",
        "style" => "表达风格",
        _ => "其他",
    }
}