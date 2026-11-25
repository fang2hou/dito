use anyhow::Result;
use dito_core::config::EngineLabels;
use dito_core::intent::{Decision, Intent, RewriteOp};
use dito_core::prompts::AppTarget;
use dito_core::store::{
    CommandDetails, CommandOutcome, HistoryKind, InstructionOrigin, NewHistory, SourceKind, Sources, Speaker,
    StepMillis, Store, ToolChoice,
};

const SAMPLE_WORDS: [&str; 5] = ["Acme", "Contoso", "gpui-kit", "サンプル株式会社", "Bedrock"];
const DAILY_CHARS: [usize; 6] = [1200, 980, 1560, 1340, 1820, 560];
const DAILY_APPS: [&str; 4] = ["Slack", "Gmail", "Notion", "Teams"];

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
    transcript(store, now)
}

/// The newest transcript's tail while it is shown as still recording: a paragraph waiting for polishing, then the
/// settled and the unsettled words of the paragraph still being heard.
#[cfg(feature = "screenshots")]
pub const LIVE_TAIL: [&str; 3] = [
    "残りの二件は調査中ですが、こちらも木曜のリリースで直る見込みです。",
    "最後に来週の予定です。",
    "月曜は全社会議があるので、定例は火曜に移します。",
];

/// A meeting played to a transcription session in real time: each line, how long it is spoken and the pause after it.
#[cfg(feature = "screenshots")]
pub const FILM_SCRIPT: [(&str, f32, f32); 9] = [
    ("えーと、それでは定例を始めます。", 2.5, 0.7),
    ("今日は、あの、リリース計画と、問い合わせ対応の二点です。", 3.5, 0.7),
    ("まずリリースですが、来週の水曜に、いや木曜に、ステージングに出す予定です。", 4.5, 0.6),
    ("結合テストはほぼ終わっていて、残りは決済まわりだけです。", 3.5, 3.0),
    ("はい、決済の件なんですけど、えー、テスト環境の証明書が切れていて、昨日から止まってます。", 5.5, 0.6),
    ("今日中に更新できれば、木曜には間に合うと思います。", 3.0, 3.0),
    ("では次に、問い合わせ対応です。先週は、えー、ログインできないという問い合わせが十二件ありました。", 6.0, 0.7),
    ("ほとんどがパスワードの再設定で解決していますが、二件はまだ調査中です。", 4.0, 0.6),
    ("こちらも、あの、木曜のリリースで直る見込みです。", 3.0, 0.0),
];

/// One finished meeting recorded with the microphone, so both speakers show.
fn transcript(store: &Store, now: i64) -> Result<()> {
    let started = now - 3 * 3600;
    let id = store.create_transcript(started, Sources { system: true, microphone: true })?;
    store.set_transcript_title(id, "週次定例")?;
    let paragraphs = [
        (
            Speaker::Others,
            4_000,
            21_000,
            "それでは定例を始めます。今日はリリース計画と問い合わせ対応の二点です。来週の木曜にステージングに出す予定で、結合テストは決済周りを残すだけです。",
        ),
        (
            Speaker::Me,
            23_500,
            31_000,
            "決済の件ですが、テスト環境の証明書が切れていて昨日から止まっています。今日中に更新できれば、木曜に間に合います。",
        ),
        (
            Speaker::Others,
            34_000,
            49_000,
            "では次に問い合わせ対応です。先週はログインできないという問い合わせが 12 件ありました。ほとんどはパスワードの再設定で解決しています。",
        ),
    ];
    for (speaker, start_ms, end_ms, text) in paragraphs {
        let p = store.add_paragraph(id, speaker, start_ms, end_ms, text)?;
        store.finish_paragraph(p, text, None)?;
    }
    store.end_transcript(id, 52 * 60 * 1000, started + 52 * 60)?;
    Ok(())
}

fn entry(ts: i64, kind: HistoryKind, target: AppTarget, app: &str, lang: &str, duration_ms: u64) -> NewHistory {
    NewHistory {
        ts,
        kind,
        style: target,
        app: app.into(),
        lang: lang.into(),
        duration_ms,
        raw: String::new(),
        polished: None,
        error: None,
        audio_path: None,
        engines: EngineLabels::default(),
        timings: StepMillis::default(),
        source: None,
        command: None,
    }
}

