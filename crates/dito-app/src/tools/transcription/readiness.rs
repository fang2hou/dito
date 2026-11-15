//! The pre-flight gate: while a permission or the speech detector is missing, the page shows what a session needs
//! and the one action that gets it.

use dito_core::i18n::Tr;
use dito_platform::{Permission, Status, permissions};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::TranscriptionPage;
use crate::assets::Ic;
use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, BtnKind, cbk, text};

/// `design:tools/transcripts` measurements.
const GATE_W: f32 = 620.;
const ROW_H: f32 = 92.;

pub(super) fn view(page: &TranscriptionPage, c: &Ctx, cx: &mut Context<TranscriptionPage>) -> Div {
    let p = &c.p;
    let lead = text(c.t(Tr::TranscriptionReadinessLead), 14., FontWeight::NORMAL, p.muted);
    let header = div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .child(text(c.t(Tr::NavTranscription), 24., FontWeight::BOLD, p.text))
        .child(lead);

    let rows = vec![
        row(p, page.mic_ready(), Ic::Mic, c.t(Tr::GeneralMicrophone), c.t(Tr::TranscriptionReadinessMic), {
            match page.perms.microphone {
                Status::Granted => pill(p, c.t(Tr::StatusAllowed)),
                Status::NotDetermined => allow_button("readiness-allow-mic", c, cx, Permission::Microphone),
                Status::Denied => settings_button("readiness-settings-mic", c, cx, Permission::Microphone),
            }
        }),
        row(
            p,
            page.system_audio_ready(),
            Ic::Headphones,
            c.t(Tr::StatusSystemAudio),
            c.t(Tr::TranscriptionReadinessSystemAudio),
            {
                match page.perms.system_audio {
                    Status::Granted => pill(p, c.t(Tr::StatusAllowed)),
                    Status::NotDetermined => {
                        allow_button("readiness-allow-system-audio", c, cx, Permission::SystemAudio)
                    }
                    Status::Denied => {
                        settings_button("readiness-settings-system-audio", c, cx, Permission::SystemAudio)
                    }
                }
            },
        ),
        row(
            p,
            page.vad_ready(),
            Ic::Cpu,
            c.t(Tr::TranscriptionReadinessVadTitle),
            c.t(Tr::TranscriptionReadinessVad),
            if page.vad_ready() {
                pill(p, c.t(Tr::ModelsDownloaded))
            } else {
                ui::button(
                    "readiness-open-models",
                    p,
                    &c.t(Tr::StatusOpenModelSettings),
                    BtnKind::Secondary,
                    Some(Ic::External),
                    cbk(cx, |page, _, cx| {
                        let st = page.st.clone();
                        crate::settings::open_vad_models(st, cx);
                    }),
                )
                .into_any_element()
            },
        ),
    ];
    let count = rows.len();
    let card = ui::card(p).children(
        rows.into_iter()
            .enumerate()
            .map(|(i, row)| if i + 1 == count { row } else { row.border_b_1().border_color(p.border) }),
    );

    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(div().w(px(GATE_W)).flex().flex_col().gap(px(22.)).child(header).child(card))
}

/// `ready` takes a soft green plate and the solid green chip with a white glyph; pending rows stay on the card.
fn row(p: &Palette, ready: bool, icon: Ic, title: SharedString, desc: SharedString, trailing: AnyElement) -> Div {
    let chip = div()
        .size(px(36.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .bg(if ready { p.success } else { p.sub })
        .child(ui::icon(icon, 18., if ready { ui::white() } else { p.text }));
    div()
        .w_full()
        .h(px(ROW_H))
        .px(px(18.))
        .flex()
        .items_center()
        .gap(px(14.))
        .when(ready, |row| row.bg(p.success_row))
        .child(chip)
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(text(title, 14., FontWeight::SEMIBOLD, p.text).line_height(px(20.)))
                .child(text(desc, 12., FontWeight::NORMAL, p.muted).line_height(px(18.))),
        )
        .child(div().flex_none().child(trailing))
}

fn pill(p: &Palette, label: SharedString) -> AnyElement {
    div()
        .h(px(24.))
        .px(px(10.))
        .flex_none()
        .flex()
        .items_center()
        .rounded_full()
        .bg(p.success)
        .child(text(label, 12., FontWeight::MEDIUM, ui::white()).line_height(px(18.)))
        .into_any_element()
}

fn allow_button(id: &'static str, c: &Ctx, cx: &mut Context<TranscriptionPage>, permission: Permission) -> AnyElement {
    ui::button(
        id,
        &c.p,
        &c.t(Tr::GeneralAllow),
        BtnKind::Secondary,
        None,
        cbk(cx, move |_, _, cx| {
            permissions::request(permission);
            cx.notify();
        }),
    )
    .into_any_element()
}

fn settings_button(
    id: &'static str,
    c: &Ctx,
    cx: &mut Context<TranscriptionPage>,
    permission: Permission,
) -> AnyElement {
    ui::button(
        id,
        &c.p,
        &c.t(Tr::StatusOpenSystemSettings),
        BtnKind::Secondary,
        None,
        cbk(cx, move |_, _, _| permissions::open_settings(permission)),
    )
    .into_any_element()
}
