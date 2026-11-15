//! Catalog facts shown as labels after a model's name, per `design:settings/local-models/labels`.

use dito_core::i18n::{Lang, Tr, fmt};
use dito_local::catalog::{ModelKind, ModelSpec, Quantization};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::theme::Palette;
use crate::ui::{self, text};

/// Labels keep the notation the field uses in every display language; each tooltip says the same in everyday words.
pub(super) fn model_labels(c: &Ctx, spec: &ModelSpec) -> Div {
    let size = ui::fmt_bytes(spec.total_size());
    let mut labels = vec![
        ("parameters", parameters_label(spec.parameters_m), parameters_tip(c.lang, spec.parameters_m)),
        ("quantization", spec.quantization.label.to_string(), quantization_tip(c.lang, spec.quantization)),
    ];
    // A detector is language agnostic, which the multilingual code states as well as it does for a speech model.
    if matches!(spec.kind, ModelKind::Asr | ModelKind::Vad) {
        let (code, tip) = match spec.language_focus {
            None => ("MULT", Tr::ModelsLabelMultilingualTip),
            Some(Lang::Ja) => ("JA", Tr::ModelsLabelJapaneseTip),
            Some(Lang::En) => ("EN", Tr::ModelsLabelEnglishTip),
            Some(Lang::Zh) => ("ZH", Tr::ModelsLabelChineseTip),
        };
        labels.push(("language", code.to_string(), c.lang.tr(tip).to_string()));
    }
    labels.push(("size", size.clone(), fmt::model_size_tip(c.lang, &size)));
    div().flex().flex_none().items_center().gap(px(10.)).children(labels.into_iter().map(|(fact, label, tip)| {
        let id = ElementId::Name(format!("model-label-{}-{fact}", spec.id).into());
        model_label(&c.p, id, label.into(), tip.into())
    }))
}

/// Only colors move on hover, so the row never shifts.
fn model_label(p: &Palette, id: ElementId, label: SharedString, tip: SharedString) -> Stateful<Div> {
    ui::tip_label(id.clone(), tip);
    let (rest_border, meta, bright) = (p.border_strong, p.model_meta, p.text);
    div()
        .id(id.clone())
        .relative()
        .flex_none()
        .on_hover(ui::hover_tracker(id.clone()))
        .child(ui::tweened(
            ElementId::from((id.clone(), "label")),
            f32::from(ui::is_hovered(&id)),
            ui::HOVER_MS,
            move |v| {
                div()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(ui::mix(rest_border, meta, v))
                    .child(
                        text(label.clone(), 11., FontWeight::MEDIUM, ui::mix(meta, bright, v))
                            .line_height(relative(1.)),
                    )
            },
        ))
        .child(ui::tip_above(p, &id))
}

fn parameters_label(millions: u32) -> String {
    if millions < 1000 { format!("{millions}M") } else { format!("{}B", one_decimal(f64::from(millions) / 1000.)) }
}

fn parameters_tip(lang: Lang, millions: u32) -> String {
    let count = match lang {
        Lang::En if millions >= 1000 => {
            fmt::model_parameters_count_billions(lang, &one_decimal(f64::from(millions) / 1000.))
        }
        Lang::En => fmt::model_parameters_count(lang, &two_digits(millions).to_string()),
        // Japanese and Chinese count in units of a hundred million.
        Lang::Ja | Lang::Zh => fmt::model_parameters_count(lang, &two_figures(f64::from(millions) / 100.)),
    };
    fmt::model_parameters_tip(lang, &count, &parameters_label(millions))
}

fn quantization_tip(lang: Lang, quantization: Quantization) -> String {
    let (min, max) = quantization.bits;
    let to = if min == max { min.to_string() } else { fmt::model_bit_range(lang, &max.to_string(), &min.to_string()) };
    let from = quantization.source_bits.to_string();
    if min >= 8 {
        fmt::model_quantization_tip(lang, &from, &to)
    } else {
        fmt::model_quantization_lossy_tip(lang, &from, &to)
    }
}

fn one_decimal(value: f64) -> String {
    let text = format!("{value:.1}");
    text.strip_suffix(".0").map(str::to_string).unwrap_or(text)
}

/// Two significant figures: one decimal below ten, whole numbers from ten.
fn two_figures(value: f64) -> String {
    if value >= 10. { format!("{value:.0}") } else { one_decimal(value) }
}

fn two_digits(value: u32) -> u32 {
    if value < 100 { value } else { (f64::from(value) / 10.).round() as u32 * 10 }
}

#[cfg(test)]
mod tests {
    use super::{Lang, Quantization, parameters_label, parameters_tip, quantization_tip, two_digits, two_figures};

    #[test]
    fn a_parameter_count_reads_in_the_field_notation_and_in_everyday_units() {
        assert_eq!(parameters_label(616), "616M");
        assert_eq!(parameters_label(2349), "2.3B");
        assert_eq!(parameters_label(9000), "9B");
        assert_eq!(two_figures(6.16), "6.2");
        assert_eq!(two_figures(8.), "8");
        assert_eq!(two_figures(23.49), "23");
        assert_eq!(two_digits(616), 620);
        assert!(parameters_tip(Lang::En, 616).contains("620 million"));
        assert!(parameters_tip(Lang::En, 2349).contains("2.3 billion"));
        assert!(parameters_tip(Lang::Ja, 616).starts_with("616M"));
    }

    #[test]
    fn only_eight_bits_or_more_is_described_as_keeping_the_accuracy() {
        let kept = quantization_tip(Lang::En, Quantization::Q8_0);
        let mixed = quantization_tip(Lang::En, Quantization { label: "INT4/8", source_bits: 32, bits: (4, 8) });
        assert_ne!(kept, quantization_tip(Lang::En, Quantization::Q5_K_M));
        assert!(kept.contains("16") && kept.contains('8'));
        assert!(mixed.contains("32") && mixed.contains("4 to 8"));
    }
}
