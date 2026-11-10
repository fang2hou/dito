use anyhow::Result;
use dito_core::config::EngineLabels;
use dito_core::prompts::Style;
use dito_core::store::{HistoryKind, NewHistory, Store};

const SAMPLE_WORDS: [&str; 5] = ["Acme", "Contoso", "gpui-kit", "サンプル株式会社", "Bedrock"];
const DAILY_CHARS: [usize; 6] = [1200, 980, 1560, 1340, 1820, 560];
const DAILY_APPS: [&str; 4] = ["Slack", "Gmail", "Notion", "Teams"];

/// Sample history and dictionary so a fresh mock run has something to show.
pub fn demo(store: &Store, now: i64) -> Result<()> {
    if !store.list_history(&Default::default())?.is_empty() {
        return Ok(());
    }
    for word in SAMPLE_WORDS {
        store.add_word(word, now)?;
    }
    for entry in recent_entries(now).iter().chain(&daily_volume(now)) {
        store.add_history(entry)?;
    }
    Ok(())
}

fn entry(ts: i64, kind: HistoryKind, style: Style, app: &str, lang: &str, duration_ms: u64) -> NewHistory {
    NewHistory {
        ts,
        kind,
        style,
        app: app.into(),
        lang: lang.into(),
        duration_ms,
        raw: String::new(),
        polished: None,
        error: None,
        audio_path: None,
        engines: EngineLabels::default(),
    }
}

fn dictation(ts: i64, style: Style, app: &str, lang: &str, duration_ms: u64, raw: &str, polished: &str) -> NewHistory {
    NewHistory {
        raw: raw.into(),
        polished: Some(polished.into()),
        ..entry(ts, HistoryKind::Dictation, style, app, lang, duration_ms)
    }
}

fn recent_entries(now: i64) -> [NewHistory; 6] {
    [
        dictation(
            now - 600,
            Style::Slack,
            "Slack",
            "ja",
            9_000,
            "えーと 明日の3時の、あのレビュー会議 4時に変えて、で、えっと Dito のプロトタイプを 見といてください",
            "明日15時のレビュー会議は16時に変更します。事前にDitoのプロトタイプをご確認ください。",
        ),
        entry(now - 1_000, HistoryKind::Cancelled, Style::General, "Notes", "", 8_000),
        dictation(
            now - 1_400,
            Style::General,
            "Mail",
            "en",
            5_000,
            "um thanks I'll send the revised proposal Monday",
            "Thanks, I'll send the revised proposal Monday.",
        ),
        dictation(
            now - 1_900,
            Style::General,
            "WeChat",
            "zh",
            4_000,
            "请把周五的会议纪要发给我 谢谢",
            "请把周五的会议纪要发给我，谢谢。",
        ),
        dictation(
            now - 2_300,
            Style::Teams,
            "Teams",
            "ja",
            6_000,
            "本日の定例は15分遅れで開始します",
            "本日の定例は15分遅れで開始します。",
        ),
        NewHistory {
            error: Some("Auth: HTTP 401: invalid api key".into()),
            ..entry(now - 2_600, HistoryKind::Failed, Style::General, "Notes", "", 3_000)
        },
    ]
}

/// One long dictation per day for the last six days, so the statistics page has a week to draw.
fn daily_volume(now: i64) -> Vec<NewHistory> {
    DAILY_CHARS
        .iter()
        .enumerate()
        .map(|(i, &chars)| {
            let text = "あ".repeat(chars);
            let lang = if i % 3 == 2 { "en" } else { "ja" };
            dictation(
                now - 86_400 * (6 - i as i64),
                Style::General,
                DAILY_APPS[i % DAILY_APPS.len()],
                lang,
                chars as u64 * 340,
                &text,
                &text,
            )
        })
        .collect()
}
