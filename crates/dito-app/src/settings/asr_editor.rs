use super::Ctx;
use crate::{
    state::AppState,
    ui::{self, BtnKind, cbk, text},
};
use dito_core::i18n::Tr;
use dito_core::provider::configured_asr::{AsrProfiles, validate};
use gpui_kit::{
    component::input::{Editor, EditorState, InputEvent},
    prelude::*,
    *,
};
use serde_json::Value;
use std::{rc::Rc, sync::Arc, time::Duration};

pub struct Closed(pub bool);
pub struct AsrEditor {
    st: Arc<AppState>,
    source: Entity<EditorState>,
    drafts: Vec<String>,
    selected: usize,
    error: Option<String>,
    pending: bool,
    revision: u64,
    help: bool,
    highlighter_dark: Option<bool>,
    help_scroll: ScrollHandle,
    motion: ui::MotionScope,
}
impl EventEmitter<Closed> for AsrEditor {}
impl AsrEditor {
    fn translate(&self, value: &str) -> SharedString {
        let key = match value {
            "接続設定" => Tr::AsrEditorTitle,
            "設定の書き方" => Tr::AsrEditorHelpTitle,
            "テンプレート" => Tr::AsrEditorTemplate,
            "編集" => Tr::AsrEditorEdit,
            "自動チェック" => Tr::AsrEditorCheck,
            "保存" => Tr::AsrEditorSave,
            "設定の構成" => Tr::AsrEditorStructure,
            "形式のバージョン" => Tr::AsrEditorVersion,
            "接続先とモデル" => Tr::AsrEditorConnection,
            "言語コードの変換" => Tr::AsrEditorLanguageMap,
            "送信するデータ" => Tr::AsrEditorRequest,
            "認識結果の取得先" => Tr::AsrEditorResponse,
            "接続先が JP / EN / ZH を使う場合の例" => Tr::AsrEditorExample,
            "完全なタグ → 言語コード → other" => Tr::AsrEditorOrder,
            "auto が null なら、言語を指定しません。" => Tr::AsrEditorAuto,
            "使用できる変数" => Tr::AsrEditorVariables,
            "モデル ID" => Tr::AsrEditorModel,
            "変換後の言語コード" => Tr::AsrEditorMapped,
            "API キー" => Tr::AsrEditorKey,
            "Base64 の音声" => Tr::AsrEditorBase64,
            "Data URI の音声" => Tr::AsrEditorDataUri,
            "WAV ファイル" => Tr::AsrEditorWav,
            "送信内容と認識結果" => Tr::AsrEditorIo,
            "ベース URL の末尾に追加します。" => Tr::AsrEditorPath,
            "認識した文章の場所を指定します。" => Tr::AsrEditorPointer,
            "AI 用の説明をコピー" => Tr::AsrEditorCopyHelp,
            "言語コード" => Tr::AsrEditorLanguage,
            "設定をコピー" => Tr::AsrEditorCopy,
            "確認しています…" => Tr::AsrEditorPending,
            "エラーはありません" => Tr::AsrEditorValid,
            "入力内容の確認" => Tr::AsrEditorValidation,
            "ヘルプ" => Tr::AsrEditorHelp,
            "キャンセル" => Tr::AsrEditorCancel,
            _ => return value.to_owned().into(),
        };
        self.st.cfg.get().general.lang().tr(key).into()
    }