fn command(
    ts: i64,
    instruction: &str,
    source: (SourceKind, &str),
    result: &str,
    decision: Decision,
    choice: ToolChoice,
    outcome: CommandOutcome,
) -> NewHistory {
    NewHistory {
        raw: instruction.into(),
        polished: Some(result.into()),
        source: Some(source.1.into()),
        engines: EngineLabels { asr: Some("local:sample-asr".into()), polish: Some("local:sample-chat".into()) },
        timings: StepMillis { asr: Some(420), polish: Some(1_800) },
        command: Some(CommandDetails {
            origin: Some(InstructionOrigin::Spoken),
            tool: Some(decision.intent),
            decision: Some(decision),
            choice: Some(choice),
            source: Some(source.0),
            outcome,
            route_ms: Some(210),
            router_engine: Some("local:sample-chat".into()),
        }),
        ..entry(ts, HistoryKind::Command, AppTarget::General, "Notes", "ja", 2_600)
    }
}

fn dictation(
    ts: i64,
    target: AppTarget,
    app: &str,
    lang: &str,
    duration_ms: u64,
    raw: &str,
    polished: &str,
) -> NewHistory {
    NewHistory {
        raw: raw.into(),
        polished: Some(polished.into()),
        ..entry(ts, HistoryKind::Dictation, target, app, lang, duration_ms)
    }
}

fn recent_entries(now: i64) -> [NewHistory; 8] {
    [
        command(
            now - 300,
            "もっと短くして",
            (
                SourceKind::Selection,
                "明日の会議は、午後4時から、第2会議室で開催いたしますので、ご出席をお願いいたします。",
            ),
            "明日16時から第2会議室で会議です。ご出席ください。",
            Decision {
                intent: Intent::Rewrite,
                confidence: 0.96,
                probabilities: [0.01, 0.96, 0.01, 0.01, 0.005, 0.002, 0.002, 0.001],
                rewrite_op: Some((RewriteOp::Shorten, 0.93)),
            },
            ToolChoice::Confident,
            CommandOutcome::Replaced,
        ),
        command(
            now - 450,
            "これを英語にして",
            (SourceKind::Entered, "金曜日までに議事録を送ってください。"),
            "Please send me the minutes by Friday.",
            Decision {
                intent: Intent::Translate,
                confidence: 0.62,
                probabilities: [0.2, 0.08, 0.62, 0.05, 0.01, 0.01, 0.01, 0.02],
                rewrite_op: None,
            },
            ToolChoice::UserChoice,
            CommandOutcome::Inserted,
        ),
        dictation(
            now - 600,
            AppTarget::builtin("slack").expect("built-in app rules"),
            "Slack",
            "ja",
            9_000,
            "えーと 明日の3時の、あのレビュー会議 4時に変えて、で、えっと Dito のプロトタイプを 見といてください",
            "明日15時のレビュー会議は16時に変更します。事前にDitoのプロトタイプをご確認ください。",
        ),
        entry(now - 1_000, HistoryKind::Cancelled, AppTarget::General, "Notes", "", 8_000),
        dictation(
            now - 1_400,
            AppTarget::General,
            "Mail",
            "en",
            5_000,
            "um thanks I'll send the revised proposal Monday",
            "Thanks, I'll send the revised proposal Monday.",
        ),
        dictation(
            now - 1_900,
            AppTarget::General,
            "WeChat",
            "zh",
            4_000,
            "请把周五的会议纪要发给我 谢谢",
            "请把周五的会议纪要发给我，谢谢。",
        ),
        dictation(
            now - 2_300,
            AppTarget::builtin("teams").expect("built-in app rules"),
            "Teams",
            "ja",
            6_000,
            "本日の定例は15分遅れで開始します",
            "本日の定例は15分遅れで開始します。",
        ),
        NewHistory {
            error: Some("Auth: HTTP 401: invalid api key".into()),
            ..entry(now - 2_600, HistoryKind::Failed, AppTarget::General, "Notes", "", 3_000)
        },
    ]
}

fn daily_volume(now: i64) -> Vec<NewHistory> {
    DAILY_CHARS
        .iter()
        .enumerate()
        .map(|(i, &chars)| {
            let text = "あ".repeat(chars);
            let lang = if i % 3 == 2 { "en" } else { "ja" };
            dictation(
                now - 86_400 * (6 - i as i64),
                AppTarget::General,
                DAILY_APPS[i % DAILY_APPS.len()],
                lang,
                chars as u64 * 340,
                &text,
                &text,
            )
        })
        .collect()
}
