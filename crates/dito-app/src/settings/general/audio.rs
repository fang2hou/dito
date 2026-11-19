use std::rc::Rc;

use dito_core::config::PreferredLanguage;
use dito_core::i18n::Tr;
use gpui_kit::*;

use super::bindings::PREFERRED_LANGUAGE;
use super::page::GeneralPage;
use crate::services;
use crate::settings::Ctx;
use crate::ui::{self, SelectBadge, SelectItem};

impl GeneralPage {
    pub(super) fn speech_recognition_group(&self, c: &Ctx, cx: &mut Context<Self>) -> impl IntoElement {
        let p = c.p;
        ui::group(
            &p,
            c.t(Tr::GeneralSpeechRecognition),
            vec![
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralMicrophone),
                    c.t(Tr::GeneralMicrophoneHint),
                    self.microphone_select(c, cx),
                    false,
                ),
                ui::row_described(
                    &p,
                    c.t(Tr::GeneralPreferredLanguage),
                    c.t(Tr::GeneralPreferredLanguageHint),
                    self.preferred_language_select(c, cx),
                    true,
                ),
            ],
        )
    }

    /// The saved choice stays listed (and selected) while the list is unloaded or the device is unplugged.
    fn microphone_names(&self) -> Vec<String> {
        let saved = self.st.cfg.get().general.microphone.clone();
        let mut names: Vec<String> = self.devices.iter().map(|d| d.name.clone()).collect();
        if !saved.is_empty() && !names.contains(&saved) {
            names.push(saved);
        }
        names
    }

    fn microphone_items(&self, c: &Ctx, names: &[String]) -> Vec<SelectItem> {
        let cfg = self.st.cfg.get();
        let g = &cfg.general;
        let mut items = vec![SelectItem { label: c.t(Tr::GeneralAutomatic), badge: None }];
        items.extend(names.iter().map(|name| {
            let badge = self.devices.iter().find(|d| d.name == *name).map(|d| {
                let on = d.auto(&g.microphone_auto);
                let (st, device) = (self.st.clone(), name.clone());
                SelectBadge {
                    active: !on,
                    tip: if on { c.t(Tr::GeneralIncludedInAutoPicking) } else { c.t(Tr::GeneralManualOnlyClickToAuto) },
                    on_click: Rc::new(move |_, _| {
                        let (recorder_st, device) = (st.clone(), device.clone());
                        services::save_config(&st, move |cfg| {
                            cfg.general.microphone_auto.insert(device, !on);
                            recorder_st.recorder_auto(&cfg.general.microphone_auto);
                        })
                    }),
                }
            });
            SelectItem { label: SharedString::from(name.clone()), badge }
        }));
        items
    }

    fn microphone_select(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let st = self.st.clone();
        let names = self.microphone_names();
        let items = self.microphone_items(c, &names);
        let selected = names.iter().position(|d| *d == st.cfg.get().general.microphone).map(|i| i + 1).unwrap_or(0);
        ui::select_items(
            "mic",
            &c.p,
            &items,
            selected,
            self.open == Some("mic"),
            Some(240.),
            self.toggle(cx, "mic"),
            Rc::new(move |i, _, _| {
                let name = if i == 0 { String::new() } else { names.get(i - 1).cloned().unwrap_or_default() };
                st.recorder_device(&name);
                services::save_config(&st, move |c| c.general.microphone = name)
            }),
        )
    }

    fn preferred_language_select(&self, c: &Ctx, cx: &mut Context<Self>) -> Div {
        let options = PreferredLanguage::ALL.map(|language| language.name(c.lang).into());
        self.bound_select(c, cx, "pref-lang", &options, &PREFERRED_LANGUAGE)
    }
}
