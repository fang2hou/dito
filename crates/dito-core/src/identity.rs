//! Which app this build is. Debug builds (`cargo run`, the dev runner, `cargo test`) get their own identity, so they
//! never read or write the installed app's config, history, models, Keychain items or login item.

/// Bundle id, signing identifier, Keychain service and login item label.
pub const BUNDLE_ID: &str = if cfg!(debug_assertions) { "com.fang2hou.dito-dev" } else { "com.fang2hou.dito" };

/// Application name behind the data directory (`com.fang2hou.<name>` under Application Support).
pub(crate) const DATA_DIR_NAME: &str = if cfg!(debug_assertions) { "Dito-dev" } else { "Dito" };

#[cfg(test)]
mod tests {
    use crate::config::default_data_dir;

    #[test]
    fn debug_builds_never_point_at_the_installed_apps_data() {
        if cfg!(debug_assertions) {
            assert_eq!(super::BUNDLE_ID, "com.fang2hou.dito-dev");
            assert!(default_data_dir().ends_with("com.fang2hou.Dito-dev"), "{:?}", default_data_dir());
        } else {
            assert_eq!(super::BUNDLE_ID, "com.fang2hou.dito");
            assert!(default_data_dir().ends_with("com.fang2hou.Dito"), "{:?}", default_data_dir());
        }
    }
}
