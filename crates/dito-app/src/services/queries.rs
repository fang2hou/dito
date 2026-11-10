//! Reads of local data that views need: history queries for UI snapshots and AWS profile
//! discovery from the user's config files.

use std::sync::Arc;

use anyhow::Result;
use dito_core::store::{HistoryEntry, HistoryFilter, Stats, Store};

use crate::state::AppState;

/// History list for UI display, run off the async executor. `filter` is taken by value so the
/// returned rows are fully owned; errors propagate instead of degrading to an empty list.
pub async fn history_list(st: Arc<AppState>, filter: HistoryFilter) -> Result<Vec<HistoryEntry>> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || store.list_history(&filter))
        .await
        .map_err(|e| anyhow::anyhow!("history list task failed: {e}"))?
        .map_err(Into::into)
}

/// Aggregate stats (`Stats`) for the stats page, run off the async executor.
pub async fn history_stats(st: Arc<AppState>, since_ts: Option<i64>) -> Result<Stats> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || store.stats(since_ts))
        .await
        .map_err(|e| anyhow::anyhow!("history stats task failed: {e}"))?
        .map_err(Into::into)
}

/// The wordbook page's snapshot: the words matching a search and the size of the whole list.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WordbookWords {
    pub matching: Vec<String>,
    pub total: usize,
}

/// Wordbook words matching `query`, plus the total count, run off the async executor.
pub async fn wordbook_words(st: Arc<AppState>, query: String) -> Result<WordbookWords> {
    let store = st.store.clone();
    st.rt
        .spawn_blocking(move || wordbook_snapshot(&store, &query))
        .await
        .map_err(|e| anyhow::anyhow!("wordbook query task failed: {e}"))?
}

fn wordbook_snapshot(store: &Store, query: &str) -> Result<WordbookWords> {
    Ok(WordbookWords { matching: store.words(Some(query))?, total: store.words(None)?.len() })
}

/// AWS profile names from `~/.aws/config` and `~/.aws/credentials`.
pub fn aws_profiles() -> Vec<String> {
    let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
    profiles_in(&std::path::PathBuf::from(home).join(".aws"))
}

/// Profile names from a given `.aws` directory: `config` sections carry a `profile ` prefix,
/// `credentials` sections do not. Order is preserved, duplicates across both files dropped.
fn profiles_in(dir: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    for (file, prefix) in [("config", "profile "), ("credentials", "")] {
        let Ok(text) = std::fs::read_to_string(dir.join(file)) else { continue };
        push_profile_names(&text, prefix, &mut out);
    }
    out
}

/// Append the `[section]` names found in `text` to `out`, stripping `prefix` from names that
/// have it. `default` keeps its name; empty names and repeats are skipped.
fn push_profile_names(text: &str, prefix: &str, out: &mut Vec<String>) {
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            let name = name.trim();
            let name = if name == "default" { name } else { name.strip_prefix(prefix).unwrap_or(name) };
            if !name.is_empty() && !out.iter().any(|n| n == name) {
                out.push(name.to_string());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordbook_snapshot_filters_matches_but_counts_every_word() {
        let store = Store::open_in_memory().unwrap();
        for word in ["alpha", "beta", "alphabet"] {
            store.add_word(word, 1).unwrap();
        }
        let snapshot = wordbook_snapshot(&store, "alpha").unwrap();
        assert_eq!(snapshot.total, 3);
        assert_eq!(snapshot.matching.len(), 2);
    }

    #[test]
    fn profiles_are_parsed_from_both_aws_files_without_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let aws = dir.path().join(".aws");
        std::fs::create_dir_all(&aws).unwrap();
        std::fs::write(aws.join("config"), "[default]\nregion=x\n[profile dev-sso]\n[profile prod]\n").unwrap();
        std::fs::write(aws.join("credentials"), "[default]\nkey=1\n[extra]\n").unwrap();
        assert_eq!(profiles_in(&aws), ["default", "dev-sso", "prod", "extra"]);
    }

    #[test]
    fn text_parser_strips_prefix_keeps_default_and_skips_repeats() {
        let mut out = Vec::new();
        push_profile_names("[default]\n[profile dev]\n[dev]\n[profile dev]\n[]", "profile ", &mut out);
        assert_eq!(out, ["default", "dev"]);
    }
}
