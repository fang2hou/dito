//! The chat page of the Tools window in both appearances: a new chat, a conversation, the Markdown typography, a
//! reply that streams, stops and is generated again, the pickers, the compact window and the switching motion.
//! `DITO_SHOTS_ONLY=chat`.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dito_core::config::{Appearance, DisplayLanguage};
use dito_core::store::{Attachment, Author, MessageState, NewChatMessage};
use gpui_kit::*;

use super::capture::{close_window, render_tools_window, resize_tools_window, save, theme_tag};
use super::input::on_tools_window;
use crate::services;
use crate::state::AppState;
use crate::timefmt::now;
use crate::tools;

const NOTES: &str = "# Release plan\n\n- proposal\n- prototype\n";
const ANSWER: &str = "次回までの対応は、以下の3つです。\n\n1. **提案書を更新する**\n   費用の内訳と作業範囲を追記します。\n2. **試作の確認項目をそろえる**\n   担当ごとに動作確認の結果を共有します。\n3. **次回の日程を確定する**\n   候補をまとめ、参加者に確認します。";
const TYPOGRAPHY: &str = "# リリース前の確認\n\n今週中に**3つの作業**を終えます。設定は `config.toml` に保存されます。\n\n## 作業\n\n- 表示言語の切り替えを確認する\n- ショートカットの競合を調べる\n\n1. ビルドを作る\n2. 署名を確認する\n\n> リリースノートは日本語と英語の両方で用意します。\n\n```bash\ncurl http://127.0.0.1:17841/v1/models \\\n  -H \"Authorization: Bearer $DITO_TOKEN\"\n```\n\n| 項目 | 担当 | 期限 |\n|---|---|---|\n| 表示言語 | 山田 | 11月18日 |\n| ショートカット | 佐藤 | 11月20日 |\n\n詳しくは[リリース手順](https://example.test)を参照してください。";

struct Seeded {
    summary: i64,
    typography: i64,
    polish: i64,
}

fn seed(st: &Arc<AppState>) -> Seeded {
    let store = &st.store;
    let ts = now();
    let chat = |title: &str, at: i64, turns: &[(&str, Vec<Attachment>, &str)]| {
        let id = store.create_chat(title, at).expect("chat");
        for (i, (question, attachments, answer)) in turns.iter().enumerate() {
            let t = at + i as i64 * 2;
            let user = NewChatMessage {
                author: Author::User,
                content: question,
                state: MessageState::Done,
                attachments,
                model: None,
                ts: t,
            };
            store.add_chat_message(id, &user).expect("question");
            let reply = NewChatMessage {
                author: Author::Assistant,
                content: answer,
                state: MessageState::Done,
                attachments: &[],
                model: Some("Qwen3.5-4B"),
                ts: t + 1,
            };
            store.add_chat_message(id, &reply).expect("reply");
        }
        id
    };
    let file = Attachment::File { name: "議事メモ.md".into(), bytes: 4096, text: NOTES.into() };
    let polish = chat(
        "文面を整える",
        ts - 30,
        &[
            ("この文面を丁寧に整えて。", vec![], "以下のように整えました。"),
            ("もう少し短くして。", vec![], "承知しました。短くまとめます。"),
        ],
    );
    let typography = chat("リリース前の確認", ts - 20, &[("リリース前の確認事項を教えて。", vec![], TYPOGRAPHY)]);
    let summary = chat("議事メモの要約", ts - 10, &[("次の会議までにやることを3つにまとめて。", vec![file], ANSWER)]);
    Seeded { summary, typography, polish }
}

async fn settle(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
}

async fn shot(cx: &mut AsyncApp, dir: &Path, name: &str) {
    settle(cx, 500).await;
    if let Some(Ok(img)) = render_tools_window(cx) {
        save(dir, name, img);
    }
}

