use std::path::PathBuf;

pub fn default_data_dir() -> PathBuf {
    directories::ProjectDirs::from("com", "fang2hou", crate::identity::DATA_DIR_NAME)
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".dito"))
}
