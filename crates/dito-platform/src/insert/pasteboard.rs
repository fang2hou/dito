use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

use super::insert_error::InsertError;
use super::keyboard;

/// Every item's every type as raw bytes: a plain text snapshot would silently drop everything else the user had copied
/// (images, file references, rich text).
type PasteboardItems = Vec<Vec<(String, Vec<u8>)>>;

/// `None` when the pasteboard is unreadable, in which case no restore happens and the dictated text simply stays on
/// the clipboard.
fn snapshot_pasteboard() -> Option<PasteboardItems> {
    let pb = NSPasteboard::generalPasteboard();
    let items = pb.pasteboardItems()?;
    let mut out = Vec::with_capacity(items.len());
    for item in items.iter() {
        let mut types = Vec::new();
        for ty in item.types().iter() {
            let bytes = item.dataForType(&ty).map(|d| d.to_vec()).unwrap_or_default();
            types.push((ty.to_string(), bytes));
        }
        out.push(types);
    }
    Some(out)
}

fn restore_pasteboard(items: &PasteboardItems) -> bool {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    let new_items: Vec<Retained<NSPasteboardItem>> = items
        .iter()
        .map(|types| {
            let item = NSPasteboardItem::new();
            for (ty, bytes) in types {
                let data = NSData::with_bytes(bytes);
                let ty = NSString::from_str(ty);
                item.setData_forType(&data, &ty);
            }
            item
        })
        .collect();
    let writing: Vec<&ProtocolObject<dyn NSPasteboardWriting>> =
        new_items.iter().map(|item| ProtocolObject::from_ref(&**item)).collect();
    pb.writeObjects(&NSArray::from_slice(&writing))
}

/// `pasted_at` plus the floor keeps the restore from racing the target app's asynchronous pasteboard read; the
/// change-count guard keeps it from clobbering a clipboard the user (or a clipboard manager) has rewritten since. In
/// that case their content wins and the dictated text stays where it is.
#[derive(Debug)]
pub(super) struct PasteGuard {
    pasted_at: Instant,
    prev: Option<PasteboardItems>,
    count_after_write: isize,
}

impl PasteGuard {
    fn remaining(&self, floor: Duration) -> Duration {
        floor.saturating_sub(self.pasted_at.elapsed())
    }

    fn restore(&self) {
        let Some(prev) = &self.prev else { return };
        if NSPasteboard::generalPasteboard().changeCount() != self.count_after_write {
            tracing::info!("insert: clipboard changed since the paste; keeping the newer content");
            return;
        }
        restore_pasteboard(prev);
    }

    pub(super) fn restore_after(self, floor: Duration) {
        let wait = self.remaining(floor);
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            self.restore();
        });
    }

    /// Used before overwriting the clipboard with the fallback copy, which a pending background restore would clobber
    /// right after.
    pub(super) fn restore_blocking(self, floor: Duration) {
        std::thread::sleep(self.remaining(floor));
        self.restore();
    }
}

/// On failure the previous clipboard is restored before returning; on success that is the returned guard's job.
pub(super) fn begin_paste(text: &str) -> Result<PasteGuard, InsertError> {
    let prev = snapshot_pasteboard();
    copy_to_clipboard(text)?;
    let count_after_write = NSPasteboard::generalPasteboard().changeCount();
    if !keyboard::post_paste() {
        tracing::warn!("insert: could not send Cmd+V; restoring the clipboard");
        if let Some(prev) = &prev {
            restore_pasteboard(prev);
        }
        return Err(InsertError::PasteKeystroke);
    }
    Ok(PasteGuard { pasted_at: Instant::now(), prev, count_after_write })
}

const PLAIN_TEXT: &str = "public.utf8-plain-text";
const FILE_TYPES: [&str; 2] = ["public.file-url", "NSFilenamesPboardType"];
/// Settle polls after the first change: apps write their types one after the other.
const SETTLE_POLL: Duration = Duration::from_millis(12);
const SETTLE_TRIES: usize = 5;
const CHANGE_POLL: Duration = Duration::from_millis(8);

/// What an app's Copy command left on the pasteboard.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Copied {
    /// The copy could not be started, or the clipboard could not be read first; nothing was touched.
    NotStarted,
    /// The copy did not change the pasteboard, or what it wrote has no text.
    Nothing,
    Text(String),
    /// Editors copy the current line when nothing is selected, and say so in metadata of their own.
    CurrentLine,
    /// Files, not text, were copied (Finder).
    Files,
}

