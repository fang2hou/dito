use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use dito_core::config::{
    Appearance, DisplayLanguage, HudPosition, InsertMethod, MusicMuteMode, PreferredLanguage, RecordMode,
};
use dito_core::i18n::{Lang, Tr};
use dito_platform::hotkey::Shortcut;
use dito_platform::{Permission, Status, permissions, system};
use gpui_kit::*;

use super::{Ctx, watch_config};
use crate::assets::Ic;
use crate::hotkeys::CaptureState;
use crate::state::AppState;
use crate::ui::{self, BtnKind, Cb, cbk, text};

const MIC_POLL: Duration = Duration::from_secs(1);

pub struct GeneralPage {
    st: Arc<AppState>,
    open: Option<&'static str>,
    /// Filled when the microphone list is opened; enumerating devices at launch touches the audio
    /// system (and any nearby iPhone) before the user has asked for a microphone.
    devices: Vec<dito_audio::InputDevice>,
    /// Bumped each time the microphone list opens; ends the previous refresh loop.
    mic_poll: u64,
    capturing: bool,
    accessibility_ok: bool,
    hud_preview: Option<Entity<crate::hud_style_preview::StylePreview>>,
    hud_height: Entity<super::hud_slider::HudHeightSlider>,
    /// The page's own motion scope: its controls' transient state, reset on navigation.
    motion: ui::MotionScope,
}

