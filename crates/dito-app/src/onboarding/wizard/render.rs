use dito_core::i18n::Tr;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::theme;
use crate::ui::{self, text};

use super::super::{components, steps, transition};
use super::{Onboarding, Step};

impl Render for Onboarding {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scope = self.motion.handle();
        ui::with_motion_scope(scope, || ui::tooltip_root(self.render_scoped(window, cx).children(self.editor.clone())))
    }
}

impl Onboarding {
    fn render_scoped(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = crate::settings::Ctx::new(&self.st, cx);
        theme::apply(&c.p, cx);
        let p = c.p;
        ui::set_ui_language(c.lang);

        let slide = self.slide.as_ref().map(|s| (s.from, s.forward, s.motion, s.at.elapsed().as_secs_f32()));
        let root = div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(crate::ui::cjk_family())
            .capture_any_mouse_down(|_, window, cx| window.blur(cx));

        let Some((from, forward, motion, t)) = slide else {
            return root.child(self.page(self.step, &c, cx, None));
        };
        if t >= transition::total(motion) {
            self.slide = None;
            // The page is drawn again as a new element tree: hover state must be read afresh from the pointer.
            self.reset_motion();
            return root.child(self.page(self.step, &c, cx, None));
        }
        window.request_animation_frame();

        let v = transition::view(forward, motion, t);

        // The leaving page is built under its own motion scope, so controls that both pages share (the footer's
        // buttons) never trade hover state.
        let leaving_scope = self.leaving_motion.handle();
        let leaving = ui::with_motion_scope(leaving_scope, || self.page(from, &c, cx, None));
        let arriving = self.page(self.step, &c, cx, Some(v.art_dx));
        root.child(div().id("page-leaving").absolute().size_full().left(px(v.out_dx)).opacity(v.out_a).child(leaving))
            .child(div().id("page-arriving").absolute().size_full().left(px(v.in_dx)).opacity(v.in_a).child(arriving))
    }

    /// `art_dx` offsets the art panel while the page slides in.
    fn page(&mut self, step: Step, c: &crate::settings::Ctx, cx: &mut Context<Self>, art_dx: Option<f32>) -> Div {
        let p = c.p;
        if step == Step::Welcome {
            return steps::welcome(self, c, cx);
        }
        let current = std::mem::replace(&mut self.step, step);
        let steps::StepPage { title, desc, body } = steps::StepPage::current(self, c, cx);
        let next_label = if step == Step::Try { c.t(Tr::OnboardingDone) } else { c.t(Tr::OnboardingContinue) };
        let can_next = self.can_next(cx);
        let footer = components::footer(step, c, cx, next_label, can_next);
        let art = components::art(step, &p);
        self.step = current;
        let left = div()
            .flex_1()
            .h_full()
            .px(px(56.))
            .pt(px(60.))
            .pb(px(36.))
            .flex()
            .flex_col()
            .gap(px(28.))
            .child(components::stepper(step, &p, c.lang))
            .child(div().flex().flex_col().gap(px(10.)).child(text(title, 28., FontWeight::BOLD, p.text)).child(text(
                desc,
                13.,
                FontWeight::NORMAL,
                p.muted,
            )))
            .child(div().flex_1().w_full().child(body))
            .child(footer);
        let art = match art_dx {
            Some(dx) => art.relative().left(px(dx)),
            None => art,
        };
        div().size_full().flex().child(left).child(art)
    }
}
