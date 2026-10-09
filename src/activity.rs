//! A short history of finished operations for the Activity view.
use crate::{files, model::Paths};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    /// Unix time the operation finished.
    pub time: i64,
    pub action: String,
    pub app: String,
    /// Complete, Failed or Cancelled.
    pub stage: String,
    /// What happened, one line per change ("Installed LightCraft 0.4.0").
    pub lines: Vec<String>,
    pub error: String,
}
const FILE: &str = "runtime/activity.json";
const KEEP: usize = 100;

pub fn read(paths: &Paths) -> Result<Vec<Entry>> {
    files::read_or_default(&paths.at(FILE))
}
/// Adds an entry, newest first, keeping the last hundred.
pub fn record(paths: &Paths, entry: Entry) -> Result<()> {
    let mut entries = read(paths).unwrap_or_default();
    entries.insert(0, entry);
    entries.truncate(KEEP);
    files::write_json(&paths.at(FILE), &entries)
}
/// The lines of a job log that say what changed, for the history.
pub fn summary(log: &str) -> Vec<String> {
    const MARKS: [&str; 9] = [
        "Installed ",
        " → ",
        "Moved ",
        "Deleted ",
        "Restored ",
        "update available",
        "is up to date",
        "are up to date",
        "update(s) available",
    ];
    log.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("ERROR"))
        .filter(|line| MARKS.iter().any(|m| line.contains(m)))
        .map(str::to_owned)
        .take(30)
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_keeps_newest_first_and_summaries_keep_only_changes() {
        let root = std::env::temp_dir().join(format!("craft-activity-{}", uuid::Uuid::new_v4()));
        let paths = Paths::new(root.clone(), None);
        for n in 0..(KEEP + 5) {
            record(
                &paths,
                Entry {
                    time: n as i64,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let entries = read(&paths).unwrap();
        assert_eq!(entries.len(), KEEP);
        assert_eq!(entries[0].time, (KEEP + 4) as i64);
        let log = "Checking releases\nInstalled LightCraft 0.4.0 (abc)\nDownloading 50%\nlightcraft: 0.2.1 → 0.4.0 available\nERROR: Installed nothing\n";
        assert_eq!(
            summary(log),
            [
                "Installed LightCraft 0.4.0 (abc)",
                "lightcraft: 0.2.1 → 0.4.0 available"
            ]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
