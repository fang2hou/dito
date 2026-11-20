use std::sync::Arc;

use dito_core::config::InsertMethod;
use dito_core::controller::{CapturedSelection, Delivery, SelectionSource};
use dito_core::surrounding::Surrounding;

use super::selection::{Selection, capture_selection};

/// Reads the selection and the text of the focused field of the app in front through Accessibility when a command
/// starts.
pub struct MacSelection;

impl SelectionSource for MacSelection {
    fn capture(&self) -> Option<Arc<dyn CapturedSelection>> {
        capture_selection().map(|selection| Arc::new(Captured::new(selection)) as Arc<dyn CapturedSelection>)
    }
}

struct Captured {
    selection: Selection,
    surrounding: Option<Surrounding>,
}

impl Captured {
    fn new(selection: Selection) -> Self {
        let surrounding =
            selection.field.as_ref().and_then(|field| Surrounding::locate(&field.value, &selection.text, field.range));
        Self { selection, surrounding }
    }
}

impl CapturedSelection for Captured {
    fn text(&self) -> &str {
        &self.selection.text
    }

    fn surrounding(&self) -> Option<&Surrounding> {
        self.surrounding.as_ref()
    }

    fn editable(&self) -> bool {
        self.selection.editable
    }

    fn replace(&self, text: &str, method: InsertMethod) -> Delivery {
        self.selection.replace(text, method)
    }
}
