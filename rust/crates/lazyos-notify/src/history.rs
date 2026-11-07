use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::model::Notification;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryRecord {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub urgency: String,
    pub timestamp: u64,
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_tag: Option<String>,
}

pub fn history_path() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
            PathBuf::from(home).join(".local").join("state")
        });
    base.join("lazyos").join("notify").join("history.jsonl")
}

pub fn append_record(record: &HistoryRecord) {
    let path = history_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        if let Ok(line) = serde_json::to_string(record) {
            let _ = writeln!(file, "{line}");
        }
    }
}

pub fn record_from_notification(n: &Notification) -> HistoryRecord {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let actions = n
        .actions
        .iter()
        .flat_map(|a| [a.key.clone(), a.label.clone()])
        .collect();

    HistoryRecord {
        id: n.id,
        app_name: n.app_name.clone(),
        summary: n.summary.clone(),
        body: n.body.clone(),
        urgency: n.urgency.to_str().to_string(),
        timestamp,
        actions,
        progress: n.progress,
        sync_tag: n.sync_tag.clone(),
    }
}

#[allow(dead_code)]
pub fn load_history() -> Vec<HistoryRecord> {
    let path = history_path();
    let Ok(file) = fs::File::open(path) else {
        return Vec::new();
    };

    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for line in reader.lines().map_while(Result::ok) {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            if let Ok(record) = serde_json::from_str::<HistoryRecord>(trimmed) {
                records.push(record);
            }
        }
    }
    records
}

#[allow(dead_code)]
pub fn clear_history() {
    let path = history_path();
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_jsonl_roundtrip() {
        let record = HistoryRecord {
            id: 42,
            app_name: "TestApp".to_string(),
            summary: "Test summary".to_string(),
            body: "Test body".to_string(),
            urgency: "normal".to_string(),
            timestamp: 1730000000,
            actions: vec!["default".to_string(), "Open".to_string()],
            progress: Some(80),
            sync_tag: Some("test-sync".to_string()),
        };

        let serialized = serde_json::to_string(&record).unwrap();
        let deserialized: HistoryRecord = serde_json::from_str(&serialized).unwrap();
        assert_eq!(record, deserialized);
    }
}