impl GeneralPage {
    pub fn new(st: Arc<AppState>, cx: &mut Context<Self>) -> Self {
        watch_config(&st, cx);

        let mut rx = st.hotkeys.capture.subscribe();
        cx.spawn(async move |this, cx| {
            while rx.changed().await.is_ok() {
                let waiting = *rx.borrow_and_update() == CaptureState::Waiting;
                if this
                    .update(cx, |t, cx| {
                        t.capturing = waiting;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let mut unavailable = st.hotkeys.unavailable.subscribe();
        cx.spawn(async move |this, cx| {
            while unavailable.changed().await.is_ok() {
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        // Permissions change outside the app; poll lightly while the window exists.
        cx.spawn(async move |this, cx| {
            loop {
                let ok = permissions::status(Permission::Accessibility) == Status::Granted;
                if this
                    .update(cx, |t, cx| {
                        if t.accessibility_ok != ok {
                            t.accessibility_ok = ok;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor().timer(Duration::from_secs(2)).await;
            }
        })
        .detach();

        let mut hud = st.dictation.hud();
        cx.spawn(async move |this, cx| {
            while hud.changed().await.is_ok() {
                if *hud.borrow_and_update() != dito_core::controller::HudState::Hidden
                    && this.update(cx, |page, cx| page.cancel_hud_preview(cx)).is_err()
                {
                    break;
                }
            }
        })
        .detach();

        let hud_height = cx.new(|cx| super::hud_slider::HudHeightSlider::new(st.clone(), cx));
        Self {
            motion: ui::MotionScope::new(),
            hud_preview: None,
            hud_height,
            devices: Vec::new(),
            mic_poll: 0,
            st,
            open: None,
            capturing: false,
            accessibility_ok: permissions::status(Permission::Accessibility) == Status::Granted,
        }
    }

    pub(super) fn set_hud_appearance(&mut self, appearance: Appearance, cx: &mut Context<Self>) {
        if self.st.cfg.get().general.hud_appearance == appearance {
            return;
        }
        if let Err(error) = self.st.cfg.update(|cfg| cfg.general.hud_appearance = appearance) {
            tracing::warn!("could not save HUD appearance: {error}");
            return;
        }
        self.hud_preview = if *self.st.dictation.hud().borrow() == dito_core::controller::HudState::Hidden {
            let language = self.st.cfg.get().general.lang();
            Some(cx.new(|cx| crate::hud_style_preview::StylePreview::new(appearance, language, cx)))
        } else {
            None
        };
        cx.notify();
    }

    pub(super) fn cancel_hud_preview(&mut self, cx: &mut Context<Self>) {
        if self.hud_preview.take().is_some() {
            cx.notify();
        }
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn hud_preview(&self, cx: &App) -> Option<Entity<crate::hud_style_preview::StylePreview>> {
        self.hud_preview.clone().filter(|preview| preview.read(cx).active())
    }

    #[cfg(feature = "screenshots")]
    pub(super) fn hud_height_bounds(&self, cx: &App) -> Bounds<Pixels> {
        self.hud_height.read(cx).bounds(cx)
    }

    #[cfg(feature = "screenshots")]
    pub fn open_select(&mut self, id: &'static str, cx: &mut Context<Self>) {
        self.open = Some(id);
        if id == "mic" && (!self.st.opts.mock || self.st.opts.real_mic) {
            self.devices = dito_audio::list_input_devices();
        }
        cx.notify();
    }

    fn toggle(&self, cx: &mut Context<Self>, id: &'static str) -> Cb {
        cbk(cx, move |this: &mut Self, _, cx| {
            this.open = if this.open == Some(id) { None } else { Some(id) };
            if this.open == Some("mic") && (!this.st.opts.mock || this.st.opts.real_mic) {
                this.devices = dito_audio::list_input_devices();
                this.mic_poll += 1;
                let generation = this.mic_poll;
                // Headsets connect a few seconds after they are put on: keep the list current while it is shown.
                cx.spawn(async move |this, cx| {
                    loop {
                        cx.background_executor().timer(MIC_POLL).await;
                        let devices = cx.background_executor().spawn(async { dito_audio::list_input_devices() }).await;
                        let keep = this.update(cx, |t, cx| {
                            if t.open != Some("mic") || t.mic_poll != generation {
                                return false;
                            }
                            if t.devices != devices {
                                t.devices = devices;
                                cx.notify();
                            }
                            true
                        });
                        if !keep.unwrap_or(false) {
                            break;
                        }
                    }
                })
                .detach();
            }
            cx.notify();
        })
    }

    fn banner(&self, c: &Ctx, cx: &mut Context<Self>) -> Option<Div> {
        if self.accessibility_ok && self.st.hotkeys.available() {
            return None;
        }
        let p = &c.p;
        let (bg, border, fg) = if p.dark {
            (ui::hex_alpha(p.danger, 0.12), ui::hex_alpha(p.danger, 0.5), p.text)
        } else {
            (p.danger_soft, p.danger, p.danger_text)
        };
        Some(
            div()
                .w_full()
                .pl(px(20.))
                .pr(px(12.))
                .py(px(10.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(14.))
                .rounded(px(16.))
                .bg(bg)
                .border_1()
                .border_color(border)
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .items_center()
                        .gap(px(12.))
                        .child(ui::icon(Ic::Alert, 18., if p.dark { p.danger } else { p.danger_text }))
                        .child(text(c.t(Tr::StatusNeedAccessibility), 13., FontWeight::MEDIUM, fg)),
                )
                .child(ui::button(
                    "allow",
                    p,
                    &c.t(Tr::GeneralAllow),
                    if p.dark { BtnKind::Secondary } else { BtnKind::Primary },
                    None,
                    cbk(cx, |_, _, _| permissions::request(Permission::Accessibility)),
                )),
        )
    }

    fn shortcut_field(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let p = &c.p;
        let cfg = self.st.cfg.get();
        let field = if self.capturing {
            div()
                .id("shortcut-recording")
                .w(px(190.))
                .h(px(ui::CONTROL_H))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.))
                .rounded(px(12.))
                .bg(p.accent_soft)
                .border_2()
                .border_color(p.accent)
                .child(div().size(px(8.)).rounded_full().bg(p.danger))
                .child(
                    text(c.t(Tr::GeneralWaitingForInput), 13., FontWeight::MEDIUM, p.accent_text)
                        .line_height(relative(1.)),
                )
                .into_any_element()
        } else {
            let label = Shortcut::parse(&cfg.general.shortcut)
                .map(|s| dito_platform::keymap::label(&s, c.lang))
                .unwrap_or_else(|| cfg.general.shortcut.clone());
            div()
                .id("shortcut-field")
                .w(px(190.))
                .h(px(ui::CONTROL_H))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.))
                .bg(p.input_bg)
                .border_1()
                .border_color(p.border_strong)
                .cursor_pointer()
                .on_click(cx.listener(|this, _, _, _| this.st.hotkeys.begin_capture()))
                .child(text(label, 15., FontWeight::BOLD, p.text).line_height(relative(1.)))
                .into_any_element()
        };
        let hint = c.t(Tr::GeneralPressEscToCancel);
        // One control, two states: the field keeps its width while capturing, and the same
        // pop-in bubble the hover tips use floats above it. Static positioned box (an animated
        // one re-lays out against the wrong ancestor); the symmetric box (non-zero insets)
        // centres the bubble without measuring the text; the top offset clears the field.
        // Deferred, so following rows' separators cannot paint over the bubble.
        div().relative().child(field).child(deferred(
            div()
                .absolute()
                .top(px(-(ui::TIP_BUBBLE_H + 6.)))
                .left(px(0.5))
                .right(px(0.5))
                .flex()
                .justify_center()
                .child(ui::tip_pop(p, ElementId::from("shortcut-hint"), hint, self.capturing)),
        ))
    }
}

impl GeneralPage {
    /// Reset the page's transient motion state; its controls mount statically on the next entry.
    pub(super) fn reset_motion(&mut self) {
        self.motion.reset();
    }

    /// The page's scope, for the screenshot harness.
    #[cfg(feature = "screenshots")]
    pub(super) fn motion_scope(&self) -> ui::ScopeHandle {
        self.motion.handle()
    }
}

impl Render for GeneralPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The page draws under its own scope: after a page switch its controls mount statically,
        // never sliding to values remembered from another page or an earlier visit.
        ui::with_motion_scope(self.motion.handle(), || self.view(cx))
    }
}

impl GeneralPage {
    fn view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Ctx::new(&self.st, cx);
        let p = c.p;
        let cfg = self.st.cfg.get();
        let g = &cfg.general;
        let st = self.st.clone();

        let update = |st: &Arc<AppState>, f: Box<dyn FnOnce(&mut dito_core::config::Config)>| {
            if let Err(e) = st.cfg.update(f) {
                tracing::warn!("could not save settings: {e}");
            }
        };

        // 入力
        let st1 = st.clone();
        let mode = ui::seg(
            "mode",
            &p,
            &[c.t(Tr::GeneralToggle), c.t(Tr::GeneralHold)],
            if g.record_mode == RecordMode::Toggle { 0 } else { 1 },
            Rc::new(move |i, _, _| {
                update(
                    &st1,
                    Box::new(move |c| {
                        c.general.record_mode = if i == 0 { RecordMode::Toggle } else { RecordMode::Hold }
                    }),
                )
            }),
        );
        let st2 = st.clone();
        let insert = ui::seg(
            "insert",
            &p,
            &[c.t(Tr::GeneralKeyInput), c.t(Tr::GeneralPaste)],
            if g.insert_method == InsertMethod::KeyInput { 0 } else { 1 },
            Rc::new(move |i, _, _| {
                update(
                    &st2,
                    Box::new(move |c| {
                        c.general.insert_method = if i == 0 { InsertMethod::KeyInput } else { InsertMethod::Paste }
                    }),
                )
            }),
        );
        let st9 = st.clone();
        let music = ui::seg(
            "music-mute",
            &p,
            &[c.t(Tr::GeneralOff), c.t(Tr::GeneralAuto)],
            if g.music_mute == MusicMuteMode::Auto { 1 } else { 0 },
            Rc::new(move |i, _, _| {
                update(
                    &st9,
                    Box::new(move |c| {
                        c.general.music_mute = if i == 1 { MusicMuteMode::Auto } else { MusicMuteMode::Off }
                    }),
                )
            }),
        );

        // 音声
        // The saved choice stays listed (and selected) while the list is unloaded or the device is unplugged.
        let mut devices: Vec<String> = self.devices.iter().map(|d| d.name.clone()).collect();
        if !g.microphone.is_empty() && !devices.contains(&g.microphone) {
            devices.push(g.microphone.clone());
        }
        let mut mic_opts = vec![ui::SelectItem { label: c.t(Tr::GeneralAutomaticRecommended), badge: None }];
        mic_opts.extend(devices.iter().map(|name| {
            let badge = self.devices.iter().find(|d| d.name == *name).map(|d| {
                let on = d.auto(&g.microphone_auto);
                let (stb, device) = (st.clone(), name.clone());
                ui::SelectBadge {
                    active: !on,
                    tip: if on { c.t(Tr::GeneralIncludedInAutoPicking) } else { c.t(Tr::GeneralManualOnlyClickToAuto) },
                    on_click: Rc::new(move |_, _| {
                        let (stc, device) = (stb.clone(), device.clone());
                        update(
                            &stb,
                            Box::new(move |cfg| {
                                cfg.general.microphone_auto.insert(device, !on);
                                stc.recorder_auto(&cfg.general.microphone_auto);
                            }),
                        )
                    }),
                }
            });
            ui::SelectItem { label: SharedString::from(name.clone()), badge }
        }));
        let mic_sel = devices.iter().position(|d| *d == g.microphone).map(|i| i + 1).unwrap_or(0);
        let st3 = st.clone();
        let mic = ui::select_items(
            "mic",
            &p,
            &mic_opts,
            mic_sel,
            self.open == Some("mic"),
            Some(240.),
            self.toggle(cx, "mic"),
            Rc::new(move |i, _, _| {
                let name = if i == 0 { String::new() } else { devices.get(i - 1).cloned().unwrap_or_default() };
                st3.recorder_device(&name);
                update(&st3, Box::new(move |c| c.general.microphone = name))
            }),
        );
        let langs = [PreferredLanguage::Auto, PreferredLanguage::Ja, PreferredLanguage::En, PreferredLanguage::Zh];
        let st4 = st.clone();
        let pref = ui::select(
            "pref-lang",
            &p,
            &[c.t(Tr::GeneralAuto), "日本語".into(), "English".into(), "中文".into()],
            langs.iter().position(|l| *l == g.preferred_language).unwrap_or(0),
            self.open == Some("pref-lang"),
            Some(240.),
            self.toggle(cx, "pref-lang"),
            Rc::new(move |i, _, _| update(&st4, Box::new(move |c| c.general.preferred_language = langs[i]))),
        );
        let st5 = st.clone();
        let login = ui::switch(
            "login",
            &p,
            g.launch_at_login,
            Rc::new(move |on, _, _| {
                if !st5.opts.mock
                    && let Err(e) = system::set_launch_at_login(on)
                {
                    tracing::warn!("login item: {e}");
                }
                update(&st5, Box::new(move |c| c.general.launch_at_login = on))
            }),
        );
        let looks = [Appearance::System, Appearance::Light, Appearance::Dark];
        let st7 = st.clone();
        let appearance = ui::appearance_seg(
            "appearance",
            &p,
            &[c.t(Tr::GeneralSystem), c.t(Tr::GeneralLight), c.t(Tr::GeneralDark)],
            looks.iter().position(|a| *a == g.appearance).unwrap_or(0),
            Rc::new(move |i, _, _| update(&st7, Box::new(move |c| c.general.appearance = looks[i]))),
        );
        let weak = cx.entity().downgrade();
        let hud_switch = ui::appearance_seg(
            "hud-appearance",
            &p,
            &[c.t(Tr::GeneralSystem), c.t(Tr::GeneralLight), c.t(Tr::GeneralDark)],
            looks.iter().position(|a| *a == g.hud_appearance).unwrap_or(0),
            Rc::new(move |i, _, app| {
                let _ = weak.update(app, |page, cx| page.set_hud_appearance(looks[i], cx));
            }),
        );
        let hud_appearance = div()
            .flex()
            .items_center()
            .gap(px(24.))
            .child(
                div()
                    .w(px(crate::hud::CAPSULE_W * crate::hud::HUD_SCALE))
                    .h(px(crate::hud::CAPSULE_H * crate::hud::HUD_SCALE))
                    .flex_none()
                    .children(self.hud_preview.clone()),
            )
            .child(hud_switch);
        let position_store = st.clone();
        let hud_position = ui::seg(
            "hud-position",
            &p,
            &[c.t(Tr::GeneralHudCentered), c.t(Tr::GeneralHudPointer)],
            usize::from(g.hud_position == HudPosition::Pointer),
            Rc::new(move |i, _, _| {
                update(
                    &position_store,
                    Box::new(move |c| {
                        c.general.hud_position = if i == 0 { HudPosition::Centered } else { HudPosition::Pointer };
                    }),
                )
            }),
        );
        let hud_height = self.hud_height.clone();
        let st8 = st.clone();
        let ui_lang = ui::select(
            "ui-lang",
            &p,
            &[
                c.t(Tr::GeneralFollowSystem),
                SharedString::from(Lang::Ja.native_name()),
                SharedString::from(Lang::En.native_name()),
                SharedString::from(Lang::Zh.native_name()),
            ],
            DisplayLanguage::ALL.iter().position(|l| *l == g.display_language).unwrap_or(0),
            self.open == Some("ui-lang"),
            Some(240.),
            self.toggle(cx, "ui-lang"),
            Rc::new(move |i, _, _| {
                update(&st8, Box::new(move |c| c.general.display_language = DisplayLanguage::ALL[i]))
            }),
        );

        // Testing aid: forget the first-run state, close settings and walk the wizard again.
        let restart_wizard = ui::button(
            "restart-wizard",
            &p,
            &c.t(Tr::OnboardingRunAgain),
            BtnKind::Secondary,
            Some(Ic::Reset),
            cbk(cx, |this: &mut Self, window, cx| {
                if let Err(e) = this.st.cfg.update(|c| c.onboarded = false) {
                    tracing::warn!("could not reset first-run state: {e}");
                }
                let st = this.st.clone();
                window.remove_window();
                cx.defer(move |cx| crate::onboarding::restart(st, cx));
            }),
        );

        let banner = self.banner(&c, cx);
        let shortcut = self.shortcut_field(&c, cx);

        // The master switch for polish lives here — not in Models, not in Prompts.
        let polish_st = self.st.clone();
        let polish_toggle = ui::switch(
            "polish-enabled",
            &p,
            cfg.polish.enabled,
            Rc::new(move |on, _, _| {
                let _ = polish_st.cfg.update(|c| c.polish.enabled = on);
            }),
        );

        super::page(
            p,
            "general",
            c.t(Tr::NavGeneral),
            None,
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(28.))
                .children(banner)
                .child(ui::group(
                    &p,
                    c.t(Tr::GeneralInput),
                    vec![
                        ui::row(&p, c.t(Tr::GeneralRecord), shortcut, false),
                        ui::row(&p, c.t(Tr::GeneralMode), mode, false),
                        ui::row(&p, c.t(Tr::GeneralInsertMethod), insert, false),
                        ui::row(&p, c.t(Tr::ModelsPolish), polish_toggle, false),
                        ui::row(&p, c.t(Tr::GeneralMuteMusic), music, true),
                    ],
                ))
                .child(ui::group(
                    &p,
                    c.t(Tr::GeneralAudio),
                    vec![
                        ui::row(&p, c.t(Tr::GeneralMicrophone), mic, false),
                        ui::row(&p, c.t(Tr::GeneralPreferredLanguage), pref, true),
                    ],
                ))
                .child(ui::group(&p, c.t(Tr::GeneralSystem), {
                    let mut rows = vec![
                        ui::row(&p, c.t(Tr::GeneralLaunchAtLogin), login, false),
                        ui::row(&p, c.t(Tr::GeneralAppearance), appearance, false),
                        ui::row(&p, c.t(Tr::GeneralHudAppearance), hud_appearance, false),
                        ui::row(&p, c.t(Tr::GeneralHudPosition), hud_position, false),
                    ];
                    let height_title = c.t(Tr::GeneralHudHeight);
                    let target = if g.hud_position == HudPosition::Centered { 1. } else { 0. };
                    let duration = if system::reduce_motion() { 1 } else { 180 };
                    rows.push(div().w_full().child(ui::tweened(
                        ElementId::from("hud-height-row"),
                        target,
                        duration,
                        move |v| {
                            let row = div().w_full().h(px(64. * v)).overflow_hidden().opacity(v);
                            if v > 0.001 {
                                row.child(ui::row(&p, height_title.clone(), hud_height.clone(), false))
                            } else {
                                row
                            }
                        },
                    )));
                    rows.push(ui::row(&p, c.t(Tr::GeneralDisplayLanguage), ui_lang, true));
                    rows
                }))
                .child(ui::group(
                    &p,
                    c.t(Tr::OnboardingInitialSetup),
                    vec![ui::row(&p, c.t(Tr::OnboardingSetupWizard), restart_wizard, true)],
                )),
        )
    }
}
