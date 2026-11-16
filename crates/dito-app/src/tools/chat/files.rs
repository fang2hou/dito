//! What the composer shows about an attached file or meeting: its kind, its size and a duration.

#[derive(Debug, PartialEq, Eq)]
pub(super) enum FileKind {
    Markdown,
    Text,
    Code,
}

pub(super) fn file_kind(name: &str) -> FileKind {
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match extension.as_str() {
        "md" | "markdown" => FileKind::Markdown,
        "txt" | "text" | "log" | "" => FileKind::Text,
        _ => FileKind::Code,
    }
}

/// `4 KB`, `812 B` or `1.2 MB`.
pub(super) fn size_label(bytes: u64) -> String {
    const KB: f64 = 1024.;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{} KB", (b / KB).round().max(1.))
    } else {
        format!("{:.1} MB", b / (KB * KB))
    }
}

pub(super) fn clock(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_follow_the_extension_and_sizes_read_in_the_nearest_unit() {
        assert_eq!(file_kind("notes.MD"), FileKind::Markdown);
        assert_eq!(file_kind("a.txt"), FileKind::Text);
        assert_eq!(file_kind("Makefile"), FileKind::Text);
        assert_eq!(file_kind("main.rs"), FileKind::Code);
        assert_eq!(size_label(812), "812 B");
        assert_eq!(size_label(4096), "4 KB");
        assert_eq!(size_label(512 * 1024), "512 KB");
        assert_eq!(size_label(3 * 1024 * 1024 / 2), "1.5 MB");
        assert_eq!(clock(2_536_000), "42:16");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}
