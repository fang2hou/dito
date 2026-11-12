use dito_core::i18n::Lang;
use dito_core::prompts::Prompts;
use dito_core::store::HistoryEntry;

use crate::timefmt::{hhmm, local_day, ymd};
use crate::ui::history_style_label;

/// Without the prompts a removal would leave raw bundle ids in the export.
pub(super) fn history_csv(rows: &[HistoryEntry], prompts: &Prompts, lang: Lang, utc_offset_secs: i32) -> String {
    let quoted = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
    let mut out = String::from("date,time,app,style,text,original\n");
    for e in rows {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            ymd(local_day(e.ts, utc_offset_secs)),
            hhmm(e.ts, utc_offset_secs),
            quoted(&e.app),
            quoted(&history_style_label(lang, prompts, &e.style, &e.app)),
            quoted(e.final_text()),
            quoted(&e.raw)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dito_core::config::EngineLabels;
    use dito_core::prompts::Style;
    use dito_core::store::{HistoryEntry, HistoryKind};

    fn row(style: Style, app: &str) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            ts: 0,
            kind: HistoryKind::Dictation,
            style,
            app: app.into(),
            lang: "en".into(),
            duration_ms: 1_000,
            raw: "hello".into(),
            polished: None,
            error: None,
            audio_path: None,
            engines: EngineLabels::default(),
            timings: Default::default(),
            source: None,
            command: None,
        }
    }

    #[test]
    fn style_column_follows_the_configured_apps() {
        let mut prompts = Prompts::default();
        prompts.add_application("com.apple.AppStore", "App Store", &Style::General);
        let rows = [
            row(Style::General, "Dia"),
            row(Style::Application("com.apple.AppStore".into()), "App Store"),
            row(Style::Application("example.gone".into()), "Old App"),
            row(Style::builtin("slack").expect("built-in style"), "Slack"),
            row(Style::General, "App Store"),
        ];
        let csv = history_csv(&rows, &prompts, Lang::En, 0);
        let styles: Vec<&str> = csv.lines().skip(1).map(|line| line.split(',').nth(3).unwrap()).collect();
        assert_eq!(
            styles,
            ["\"Other\"", "\"App Store\"", "\"Other\"", "\"Slack\"", "\"App Store\""],
            "rows recorded before the app was configured recover it by name"
        );
    }
}