/// Runs `copy` (which must make the app copy its selection), reads the plain text it put on the pasteboard and puts the
/// previous contents back, all types, unless something else has written to the pasteboard since. `wait` bounds how long
/// the pasteboard may take to change; no change means the app had nothing to copy. `copy` returns `false` when it could
/// not be started, in which case the pasteboard has not been touched.
pub(super) fn read_after_copy(copy: impl FnOnce() -> bool, wait: Duration) -> Copied {
    let Some(prev) = snapshot_pasteboard() else {
        tracing::warn!("selection: the clipboard cannot be read; not copying the selection over it");
        return Copied::NotStarted;
    };
    let pb = NSPasteboard::generalPasteboard();
    let before = pb.changeCount();
    if !copy() {
        return Copied::NotStarted;
    }
    let deadline = Instant::now() + wait;
    while pb.changeCount() == before {
        if Instant::now() >= deadline {
            return Copied::Nothing;
        }
        std::thread::sleep(CHANGE_POLL);
    }
    let mut settled = pb.changeCount();
    for _ in 0..SETTLE_TRIES {
        std::thread::sleep(SETTLE_POLL);
        let now = pb.changeCount();
        if now == settled {
            break;
        }
        settled = now;
    }
    let text = pb.stringForType(&NSString::from_str(PLAIN_TEXT)).map(|text| text.to_string());
    let copied = classify(&snapshot_pasteboard().unwrap_or_default(), text);
    if pb.changeCount() == settled {
        restore_pasteboard(&prev);
    } else {
        tracing::info!("selection: clipboard changed while copying; keeping the newer content");
    }
    copied
}

fn classify(items: &PasteboardItems, text: Option<String>) -> Copied {
    let types = || items.iter().flatten();
    if types().any(|(ty, _)| FILE_TYPES.contains(&ty.as_str())) {
        return Copied::Files;
    }
    if types().any(|(ty, bytes)| ty != PLAIN_TEXT && marks_empty_selection(bytes)) {
        return Copied::CurrentLine;
    }
    match text {
        Some(text) if !text.is_empty() => Copied::Text(text),
        _ => Copied::Nothing,
    }
}

/// The metadata an editor attaches to a copy it made without a selection: Zed writes `"is_entire_line":true` in a JSON
/// type of its own, and VS Code and its forks write `"isFromEmptySelection":true` in `vscode-editor-data`. Looked for
/// in the data of any other type that is a JSON object or an array of objects, because those types carry dynamic names
/// the system derives itself, and copied code (HTML, RTF) may spell the same words.
fn marks_empty_selection(bytes: &[u8]) -> bool {
    const MARKS: [&[u8]; 2] = [b"\"is_entire_line\":true", b"\"isFromEmptySelection\":true"];
    // Metadata is a few hundred bytes; a large payload is content, not metadata.
    if bytes.len() > 16 * 1024 {
        return false;
    }
    let compact: Vec<u8> = bytes.iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    if !(compact.starts_with(b"{\"") || compact.starts_with(b"[{\"")) {
        return false;
    }
    MARKS.iter().any(|mark| compact.windows(mark.len()).any(|window| window == *mark))
}

/// Also used by the history UI for its copy action.
pub fn copy_to_clipboard(text: &str) -> Result<(), InsertError> {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Copied, PLAIN_TEXT, PasteboardItems, classify, marks_empty_selection};

    fn item(types: &[(&str, &str)]) -> PasteboardItems {
        vec![types.iter().map(|(ty, data)| (ty.to_string(), data.as_bytes().to_vec())).collect()]
    }

    const ZED_LINE: &str = r#"[{"len":28,"is_entire_line":true,"first_line_indent":0,"file_path":"/a.txt","line_range":{"start":1,"end":2}}]"#;
    const ZED_SELECTION: &str = r#"[{"len":4,"is_entire_line":false,"first_line_indent":0}]"#;
    const VSCODE_LINE: &str = r#"{"version":1,"isFromEmptySelection":true,"multicursorText":null,"mode":"plaintext"}"#;

    #[test]
    fn a_line_copied_without_a_selection_is_recognised_in_zed_and_vs_code() {
        for (name, meta) in [("dyn.zed", ZED_LINE), ("dyn.vscode", VSCODE_LINE)] {
            let items = item(&[(PLAIN_TEXT, "a line\n"), (name, meta)]);
            assert_eq!(classify(&items, Some("a line\n".into())), Copied::CurrentLine, "{name}");
        }
    }

    #[test]
    fn a_whole_line_that_was_selected_is_text() {
        let items = item(&[(PLAIN_TEXT, "a line\n"), ("dyn.zed", ZED_SELECTION)]);
        assert_eq!(classify(&items, Some("a line\n".into())), Copied::Text("a line\n".into()));
    }

    #[test]
    fn copied_code_that_spells_the_metadata_is_text() {
        let html = format!("<pre>{ZED_LINE}</pre>");
        let items = item(&[(PLAIN_TEXT, ZED_LINE), ("public.html", &html)]);
        assert_eq!(classify(&items, Some(ZED_LINE.into())), Copied::Text(ZED_LINE.into()));
        assert!(!marks_empty_selection(html.as_bytes()));
    }

    #[test]
    fn files_and_empty_copies_are_not_a_selection() {
        let files = item(&[(PLAIN_TEXT, "notes.txt"), ("public.file-url", "file:///notes.txt")]);
        assert_eq!(classify(&files, Some("notes.txt".into())), Copied::Files);
        assert_eq!(classify(&item(&[("public.png", "x")]), None), Copied::Nothing);
        assert_eq!(classify(&item(&[(PLAIN_TEXT, "")]), Some(String::new())), Copied::Nothing);
    }
}