    pub fn new(st: Arc<AppState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let p = st.cfg.get().asr.cloud_asr.clone().unwrap_or_default();
        let defaults = AsrProfiles::default();
        let drafts = (0..3)
            .map(|i| serde_json::to_string_pretty(p.profiles.get(i).unwrap_or(&defaults.profiles[i])).unwrap())
            .collect::<Vec<_>>();
        let selected = p.selected.min(2);
        let source = cx.new(|cx| {
            EditorState::new(window, cx).language("json").folding(false).default_value(drafts[selected].clone())
        });
        cx.subscribe_in(&source, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                this.changed(cx);
            }
        })
        .detach();
        let focus = source.read(cx).focus_handle(cx);
        focus.focus(window, cx);
        let error = check(&drafts[selected], st.cfg.get().general.lang()).err();
        Self {
            st,
            source,
            drafts,
            selected,
            error,
            pending: false,
            revision: 0,
            help: false,
            highlighter_dark: None,
            help_scroll: ScrollHandle::new(),
            motion: ui::MotionScope::new(),
        }
    }
    #[cfg(feature = "screenshots")]
    pub(super) fn preview(&mut self, error: bool, help: bool, window: &mut Window, cx: &mut Context<Self>) {
        if error {
            let mut value: Value = serde_json::from_str(&self.drafts[self.selected]).unwrap();
            value["language_map"].as_object_mut().unwrap().remove("other");
            self.source.update(cx, |s, cx| s.set_value(serde_json::to_string_pretty(&value).unwrap(), window, cx));
            self.changed(cx);
        }
        self.help = help;
        cx.notify();
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
        self.pending = true;
        self.error = None;
        self.source.update(cx, |source, _| {
            if let Some(d) = source.diagnostics_mut() {
                d.clear();
            }
        });
        self.revision += 1;
        let revision = self.revision;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(200)).await;
            let _ = this.update(cx, |this, cx| {
                if this.revision == revision {
                    this.error = check(&this.drafts[this.selected], this.st.cfg.get().general.lang()).err();
                    let error = this.error.clone();
                    this.source.update(cx, |source, cx| {
                        if let Some(diagnostics) = source.diagnostics_mut() {
                            diagnostics.clear();
                            if let Some(error) = error {
                                use gpui_kit::base::input::{Diagnostic, DiagnosticSeverity, Position};
                                let line = error_row(&this.drafts[this.selected]).saturating_sub(1) as u32;
                                diagnostics.push(
                                    Diagnostic::new(Position::new(line, 0)..Position::new(line + 1, 0), error)
                                        .with_severity(DiagnosticSeverity::Error),
                                );
                            }
                        }
                        cx.notify();
                    });
                    this.pending = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    fn switch(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.drafts[self.selected] = self.source.read(cx).value().to_string();
        self.selected = i;
        self.source.update(cx, |s, cx| s.set_value(self.drafts[i].clone(), window, cx));
        self.changed(cx);
    }
    fn mapping(&mut self, regional: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(mut value) = serde_json::from_str::<Value>(self.source.read(cx).value().as_ref())
            && let Some(map) = value["language_map"].as_object_mut()
        {
            for (k, v) in if regional {
                [("ja", "ja-JP"), ("en", "en-US"), ("zh", "zh-CN"), ("other", "en-US")]
            } else {
                [("ja", "ja"), ("en", "en"), ("zh", "zh"), ("other", "en")]
            } {
                map.insert(k.into(), v.into());
            }
            self.source.update(cx, |s, cx| s.set_value(serde_json::to_string_pretty(&value).unwrap(), window, cx));
            self.changed(cx);
        }
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        if self.pending {
            return;
        }
        let value = match check(self.source.read(cx).value().as_ref(), self.st.cfg.get().general.lang()) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                cx.notify();
                return;
            }
        };
        let selected = self.selected;
        if let Err(e) = self.st.cfg.update(|cfg| {
            let profiles = cfg.asr.cloud_asr.get_or_insert_with(AsrProfiles::default);
            while profiles.profiles.len() < 3 {
                profiles.profiles.push(AsrProfiles::default().profiles[profiles.profiles.len()].clone());
            }
            profiles.profiles[selected] = value;
            profiles.selected = selected;
        }) {
            self.error = Some(e.to_string());
            cx.notify();
            return;
        }
        cx.emit(Closed(true));
    }
    fn help_body(&self, p: &crate::theme::Palette) -> Div {
        let mut steps = div().w_full().flex().items_center().gap(px(12.));
        for (i, label) in ["テンプレート", "編集", "自動チェック", "保存"].iter().enumerate() {
            if i > 0 {
                steps = steps.child(text("→", 13., FontWeight::NORMAL, p.muted));
            }
            steps = steps.child(
                div()
                    .flex_1()
                    .h(px(56.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .rounded(px(12.))
                    .bg(p.sub)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .size(px(32.))
                            .flex_none()
                            .rounded_full()
                            .border_1()
                            .border_color(p.accent)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(text((i + 1).to_string(), 14., FontWeight::MEDIUM, p.accent_text)),
                    )
                    .child(text(self.translate(label), 13., FontWeight::MEDIUM, p.text)),
            );
        }
        let card = |title: &str, body: Div| {
            ui::card(p)
                .flex_1()
                .p(px(24.))
                .gap(px(16.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .child(div().w(px(4.)).h(px(16.)).rounded(px(2.)).bg(p.accent))
                        .child(text(self.translate(title), 17., FontWeight::BOLD, p.text)),
                )
                .child(body)
        };
        let rows = |items: &[(&str, &str)]| {
            let mut d = div().flex().flex_col().gap(px(10.));
            for (k, v) in items {
                d = d.child(
                    div()
                        .flex()
                        .gap(px(16.))
                        .child(div().w(px(148.)).child(text(self.translate(k), 12., FontWeight::NORMAL, p.muted)))
                        .child(text(self.translate(v), 13., FontWeight::NORMAL, p.text)),
                );
            }
            d
        };
        div().flex().flex_col().gap(px(24.)).child(steps)
            .child(div().flex().gap(px(24.)).child(card("設定の構成",rows(&[("version","形式のバージョン"),("connection","接続先とモデル"),("language_map","言語コードの変換"),("request","送信するデータ"),("response","認識結果の取得先")]))).child(card("言語コードの変換",div().flex().flex_col().gap(px(16.)).child(text(self.st.cfg.get().general.lang().tr(Tr::AsrEditorInputCodes),12.,FontWeight::MEDIUM,p.text)).child(text(self.st.cfg.get().general.lang().tr(Tr::AsrEditorInputAuto),12.,FontWeight::NORMAL,p.muted)).child(text(self.translate("接続先が JP / EN / ZH を使う場合の例"),12.,FontWeight::NORMAL,p.muted)).child(div().p(px(12.)).rounded(px(12.)).bg(p.sub).child(text("{\n  \"ja\": \"JP\", \"en\": \"EN\",\n  \"zh\": \"ZH\", \"auto\": null,\n  \"other\": \"EN\"\n}",12.,FontWeight::NORMAL,p.text).font_family("Menlo"))).child(text(self.translate("完全なタグ → 言語コード → other"),12.,FontWeight::NORMAL,p.muted)).child(text(self.translate("auto が null なら、言語を指定しません。"),12.,FontWeight::NORMAL,p.muted)))))
            .child(div().flex().gap(px(24.)).child(card("使用できる変数",rows(&[("{{model}}","モデル ID"),("{{language}}","変換後の言語コード"),("{{api_key}}","API キー"),("{{audio.base64}}","Base64 の音声"),("{{audio.data_uri}}","Data URI の音声"),("{{audio.file}}","WAV ファイル")]))).child(card("送信内容と認識結果",div().flex().flex_col().gap(px(12.)).child(text("request.path",12.,FontWeight::MEDIUM,p.muted)).child(text(self.translate("ベース URL の末尾に追加します。"),13.,FontWeight::NORMAL,p.text)).child(text("JSON: body  /  multipart: fields",12.,FontWeight::NORMAL,p.muted)).child(text("response.text_pointer",12.,FontWeight::MEDIUM,p.muted)).child(text("/choices/0/message/content",12.,FontWeight::NORMAL,p.text)).child(text(self.translate("認識した文章の場所を指定します。"),13.,FontWeight::NORMAL,p.muted)))))
    }
}
fn error_row(source: &str) -> usize {
    match serde_json::from_str::<Value>(source) {
        Err(e) => e.line(),
        Ok(value) => {
            let message = validate(&value).err().map(|e| e.message).unwrap_or_default();
            let path = message.split(':').next().unwrap_or("");
            let key = path.trim_start_matches('/').split(['/', '.']).next_back().unwrap_or(path);
            source
                .lines()
                .position(|l| l.contains(&format!("\"{key}\"")))
                .or_else(|| source.lines().position(|l| l.contains("\"language_map\"")))
                .unwrap_or(0)
                + 1
        }
    }
}
fn check(source: &str, lang: dito_core::i18n::Lang) -> Result<Value, String> {
    let value = serde_json::from_str::<Value>(source).map_err(|e| {
        lang.tr(Tr::FmtAsrSyntaxError)
            .replace("{line}", &e.line().to_string())
            .replace("{column}", &e.column().to_string())
            .replace("{detail}", &e.to_string())
    })?;
    validate(&value).map_err(|e| {
        lang.tr(Tr::FmtAsrFieldError).replace("{line}", &error_row(source).to_string()).replace("{detail}", &e.message)
    })?;
    Ok(value)
}

impl Render for AsrEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || {
            let c = Ctx::new(&self.st, cx);
            let p = c.p;
            if self.highlighter_dark != Some(p.dark) {
                self.highlighter_dark = Some(p.dark);
                self.source.update(cx, |source, cx| {
                    source.set_highlighter_factory(
                        Rc::new(move |_| Some(Box::new(super::asr_highlight::JsonHighlight::new(p.dark)))),
                        cx,
                    )
                });
            }

            self.source.update(cx, |source, _| {
                source.set_scrollbar_renderer(Some(Rc::new(move |geometry, _, _| {
                    use gpui_kit::component::scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode};
                    let offset = -f32::from(geometry.offset().y);
                    let remaining =
                        f32::from(geometry.content_size().height - geometry.viewport_bounds().size.height) - offset;
                    div()
                        .absolute()
                        .inset_0()
                        .when(offset > 1., |d| {
                            d.child(div().absolute().top_0().left_0().right(px(16.)).h(px(20.)).bg(linear_gradient(
                                180.,
                                linear_color_stop(p.sub, 0.),
                                linear_color_stop(ui::hex_alpha(p.sub, 0.), 1.),
                            )))
                        })
                        .when(remaining > 1., |d| {
                            d.child(div().absolute().bottom_0().left_0().right(px(16.)).h(px(20.)).bg(linear_gradient(
                                0.,
                                linear_color_stop(p.sub, 0.),
                                linear_color_stop(ui::hex_alpha(p.sub, 0.), 1.),
                            )))
                        })
                        .child(Scrollbar::new(&geometry).axis(geometry.axis()).mode(ScrollbarMode::Always).styles(
                            |s| {
                                s.track(|s| s.bg(p.border))
                                    .thumb(|s| s.bg(p.muted).width(px(4.)).inset(px(6.)).radius(px(2.)))
                            },
                        ))
                        .into_any_element()
                })))
            });

            let height =
                (f32::from(window.viewport_size().height) - 48.).min(if self.help { 836. } else { 900. }).max(400.);
            let width = (f32::from(window.viewport_size().width) - 48.).clamp(320., 900.);
            // A short window trims its top and bottom padding to 14, so the cross sits 24 from the top and 38 from
            // the right edge, a golden-ratio pair (design 20.06). A taller window keeps 28 and 38 on both sides.
            let pad_v = if height < 700. { 14. } else { 28. };
            let close = cbk(cx, |this: &mut Self, _, cx| {
                if this.help {
                    this.help = false;
                    cx.notify();
                } else {
                    cx.emit(Closed(false));
                }
            });
            let header = div()
                .w_full()
                .h(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    text(
                        self.translate(if self.help { "設定の書き方" } else { "接続設定" }),
                        26.,
                        FontWeight::BOLD,
                        p.text,
                    )
                    .line_height(px(32.)),
                )
                .child(ui::close_button("asr-close", &p, close));
            let mut body = div().w_full().flex_1().min_h(px(0.)).flex().flex_col().gap(px(14.));
            let footer = if self.help {
                let scroll = self.help_scroll.clone();
                let offset = -f32::from(scroll.offset().y);
                let max = f32::from(scroll.max_offset().y);
                body = body.child(
                    div()
                        .relative()
                        .flex_1()
                        .min_h(px(0.))
                        .child(
                            div()
                                .id("asr-help-scroll")
                                .h_full()
                                .overflow_y_scroll()
                                .track_scroll(&scroll)
                                .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                                .child(self.help_body(&p)),
                        )
                        .when(offset > 1., |d| {
                            d.child(div().absolute().top_0().left_0().right_0().h(px(24.)).bg(linear_gradient(
                                180.,
                                linear_color_stop(p.bg, 0.),
                                linear_color_stop(ui::hex_alpha(p.bg, 0.), 1.),
                            )))
                        })
                        .when(max - offset > 1., |d| {
                            d.child(div().absolute().bottom_0().left_0().right_0().h(px(24.)).bg(linear_gradient(
                                0.,
                                linear_color_stop(p.bg, 0.),
                                linear_color_stop(ui::hex_alpha(p.bg, 0.), 1.),
                            )))
                        }),
                );
                div().flex().justify_end().child(ui::button(
                    "asr-copy-help",
                    &p,
                    &self.translate("AI 用の説明をコピー"),
                    BtnKind::Primary,
                    None,
                    cbk(cx, |this: &mut Self, _, cx| {
                        let language = this.st.cfg.get().general.lang();
                        cx.write_to_clipboard(ClipboardItem::new_string(
                            dito_core::provider::configured_asr::configuration_spec(language, this.selected),
                        ));
                    }),
                ))
            } else {
                let mut templates = div().w_full().flex().gap(px(12.));
                for (i, name) in AsrProfiles::NAMES.iter().enumerate() {
                    templates = templates.child(
                        div()
                            .id(("json-template", i))
                            .flex_1()
                            .h(px(44.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(if self.selected == i { p.accent } else { p.border_strong })
                            .bg(if self.selected == i { p.accent_soft } else { p.bg })
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| this.switch(i, window, cx)))
                            .child(text(*name, 14., FontWeight::MEDIUM, p.text)),
                    );
                }
                let toolbar = div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(text(self.translate("言語コード"), 13., FontWeight::NORMAL, p.muted))
                    .child(ui::button(
                        "map-iso",
                        &p,
                        "ja / en / zh",
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |this: &mut Self, w, cx| this.mapping(false, w, cx)),
                    ))
                    .child(ui::button(
                        "map-locale",
                        &p,
                        "ja-JP / en-US / zh-CN",
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |this: &mut Self, w, cx| this.mapping(true, w, cx)),
                    ))
                    .child(div().flex_1())
                    .child(ui::button(
                        "copy-config",
                        &p,
                        &self.translate("設定をコピー"),
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |this: &mut Self, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(this.source.read(cx).value().to_string()))
                        }),
                    ));
                let source = self.source.read(cx).value().to_string();
                let code = div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(p.border_strong)
                    .bg(p.sub)
                    .overflow_hidden()
                    .child(
                        div().flex_1().min_h(px(0.)).child(
                            Editor::new(&self.source)
                                .appearance(false)
                                .bordered(false)
                                .h_full()
                                .font_family("Menlo")
                                .text_size(px(12.))
                                .aria_label("接続設定"),
                        ),
                    )
                    .child(
                        div()
                            .h(px(26.))
                            .flex_none()
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .border_t_1()
                            .border_color(p.border)
                            .child(text(
                                c.t(Tr::FmtAsrLineCount).replace("{count}", &source.lines().count().max(1).to_string()),
                                12.,
                                FontWeight::NORMAL,
                                p.muted,
                            )),
                    );
                let status = if self.pending {
                    self.translate("確認しています…").to_string()
                } else {
                    self.error.clone().unwrap_or_else(|| self.translate("エラーはありません").to_string())
                };
                body = body.child(templates).child(toolbar).child(code).child(
                    ui::card(&p)
                        .flex_none()
                        .p(px(16.))
                        .gap(px(4.))
                        .child(text(self.translate("入力内容の確認"), 13., FontWeight::MEDIUM, p.text))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(div().size(px(7.)).rounded_full().bg(if self.error.is_some() {
                                    p.danger
                                } else if self.pending {
                                    p.muted
                                } else {
                                    p.success
                                }))
                                .child(text(status, 13., FontWeight::NORMAL, p.text)),
                        ),
                );
                let can_save = !self.pending && self.error.is_none();
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        ui::button(
                            "asr-help",
                            &p,
                            &self.translate("ヘルプ"),
                            BtnKind::Secondary,
                            None,
                            cbk(cx, |this: &mut Self, _, cx| {
                                this.help = true;
                                cx.notify();
                            }),
                        )
                        .bg(p.accent_soft)
                        .border_color(p.accent),
                    )
                    .child(div().flex_1())
                    .child(ui::button(
                        "asr-cancel",
                        &p,
                        &self.translate("キャンセル"),
                        BtnKind::Secondary,
                        None,
                        cbk(cx, |_: &mut Self, _, cx| cx.emit(Closed(false))),
                    ))
                    .child(
                        ui::button(
                            "asr-save",
                            &p,
                            &self.translate("保存"),
                            BtnKind::Primary,
                            None,
                            cbk(cx, move |this: &mut Self, _, cx| {
                                if can_save {
                                    this.save(cx);
                                }
                            }),
                        )
                        .opacity(if can_save { 1. } else { 0.4 }),
                    )
            };
            div()
                .id("asr-modal-blocker")
                .absolute()
                .inset_0()
                .occlude()
                .bg(hsla(0., 0., 0., 0.3))
                .flex()
                .items_center()
                .justify_center()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        if this.help {
                            this.help = false;
                            cx.notify();
                        } else {
                            cx.emit(Closed(false));
                        }
                        cx.stop_propagation();
                    }
                }))
                .child(
                    div()
                        .w(px(width))
                        .h(px(height))
                        .px(px(28.))
                        .py(px(pad_v))
                        .rounded(px(20.))
                        .bg(p.bg)
                        .border_1()
                        .border_color(p.border_strong)
                        .flex()
                        .flex_col()
                        .gap(px(14.))
                        .child(header)
                        .child(body)
                        .child(div().flex_none().child(footer)),
                )
        })
    }
}
