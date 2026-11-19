//! The General page controls that only show and save one config field. Each field's mapping to its control is written
//! once here; a control with any other effect keeps its own code.

use std::rc::Rc;
use std::sync::Arc;

use dito_core::config::{
    Appearance, Config, DisplayLanguage, HudPosition, InsertMethod, MusicMuteMode, PreferredLanguage, RecordMode,
};
use gpui_kit::*;

use super::page::GeneralPage;
use super::system::LOOKS;
use crate::services;
use crate::settings::Ctx;
use crate::state::AppState;
use crate::theme::Palette;
use crate::ui::{self, IndexCb, ToggleCb};

/// A field shown as one option out of several: `values[i]` is what option `i` of the control saves.
pub(super) struct Choice<T: 'static> {
    values: &'static [T],
    get: fn(&Config) -> T,
    set: fn(&mut Config, T),
}

impl<T: Copy + PartialEq + 'static> Choice<T> {
    pub(super) fn index(&self, cfg: &Config) -> usize {
        let current = (self.get)(cfg);
        self.values.iter().position(|value| *value == current).unwrap_or(0)
    }

    fn pick(&self, cfg: &mut Config, index: usize) {
        if let Some(value) = self.values.get(index) {
            (self.set)(cfg, *value);
        }
    }

    pub(super) fn on_pick(&'static self, st: &Arc<AppState>) -> IndexCb {
        let st = st.clone();
        Rc::new(move |index, _, _| services::save_config(&st, |cfg| self.pick(cfg, index)))
    }
}

/// A field shown as a switch.
pub(super) struct Flag {
    get: fn(&Config) -> bool,
    set: fn(&mut Config, bool),
}

impl Flag {
    fn on_toggle(&'static self, st: &Arc<AppState>) -> ToggleCb {
        let st = st.clone();
        Rc::new(move |on, _, _| services::save_config(&st, |cfg| (self.set)(cfg, on)))
    }
}

pub(super) static RECORD_MODE: Choice<RecordMode> = Choice {
    values: &[RecordMode::Toggle, RecordMode::Hold],
    get: |cfg| cfg.general.record_mode,
    set: |cfg, value| cfg.general.record_mode = value,
};

pub(super) static MUSIC_MUTE: Choice<MusicMuteMode> = Choice {
    values: &[MusicMuteMode::Off, MusicMuteMode::Auto],
    get: |cfg| cfg.general.music_mute,
    set: |cfg, value| cfg.general.music_mute = value,
};

pub(super) static INSERT_METHOD: Choice<InsertMethod> = Choice {
    values: &[InsertMethod::KeyInput, InsertMethod::Paste],
    get: |cfg| cfg.general.insert_method,
    set: |cfg, value| cfg.general.insert_method = value,
};

pub(super) static HUD_POSITION: Choice<HudPosition> = Choice {
    values: &[HudPosition::Centered, HudPosition::Pointer],
    get: |cfg| cfg.general.hud_position,
    set: |cfg, value| cfg.general.hud_position = value,
};

pub(super) static APPEARANCE: Choice<Appearance> =
    Choice { values: &LOOKS, get: |cfg| cfg.general.appearance, set: |cfg, value| cfg.general.appearance = value };

pub(super) static DISPLAY_LANGUAGE: Choice<DisplayLanguage> = Choice {
    values: &DisplayLanguage::ALL,
    get: |cfg| cfg.general.display_language,
    set: |cfg, value| cfg.general.display_language = value,
};

pub(super) static PREFERRED_LANGUAGE: Choice<PreferredLanguage> = Choice {
    values: &PreferredLanguage::ALL,
    get: |cfg| cfg.general.preferred_language,
    set: |cfg, value| cfg.general.preferred_language = value,
};

pub(super) static POLISH_ENABLED: Flag = Flag { get: |cfg| cfg.polish.enabled, set: |cfg, on| cfg.polish.enabled = on };

pub(super) static CONFIRM_BEFORE_REPLACE: Flag =
    Flag { get: |cfg| cfg.general.confirm_before_replace, set: |cfg, on| cfg.general.confirm_before_replace = on };

pub(super) static SHOW_DOCK_ICON: Flag =
    Flag { get: |cfg| cfg.general.show_dock_icon, set: |cfg, on| cfg.general.show_dock_icon = on };

impl GeneralPage {
    pub(super) fn bound_seg<T: Copy + PartialEq>(
        &self,
        c: &Ctx,
        id: &'static str,
        labels: &[SharedString],
        choice: &'static Choice<T>,
    ) -> Stateful<Div> {
        ui::seg(id, &c.p, labels, choice.index(&self.st.cfg.get()), choice.on_pick(&self.st))
    }

    pub(super) fn bound_select<T: Copy + PartialEq>(
        &self,
        c: &Ctx,
        cx: &mut Context<Self>,
        id: &'static str,
        options: &[SharedString],
        choice: &'static Choice<T>,
    ) -> Div {
        ui::select(
            id,
            &c.p,
            options,
            choice.index(&self.st.cfg.get()),
            self.open == Some(id),
            Some(240.),
            self.toggle(cx, id),
            choice.on_pick(&self.st),
        )
    }

    pub(super) fn bound_switch(&self, p: &Palette, id: &'static str, flag: &'static Flag) -> Stateful<Div> {
        ui::switch(id, p, (flag.get)(&self.st.cfg.get()), flag.on_toggle(&self.st))
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;

    use dito_core::config::Config;

    use super::{
        APPEARANCE, CONFIRM_BEFORE_REPLACE, Choice, DISPLAY_LANGUAGE, Flag, HUD_POSITION, INSERT_METHOD, MUSIC_MUTE,
        POLISH_ENABLED, PREFERRED_LANGUAGE, RECORD_MODE, SHOW_DOCK_ICON,
    };

    fn choice_round_trips<T: Copy + PartialEq + Debug>(choice: &Choice<T>) {
        let mut cfg = Config::default();
        for (index, value) in choice.values.iter().enumerate() {
            choice.pick(&mut cfg, index);
            assert_eq!((choice.get)(&cfg), *value);
            assert_eq!(choice.index(&cfg), index);
        }
    }

    fn flag_round_trips(flag: &Flag) {
        let mut cfg = Config::default();
        for on in [true, false, true] {
            (flag.set)(&mut cfg, on);
            assert_eq!((flag.get)(&cfg), on);
        }
    }

    #[test]
    fn every_bound_control_round_trips_its_values() {
        choice_round_trips(&RECORD_MODE);
        choice_round_trips(&MUSIC_MUTE);
        choice_round_trips(&INSERT_METHOD);
        choice_round_trips(&HUD_POSITION);
        choice_round_trips(&APPEARANCE);
        choice_round_trips(&DISPLAY_LANGUAGE);
        choice_round_trips(&PREFERRED_LANGUAGE);
        flag_round_trips(&POLISH_ENABLED);
        flag_round_trips(&CONFIRM_BEFORE_REPLACE);
        flag_round_trips(&SHOW_DOCK_ICON);
    }
}
