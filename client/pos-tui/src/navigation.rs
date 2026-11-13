//! Small, local UI-only navigation history. Stores module identifiers and
//! visit timestamps, never task content, credentials or project metadata.
//!
//! The menu is ranked on launch only. Writes happen off the UI input thread.

use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const WINDOW: u64 = 7 * 24 * 60 * 60;
const MAX_BYTES: u64 = 64 * 1024;

/// XDG_STATE_HOME / pos / navigation.tsv, or ~/.local/state/pos.
/// Keep user-specific UI preferences separate from SQLite business records.
pub(crate) fn history_path() -> Option<PathBuf> {
    let root = std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    Some(root.join("pos/navigation.tsv"))
}

pub(crate) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn recent_events(text: &str, now: u64) -> Vec<(u64, usize)> {
    text.lines()
        .filter_map(|line| {
            let (epoch, section) = line.split_once('\t')?;
            let epoch = epoch.parse::<u64>().ok()?;
            let section = section.parse::<usize>().ok()?;
            if epoch <= now && now.saturating_sub(epoch) < WINDOW {
                Some((epoch, section))
            } else {
                None
            }
        })
        .collect()
}

/// Overview (first) and Settings (last) are fixed. Other sections sort
/// descending by visit count, preserving the mockup order on ties.
pub(crate) fn rank(default: &[usize], history: &str, now: u64) -> Vec<usize> {
    if default.len() <= 2 {
        return default.to_vec();
    }
    let events = recent_events(history, now);
    let mut order = default.to_vec();
    let mut counts = vec![0usize; default.iter().max().copied().unwrap_or(0) + 1];
    for (_, section) in events {
        if let Some(count) = counts.get_mut(section) {
            *count += 1;
        }
    }
    // Stable: tie order always follows the user-designed default layout.
    let last = order.len() - 1;
    order[1..last].sort_by_key(|section| std::cmp::Reverse(counts[*section]));
    order
}

pub(crate) fn load_ranked(path: &Path, default: &[usize], now: u64) -> Vec<usize> {
    // The UI must still launch when the local state directory is missing,
    // unwritable, invalid UTF-8 or otherwise inaccessible.
    match fs::read_to_string(path) {
        Ok(history) => rank(default, &history, now),
        Err(_) => default.to_vec(),
    }
}

/// Appends a single visit. Never blocks the TUI event loop.
/// Old events are compacted when the UI-only file grows beyond 64 KiB.
pub(crate) fn record(path: &Path, section: usize, now: u64) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{now}\t{section}")?;
    if file.metadata()?.len() > MAX_BYTES {
        let content = fs::read_to_string(path)?;
        let recent = recent_events(&content, now);
        let mut compact = String::new();
        for (epoch, section) in recent {
            compact.push_str(&format!("{epoch}\t{section}\n"));
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        fs::write(&temporary, compact)?;
        fs::rename(temporary, path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_on_recent_visits_but_preserves_fixed_rows_and_ties() {
        let base = [0, 1, 2, 3, 4, 5, 10];
        let now = 2_000_000;
        let events = format!(
            "{}\t4\n{}\t4\n{}\t2\n{}\t2\n{}\t3\n{}\t1\n{}\t0\n{}\t10\n",
            now - 20,
            now - 10,
            now - 1,
            now - 3,
            now - 4,
            now - WINDOW,
            now - 2,
            now - 3
        );
        assert_eq!(rank(&base, &events, now), vec![0, 2, 4, 3, 1, 5, 10]);
    }

    #[test]
    fn ignores_bad_future_and_expired_history_without_panics() {
        let base = [0, 1, 2, 3, 10];
        let now = 1_000_000;
        let input = format!(
            "not-valid\n{now}\t2\n{}\t3\n{}\t1\n12\tbad\n999\t9999999\n",
            now + 1,
            now - WINDOW
        );
        assert_eq!(rank(&base, &input, now), vec![0, 2, 1, 3, 10]);
    }

    #[test]
    fn visits_persist_only_in_an_isolated_usage_file() {
        let temp = std::env::temp_dir().join(format!(
            "pos-nav-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = temp.join("navigation.tsv");
        let now = now_secs();
        record(&path, 2, now).unwrap();
        record(&path, 2, now).unwrap();
        record(&path, 1, now).unwrap();
        let contents = fs::read_to_string(&path).unwrap();
        assert_eq!(contents.lines().count(), 3);
        assert_eq!(load_ranked(&path, &[0, 1, 2, 10], now), vec![0, 2, 1, 10]);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn missing_file_keeps_stable_default_order() {
        let path = std::env::temp_dir().join(format!(
            "pos-navigation-nonexistent-{}-{}",
            std::process::id(),
            now_secs()
        ));
        assert_eq!(
            load_ranked(&path, &[0, 1, 2, 10], now_secs()),
            vec![0, 1, 2, 10]
        );
    }
}
