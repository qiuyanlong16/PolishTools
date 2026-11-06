//! 公共数据结构。

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Issue {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub original: String,
    #[serde(default)]
    pub corrected: String,
    #[serde(default)]
    pub explain: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PolishResult {
    pub polished: String,
    pub issues: Vec<Issue>,
    pub summary: String,
}

#[derive(Clone, Debug, Default)]
pub struct Record {
    pub id: i64,
    pub ts: String,
    pub app_title: String,
    pub mode: String,
    pub original: String,
    pub polished: String,
    pub issues: Vec<Issue>,
    pub model: String,
    pub latency_ms: i64,
}

impl Record {
    #[allow(dead_code)]
    pub fn changed(&self) -> bool {
        self.original.trim() != self.polished.trim()
    }
}

#[derive(Clone, Debug, Default)]
pub struct IssueStat {
    pub kind: String,
    pub count: i64,
    pub last_seen: String,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub total: i64,
    pub today: i64,
    pub changed_total: i64,
    pub issue_stats: Vec<IssueStat>,
    pub daily: Vec<(String, i64)>,
}