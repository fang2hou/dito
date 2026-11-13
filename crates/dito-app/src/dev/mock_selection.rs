use std::sync::Arc;

use dito_core::config::InsertMethod;
use dito_core::controller::{CapturedSelection, Delivery, SelectionSource};
use dito_core::surrounding::Surrounding;

const SAMPLE: &str = "お疲れ様です。昨日の件ですが、資料の確認がまだ終わっていないので、返事は明日の午前中でもいいでしょうか。\
                      急ぎでしたら、先に概要だけお送りします。よろしくお願いします。";
const BEFORE: &str =
    "件名：資料の確認について\n\n田中さん\n\n先週の打ち合わせでは、いろいろとありがとうございました。\n\n";
const AFTER: &str =
    "\n\n来週の定例会議の資料も、金曜日までに共有する予定です。ご不明な点があれば、いつでも声をかけてください。\n山田";

/// A fixed Japanese selection inside a longer text, as the focused field holds it. `DITO_MOCK_SELECTION=none` selects
/// nothing instead, with the same text around the caret, which reaches the needs-text flow.
pub struct MockSelection {
    selected: bool,
}

impl MockSelection {
    pub fn from_env() -> Self {
        Self { selected: std::env::var("DITO_MOCK_SELECTION").map_or(true, |v| v != "none") }
    }
}

impl SelectionSource for MockSelection {
    fn capture(&self) -> Option<Arc<dyn CapturedSelection>> {
        let surrounding = Surrounding::Around { before: BEFORE.into(), after: AFTER.into() };
        Some(Arc::new(MockCaptured { selected: self.selected, surrounding }))
    }
}

struct MockCaptured {
    selected: bool,
    surrounding: Surrounding,
}

impl CapturedSelection for MockCaptured {
    fn text(&self) -> &str {
        if self.selected { SAMPLE } else { "" }
    }

    fn surrounding(&self) -> Option<&Surrounding> {
        Some(&self.surrounding)
    }

    fn editable(&self) -> bool {
        true
    }

    fn replace(&self, text: &str, _: InsertMethod) -> Delivery {
        tracing::info!("would replace the selection: {text}");
        Delivery::Inserted
    }
}
