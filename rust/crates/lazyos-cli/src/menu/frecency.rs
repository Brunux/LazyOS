//! What the person launches often and lately, as a boost for the ranked list.
//!
//! `~/.local/state/lazyos/frecency.json` (spec 007): written when a row is
//! launched, capped at [`CAP`] entries and pruned by score, and **never an
//! error to read** - a file that is missing, truncated or from a future
//! version starts the menu with no history, because a menu that fails to open
//! over a ranking hint is worse than one that ranks plainly.
//!
//! A row is promoted only from its [`PROMOTE_AFTER`]th launch on. One launch is
//! as likely a slip as a habit, and a menu that reorders itself after a single
//! mistaken Enter teaches the person not to trust where things are.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The most entries the file keeps. The root menu and the palette have about
/// sixty rows today; the launcher's applications (spec 012) are what fill it.
pub const CAP: usize = 200;

/// Launches before a row's boost becomes non-zero.
pub const PROMOTE_AFTER: u32 = 3;

const DAY: u64 = 24 * 60 * 60;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    count: u32,
    /// Seconds since the epoch of the last launch.
    last: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    version: u32,
    entries: BTreeMap<String, Entry>,
}

const VERSION: u32 = 1;

#[derive(Debug)]
pub struct Frecency {
    path: PathBuf,
    entries: BTreeMap<String, Entry>,
}

impl Frecency {
    /// The history at `path`, or an empty one when the file cannot be used.
    pub fn load(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<File>(&bytes).ok())
            .filter(|file| file.version == VERSION)
            .map(|file| file.entries)
            .unwrap_or_default();
        Self { path, entries }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn count(&self, key: &str) -> u32 {
        self.entries.get(key).map_or(0, |e| e.count)
    }

    /// The boost a ranked list adds to `key` at `now`: zero until the row has
    /// been launched [`PROMOTE_AFTER`] times, then the launches (capped, so a
    /// row used a thousand times does not bury every new habit) weighted by how
    /// recent the last one is.
    pub fn boost(&self, key: &str, now: u64) -> u32 {
        let Some(entry) = self.entries.get(key) else {
            return 0;
        };
        if entry.count < PROMOTE_AFTER {
            return 0;
        }
        score(entry, now)
    }

    /// Note a launch of `key` and keep the history within [`CAP`].
    pub fn record(&mut self, key: &str, now: u64) {
        let entry = self.entries.entry(key.to_string()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.last = now;
        self.prune(now);
    }

    /// Drop the weakest entries until [`CAP`] remain: the lowest score first,
    /// the oldest among equals, so what is forgotten is what was least used
    /// and longest ago.
    pub fn prune(&mut self, now: u64) {
        if self.entries.len() <= CAP {
            return;
        }
        let mut ranked: Vec<(u32, u64, String)> = self
            .entries
            .iter()
            .map(|(key, entry)| (score(entry, now), entry.last, key.clone()))
            .collect();
        ranked.sort();
        let excess = self.entries.len() - CAP;
        for (_, _, key) in ranked.into_iter().take(excess) {
            self.entries.remove(&key);
        }
    }

    /// Write the history with a temp file and a rename in the same directory,
    /// so a menu killed mid-write leaves the old file whole.
    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File {
            version: VERSION,
            entries: self.entries.clone(),
        };
        let bytes = serde_json::to_vec(&file).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &self.path)
    }
}

/// Launches weighted by recency: a habit of this week outranks a bigger one
/// from last season.
fn score(entry: &Entry, now: u64) -> u32 {
    let age = now.saturating_sub(entry.last);
    let weight = if age < DAY {
        4
    } else if age < 7 * DAY {
        2
    } else if age < 30 * DAY {
        1
    } else {
        0
    };
    entry.count.min(50) * weight
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazyos_ui::{ListState, Row};

    const NOW: u64 = 1_800_000_000;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "lazyos-frecency-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("frecency.json")
    }

    fn top(frecency: &Frecency, needle: &str) -> String {
        let mut list = ListState::new().ranked();
        list.set_rows(vec![
            Row::new("theme").boost(frecency.boost("theme", NOW)),
            Row::new("themes install").boost(frecency.boost("themes-install", NOW)),
        ]);
        list.set_filter(needle);
        list.selected().unwrap().label.clone()
    }

    #[test]
    fn frecency_promotes_after_three() {
        let mut frecency = Frecency::load(scratch("promote"));
        assert_eq!(top(&frecency, "th"), "theme", "the shorter prefix wins by default");

        frecency.record("themes-install", NOW);
        frecency.record("themes-install", NOW);
        assert_eq!(frecency.boost("themes-install", NOW), 0);
        assert_eq!(
            top(&frecency, "th"),
            "theme",
            "two launches are not a habit yet"
        );

        frecency.record("themes-install", NOW);
        assert!(frecency.boost("themes-install", NOW) > 0);
        assert_eq!(
            top(&frecency, "th"),
            "themes install",
            "the third launch moves the repeated choice up"
        );
    }

    #[test]
    fn frecency_prunes_to_cap() {
        let mut frecency = Frecency::load(scratch("prune"));
        // A habit, recent: it must survive any amount of one-off history.
        for _ in 0..5 {
            frecency.record("wifi", NOW);
        }
        for i in 0..(CAP + 50) {
            frecency.record(&format!("app-{i}"), NOW - 40 * DAY + i as u64);
        }
        assert_eq!(frecency.len(), CAP);
        assert_eq!(frecency.count("wifi"), 5, "the habit was kept");
        assert_eq!(
            frecency.count("app-0"),
            0,
            "the oldest one-off was the first to go"
        );
    }

    #[test]
    fn a_stale_habit_stops_boosting() {
        let mut frecency = Frecency::load(scratch("stale"));
        for _ in 0..10 {
            frecency.record("dns", NOW - 60 * DAY);
        }
        assert_eq!(frecency.boost("dns", NOW), 0);
        assert!(frecency.boost("dns", NOW - 60 * DAY + 60) > 0);
    }

    #[test]
    fn an_unreadable_file_starts_empty() {
        let path = scratch("broken");
        for bytes in [&b"{ not json"[..], b"", b"{\"version\":99,\"entries\":{}}"] {
            std::fs::write(&path, bytes).unwrap();
            let frecency = Frecency::load(&path);
            assert!(frecency.is_empty(), "{:?} must start empty", String::from_utf8_lossy(bytes));
        }
        let frecency = Frecency::load(path.with_file_name("missing.json"));
        assert!(frecency.is_empty());
    }

    #[test]
    fn the_history_round_trips_and_leaves_no_temp_file() {
        let path = scratch("roundtrip");
        let mut frecency = Frecency::load(&path);
        for _ in 0..3 {
            frecency.record("power", NOW);
        }
        frecency.save().unwrap();
        let again = Frecency::load(&path);
        assert_eq!(again.count("power"), 3);
        assert!(!path.with_extension("json.tmp").exists());
    }
}
