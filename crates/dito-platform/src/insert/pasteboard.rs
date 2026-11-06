use std::time::{Duration, Instant};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardItem, NSPasteboardWriting};
use objc2_foundation::{NSArray, NSData, NSString};

use super::insert_error::InsertError;
use super::keyboard;

/// The whole pasteboard at one moment: every item's every type as raw bytes. A plain text
/// snapshot would silently drop everything else the user had copied (images, file references,
/// rich text).
type PasteboardItems = Vec<Vec<(String, Vec<u8>)>>;

/// Read every pasteboard item's every type. `None` when the pasteboard is unreadable, in which
/// case no restore happens and the dictated text simply stays on the clipboard.
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

/// Write `items` back to the pasteboard, replacing whatever is there now.
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

/// Restores the user's clipboard after a paste attempt. `pasted_at` plus the floor keeps the
/// restore from racing the target app's asynchronous pasteboard read; the change-count guard
/// keeps it from clobbering a clipboard the user (or a clipboard manager) has rewritten since;
/// in that case their content wins and the dictated text stays where it is.
#[derive(Debug)]
pub(super) struct PasteGuard {
    pasted_at: Instant,
    prev: Option<PasteboardItems>,
    /// The pasteboard's change count right after our own write.
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

    /// Restore in the background; delivery has been accepted.
    pub(super) fn restore_after(self, floor: Duration) {
        let wait = self.remaining(floor);
        std::thread::spawn(move || {
            std::thread::sleep(wait);
            self.restore();
        });
    }

    /// Restore on the spot; used before overwriting the clipboard with the fallback copy, which a
    /// pending background restore would clobber right after.
    pub(super) fn restore_blocking(self, floor: Duration) {
        std::thread::sleep(self.remaining(floor));
        self.restore();
    }
}

/// Put `text` on the clipboard and send Cmd+V. On failure the previous clipboard is restored
/// before returning; on success that is the returned guard's job.
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

/// Put `text` on the clipboard (also used by the history UI for its copy action).
pub fn copy_to_clipboard(text: &str) -> Result<(), InsertError> {
    arboard::Clipboard::new().and_then(|mut c| c.set_text(text.to_string()))?;
    Ok(())
}