fn on_chat(cx: &mut AsyncApp, f: impl FnOnce(&mut Window, &mut App, &Entity<crate::tools::chat::ChatPage>) + 'static) {
    on_tools_window(cx, |w, app, view| {
        let chat = view.read(app).chat().clone();
        f(w, app, &chat);
    });
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    let original = st.cfg.get();
    let ids = seed(st);
    for appearance in [Appearance::Light, Appearance::Dark] {
        let tag = theme_tag(appearance);
        services::save_config(st, |c| {
            c.general.appearance = appearance;
            c.general.display_language = DisplayLanguage::Ja;
        });
        cx.update(|cx| {
            tools::open(st.clone(), cx);
            resize_tools_window(cx, 1120., 680.);
        });
        on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.show_chat(cx)));
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_open(None, cx)));
        shot(cx, dir, &format!("chat-new-{tag}.png")).await;

        open(cx, Some(ids.summary));
        shot(cx, dir, &format!("chat-conversation-{tag}.png")).await;

        open(cx, Some(ids.typography));
        shot(cx, dir, &format!("chat-markdown-{tag}.png")).await;
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_scroll_up(cx)));
        shot(cx, dir, &format!("chat-markdown-scrolled-{tag}.png")).await;

        open(cx, Some(ids.summary));
        on_chat(cx, |w, app, chat| {
            chat.update(app, |p, cx| {
                p.pose_attach(Attachment::File { name: "議事メモ.md".into(), bytes: 4096, text: NOTES.into() }, cx);
                p.pose_attach(
                    Attachment::Meeting {
                        transcript_id: 1,
                        title: "リリース計画の確認".into(),
                        started_ts: now(),
                        duration_ms: Some(2_536_000),
                        text: "決定".into(),
                    },
                    cx,
                );
                p.pose_type("この会議の決定事項を、議事メモの形式でまとめて。", w, cx);
            })
        });
        shot(cx, dir, &format!("chat-attachments-{tag}.png")).await;
        on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_picker(w, cx)));
        shot(cx, dir, &format!("chat-picker-{tag}.png")).await;
        on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_reset(w, cx)));

        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_menu(p.pose_sessions()[0], cx)));
        shot(cx, dir, &format!("chat-menu-{tag}.png")).await;
        on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_reset(w, cx)));

        let asking = "提案書の締め切りはいつ？";
        let partial = "11月20日です。\n\n1. **提案書を更新する**";
        for (name, ms) in [("20", 320), ("50", 800), ("80", 1280)] {
            open(cx, Some(ids.summary));
            on_chat(cx, move |_, app, chat| {
                chat.update(app, |p, cx| p.pose_reply(asking, partial, MessageState::Generating, ms, cx))
            });
            shot(cx, dir, &format!("chat-generating-{name}-{tag}.png")).await;
        }
        for (name, state) in [("stopped", MessageState::Stopped), ("failed", MessageState::Failed)] {
            open(cx, Some(ids.summary));
            on_chat(cx, move |_, app, chat| chat.update(app, |p, cx| p.pose_reply(asking, partial, state, 0, cx)));
            shot(cx, dir, &format!("chat-{name}-{tag}.png")).await;
        }

        open(cx, Some(ids.summary));
        on_chat(cx, |w, app, chat| {
            chat.update(app, |p, cx| {
                p.pose_type("提案書の締め切りはいつ？", w, cx);
                p.pose_send(w, cx);
            })
        });
        settle(cx, 1500).await;
        shot(cx, dir, &format!("chat-sent-{tag}.png")).await;
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_retry(cx)));

        open(cx, None);
        on_chat(cx, |w, app, chat| {
            chat.update(app, |p, cx| {
                p.pose_type("新しい話題です。", w, cx);
                p.pose_send(w, cx);
            })
        });
        settle(cx, 1500).await;
        shot(cx, dir, &format!("chat-draft-sent-{tag}.png")).await;

        open(cx, Some(ids.summary));
        on_chat(cx, |w, app, chat| chat.update(app, |p, cx| p.pose_rename(p.pose_sessions()[0], w, cx)));
        shot(cx, dir, &format!("chat-rename-{tag}.png")).await;

        open(cx, Some(ids.summary));
        settle(cx, 300).await;
        on_chat(cx, move |_, app, chat| chat.update(app, |p, cx| p.pose_switch(Some(ids.polish), cx)));
        let start = Instant::now();
        for ms in [0u64, 60, 150, 270, 390, 470] {
            let due = Duration::from_millis(ms);
            settle(cx, due.saturating_sub(start.elapsed()).as_millis() as u64).await;
            if let Some(Ok(img)) = render_tools_window(cx) {
                save(dir, &format!("chat-switch-{ms:03}-{tag}.png"), img);
            }
        }

        cx.update(|cx| resize_tools_window(cx, 900., 680.));
        open(cx, Some(ids.summary));
        shot(cx, dir, &format!("chat-compact-{tag}.png")).await;
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_scroll_up(cx)));
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_drawer(true, cx)));
        shot(cx, dir, &format!("chat-drawer-{tag}.png")).await;
        on_chat(cx, |_, app, chat| chat.update(app, |p, cx| p.pose_drawer(false, cx)));

        if let Some((window, _)) = cx.update(|cx| tools::handle(cx)) {
            close_window(cx, window);
        }
    }
    services::save_config(st, |c| {
        c.general.appearance = original.general.appearance;
        c.general.display_language = original.general.display_language;
    });
}

fn open(cx: &mut AsyncApp, id: Option<i64>) {
    on_chat(cx, move |w, app, chat| {
        chat.update(app, |p, cx| {
            p.pose_reset(w, cx);
            p.pose_open(id, cx);
        })
    });
}
