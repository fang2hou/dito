//! What typing into a field of a polish form does to its setting. Text that does not parse is not saved: the last valid
//! value stays in use and the field shows the danger border until the text parses; an empty field clears the value.

use std::ops::{RangeBounds, RangeFrom, RangeInclusive};

use dito_core::config::{CloudPolishConfiguration, LocalPolishParams};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq)]
enum FieldText<T> {
    /// Empty: the value is cleared.
    Unset,
    Set(T),
    Invalid,
}

impl<T> FieldText<T> {
    fn value(self) -> Option<T> {
        match self {
            Self::Set(value) => Some(value),
            Self::Unset | Self::Invalid => None,
        }
    }
}

/// What a temperature may be: from zero up. The top is open because the APIs differ in how far they go.
const TEMPERATURE: RangeFrom<f32> = 0.0..;

const TOP_P: RangeInclusive<f32> = 0.0..=1.0;

/// A backend rejects a number outside it only when the request is sent, far from the field that holds it.
fn decimal(text: &str, range: impl RangeBounds<f32>) -> FieldText<f32> {
    match text.trim() {
        "" => FieldText::Unset,
        text => text
            .parse::<f32>()
            .ok()
            .filter(|number| number.is_finite() && range.contains(number))
            .map_or(FieldText::Invalid, FieldText::Set),
    }
}

fn whole(text: &str) -> FieldText<u32> {
    match text.trim() {
        "" => FieldText::Unset,
        text => text.parse::<u32>().ok().filter(|number| *number > 0).map_or(FieldText::Invalid, FieldText::Set),
    }
}

fn object(text: &str) -> FieldText<Map<String, Value>> {
    match text.trim() {
        "" => FieldText::Unset,
        text => match serde_json::from_str(text) {
            Ok(Value::Object(object)) => FieldText::Set(object),
            _ => FieldText::Invalid,
        },
    }
}

/// The text a number shows. A float prints as the shortest text that reads back as the same number.
pub(super) fn show_number<T: ToString>(value: Option<T>) -> String {
    value.map(|value| value.to_string()).unwrap_or_default()
}

pub(super) fn show_extra(extra: &Map<String, Value>) -> String {
    if extra.is_empty() { String::new() } else { Value::Object(extra.clone()).to_string() }
}

/// One per place in the form, not one per template: the template only decides the labels and the placeholders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CloudPolishField {
    Name,
    Address,
    Model,
    /// The API key, which is kept in the Keychain and not in the configuration.
    Key,
    Temperature,
    TopP,
    MaxTokens,
    Effort,
    Extra,
}

impl CloudPolishField {
    pub(super) const ALL: [CloudPolishField; 9] = [
        Self::Name,
        Self::Address,
        Self::Model,
        Self::Key,
        Self::Temperature,
        Self::TopP,
        Self::MaxTokens,
        Self::Effort,
        Self::Extra,
    ];

    pub(super) fn invalid(self, text: &str) -> bool {
        match self {
            Self::Temperature => decimal(text, TEMPERATURE) == FieldText::Invalid,
            Self::TopP => decimal(text, TOP_P) == FieldText::Invalid,
            Self::MaxTokens => whole(text) == FieldText::Invalid,
            Self::Extra => object(text) == FieldText::Invalid,
            Self::Name | Self::Address | Self::Model | Self::Key | Self::Effort => false,
        }
    }

    /// Not when it is invalid, and not for a blank name: a configuration always has one.
    pub(super) fn accepts(self, text: &str) -> bool {
        !self.invalid(text) && !(self == Self::Name && text.trim().is_empty())
    }

    /// What the form shows of this setting of `config`. The key is not part of it.
    pub(super) fn show(self, config: &CloudPolishConfiguration) -> String {
        match self {
            Self::Name => config.name.clone(),
            Self::Address => config.target.address().to_owned(),
            Self::Model => config.target.model().to_owned(),
            Self::Temperature => show_number(config.params.temperature),
            Self::TopP => show_number(config.params.top_p),
            Self::MaxTokens => show_number(config.params.max_tokens),
            Self::Effort => config.params.reasoning_effort.as_str().to_owned(),
            Self::Extra => show_extra(&config.params.extra),
            Self::Key => String::new(),
        }
    }

    /// `false` when it is not [accepted](Self::accepts), and then `config` is as it was. The key lives in the Keychain,
    /// so there is nothing to put.
    pub(super) fn apply(self, config: &mut CloudPolishConfiguration, text: &str) -> bool {
        if !self.accepts(text) {
            return false;
        }
        let params = &mut config.params;
        match self {
            Self::Name => config.name = text.to_owned(),
            Self::Address => *config.target.address_mut() = text.to_owned(),
            Self::Model => *config.target.model_mut() = text.to_owned(),
            Self::Temperature => params.temperature = decimal(text, TEMPERATURE).value(),
            Self::TopP => params.top_p = decimal(text, TOP_P).value(),
            Self::MaxTokens => params.max_tokens = whole(text).value(),
            Self::Effort => params.reasoning_effort = text.trim().to_owned().into(),
            Self::Extra => params.extra = object(text).value().unwrap_or_default(),
            Self::Key => {}
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalPolishField {
    Temperature,
    TopP,
    TopK,
    MaxTokens,
}

impl LocalPolishField {
    pub(super) const ALL: [LocalPolishField; 4] = [Self::Temperature, Self::TopP, Self::TopK, Self::MaxTokens];

    pub(super) fn invalid(self, text: &str) -> bool {
        match self {
            Self::Temperature => decimal(text, TEMPERATURE) == FieldText::Invalid,
            Self::TopP => decimal(text, TOP_P) == FieldText::Invalid,
            Self::TopK | Self::MaxTokens => whole(text) == FieldText::Invalid,
        }
    }

    pub(super) fn show(self, params: &LocalPolishParams) -> String {
        match self {
            Self::Temperature => show_number(params.temperature),
            Self::TopP => show_number(params.top_p),
            Self::TopK => show_number(params.top_k),
            Self::MaxTokens => show_number(params.max_tokens),
        }
    }

    /// Put `text` into `params`. `false` when it does not parse, and then `params` is as it was.
    pub(super) fn apply(self, params: &mut LocalPolishParams, text: &str) -> bool {
        if self.invalid(text) {
            return false;
        }
        match self {
            Self::Temperature => params.temperature = decimal(text, TEMPERATURE).value(),
            Self::TopP => params.top_p = decimal(text, TOP_P).value(),
            Self::TopK => params.top_k = whole(text).value(),
            Self::MaxTokens => params.max_tokens = whole(text).value(),
        }
        true
    }
}

pub(super) fn settled_cloud_polish_name<'a>(typed: &'a str, saved: &'a str) -> &'a str {
    if typed.trim().is_empty() { saved } else { typed }
}

#[cfg(test)]
mod tests {
    use dito_core::config::{CloudPolishTemplate, ReasoningEffort};

    use super::*;

    fn configuration() -> CloudPolishConfiguration {
        CloudPolishTemplate::Compat.configuration("compat", "Mine")
    }

    #[test]
    fn decimals_parse_and_empty_means_unset() {
        assert_eq!(decimal("0.2", TEMPERATURE), FieldText::Set(0.2));
        assert_eq!(decimal(" 1 ", TEMPERATURE), FieldText::Set(1.));
        assert_eq!(decimal(".5", TOP_P), FieldText::Set(0.5));
        assert_eq!(decimal("0.", TOP_P), FieldText::Set(0.));
        assert_eq!(decimal("", TEMPERATURE), FieldText::Unset);
        assert_eq!(decimal("  ", TOP_P), FieldText::Unset);
        for text in ["abc", "0,2", "0.2.3", "1 2", "nan", "inf", "1e999", "--1", ".", "-"] {
            assert_eq!(decimal(text, TEMPERATURE), FieldText::Invalid, "{text:?}");
        }
    }

    #[test]
    fn a_temperature_is_never_negative_and_has_no_top() {
        assert_eq!(decimal("0", TEMPERATURE), FieldText::Set(0.));
        assert_eq!(decimal("2.5", TEMPERATURE), FieldText::Set(2.5), "the APIs differ in how far they go");
        assert_eq!(decimal("100", TEMPERATURE), FieldText::Set(100.));
        for text in ["-0.1", "-1", "-0.0001"] {
            assert_eq!(decimal(text, TEMPERATURE), FieldText::Invalid, "{text:?}");
        }
    }

    #[test]
    fn a_top_p_is_a_share_from_zero_to_one() {
        for (text, share) in [("0", 0.), ("0.95", 0.95), ("1", 1.), ("1.0", 1.)] {
            assert_eq!(decimal(text, TOP_P), FieldText::Set(share), "{text:?}");
        }
        for text in ["-0.1", "1.01", "2", "100"] {
            assert_eq!(decimal(text, TOP_P), FieldText::Invalid, "{text:?}");
        }
    }

    #[test]
    fn whole_numbers_are_positive_and_fit_the_type() {
        assert_eq!(whole("2048"), FieldText::Set(2048));
        assert_eq!(whole(" 7 "), FieldText::Set(7));
        assert_eq!(whole(""), FieldText::Unset);
        for text in ["0", "-1", "12.5", "1_000", "99999999999", "ten", "1e3"] {
            assert_eq!(whole(text), FieldText::Invalid, "{text:?}");
        }
    }

    #[test]
    fn extra_must_be_a_json_object() {
        assert_eq!(object(r#"{"seed": 42}"#), FieldText::Set(serde_json::from_str(r#"{"seed":42}"#).unwrap()));
        assert_eq!(object("{}"), FieldText::Set(Map::new()));
        assert_eq!(object(""), FieldText::Unset);
        for text in ["[1]", "5", r#""x""#, "null", "not json", r#"{"seed":"#, "{seed: 1}"] {
            assert_eq!(object(text), FieldText::Invalid, "{text:?}");
        }
    }

    #[test]
    fn an_invalid_text_is_not_saved_and_the_last_valid_value_stays() {
        let mut config = configuration();
        assert!(CloudPolishField::Temperature.apply(&mut config, "0.3"));
        assert_eq!(config.params.temperature, Some(0.3));
        assert!(!CloudPolishField::Temperature.apply(&mut config, "abc"));
        assert!(!CloudPolishField::Temperature.apply(&mut config, "-0.5"));
        assert_eq!(config.params.temperature, Some(0.3), "the last valid value stays in use");
        assert!(CloudPolishField::Temperature.invalid("abc") && CloudPolishField::Temperature.invalid("-0.5"));

        assert!(CloudPolishField::TopP.apply(&mut config, "0.9"));
        assert!(!CloudPolishField::TopP.apply(&mut config, "1.5"));
        assert!(!CloudPolishField::TopP.apply(&mut config, "-0.1"));
        assert_eq!(config.params.top_p, Some(0.9));

        assert!(CloudPolishField::MaxTokens.apply(&mut config, "512"));
        assert!(!CloudPolishField::MaxTokens.apply(&mut config, "0"));
        assert!(!CloudPolishField::MaxTokens.apply(&mut config, "1.5"));
        assert_eq!(config.params.max_tokens, Some(512));

        assert!(CloudPolishField::Extra.apply(&mut config, r#"{"seed": 42}"#));
        assert!(!CloudPolishField::Extra.apply(&mut config, "[1]"));
        assert!(!CloudPolishField::Extra.apply(&mut config, r#"{"seed":"#));
        assert_eq!(config.params.extra["seed"], 42, "the last valid object stays in use");
    }

    #[test]
    fn a_number_that_is_still_a_number_while_typing_is_saved() {
        let mut config = configuration();
        for (typed, saved) in [("0", Some(0.)), ("0.", Some(0.)), ("0.7", Some(0.7))] {
            assert!(CloudPolishField::TopP.apply(&mut config, typed), "{typed:?}");
            assert_eq!(config.params.top_p, saved, "{typed:?}");
        }
    }

    #[test]
    fn an_empty_field_clears_the_value() {
        let mut config = configuration();
        for field in [CloudPolishField::Temperature, CloudPolishField::TopP, CloudPolishField::MaxTokens] {
            assert!(field.apply(&mut config, "1"));
        }
        assert!(CloudPolishField::Extra.apply(&mut config, r#"{"a": 1}"#));
        assert!(CloudPolishField::Effort.apply(&mut config, "high"));
        for field in [
            CloudPolishField::Temperature,
            CloudPolishField::TopP,
            CloudPolishField::MaxTokens,
            CloudPolishField::Extra,
            CloudPolishField::Effort,
        ] {
            assert!(field.apply(&mut config, ""), "{field:?}");
        }
        let params = &config.params;
        assert_eq!((params.temperature, params.top_p, params.max_tokens), (None, None, None));
        assert!(params.extra.is_empty());
        assert_eq!(params.reasoning_effort.as_str(), "", "an empty effort is unset");
        assert!(CloudPolishField::Extra.apply(&mut configuration(), "  "), "blank is empty too");
    }

    #[test]
    fn the_effort_is_free_text_and_the_known_values_keep_their_names() {
        let mut config = configuration();
        assert!(CloudPolishField::Effort.apply(&mut config, " high "));
        assert_eq!(config.params.reasoning_effort, ReasoningEffort::High);
        assert!(CloudPolishField::Effort.apply(&mut config, "xhigh"));
        assert_eq!(config.params.reasoning_effort.as_str(), "xhigh");
        assert!(!CloudPolishField::Effort.invalid("anything at all"));
    }

    #[test]
    fn the_name_saves_as_typed_but_never_blank() {
        let mut config = configuration();
        assert!(CloudPolishField::Name.apply(&mut config, "Fast model"));
        assert_eq!(config.name, "Fast model");
        assert!(CloudPolishField::Name.apply(&mut config, "Fast model "), "as typed, trailing space included");
        assert_eq!(config.name, "Fast model ");
        for blank in ["", "   "] {
            assert!(!CloudPolishField::Name.apply(&mut config, blank));
            assert!(!CloudPolishField::Name.invalid(blank), "a blank name is not shown as wrong, it returns on blur");
        }
        assert_eq!(config.name, "Fast model ", "the last saved name stays");
        assert_eq!(settled_cloud_polish_name("", &config.name), "Fast model ");
        assert_eq!(settled_cloud_polish_name("  ", &config.name), "Fast model ");
        assert_eq!(settled_cloud_polish_name("Other", &config.name), "Other");
    }

    #[test]
    fn address_and_model_save_as_typed_into_the_field_of_the_template() {
        for template in CloudPolishTemplate::ALL {
            let mut config = template.configuration("x", "X");
            assert!(CloudPolishField::Address.apply(&mut config, "typed address"));
            assert!(CloudPolishField::Model.apply(&mut config, "typed model"));
            assert_eq!((config.target.address(), config.target.model()), ("typed address", "typed model"));
            assert!(CloudPolishField::Model.apply(&mut config, ""), "empty is a value here");
            assert_eq!(config.target.model(), "");
        }
    }

    #[test]
    fn what_the_form_shows_reads_back_as_the_same_setting() {
        let mut config = configuration();
        for (field, text) in [
            (CloudPolishField::Temperature, "0.2"),
            (CloudPolishField::TopP, "0.95"),
            (CloudPolishField::MaxTokens, "2048"),
            (CloudPolishField::Effort, "low"),
            (CloudPolishField::Extra, r#"{"seed":42,"stop":null}"#),
        ] {
            assert!(field.apply(&mut config, text));
            assert_eq!(field.show(&config), text, "{field:?}");
        }
        let mut copy = configuration();
        for field in CloudPolishField::ALL {
            assert!(field.apply(&mut copy, &field.show(&config)), "{field:?}");
        }
        assert_eq!(copy.params, config.params);
        assert_eq!(CloudPolishField::Key.show(&config), "", "the key is not in the configuration");
    }

    #[test]
    fn local_parameters_follow_the_same_rules() {
        let mut params = LocalPolishParams::default();
        assert!(LocalPolishField::Temperature.apply(&mut params, "0.3"));
        assert!(LocalPolishField::TopP.apply(&mut params, "0.9"));
        assert!(LocalPolishField::TopK.apply(&mut params, "40"));
        assert!(LocalPolishField::MaxTokens.apply(&mut params, "512"));
        assert_eq!(
            params,
            LocalPolishParams { temperature: Some(0.3), top_p: Some(0.9), top_k: Some(40), max_tokens: Some(512) }
        );

        assert!(!LocalPolishField::TopK.apply(&mut params, "0"));
        assert!(!LocalPolishField::TopK.apply(&mut params, "4.5"));
        assert!(!LocalPolishField::Temperature.apply(&mut params, "warm"));
        assert!(!LocalPolishField::Temperature.apply(&mut params, "-1"));
        assert!(!LocalPolishField::TopP.apply(&mut params, "1.1"));
        assert!(!LocalPolishField::TopP.apply(&mut params, "-0.2"));
        assert_eq!(params.top_k, Some(40), "the last valid value stays");
        assert_eq!((params.temperature, params.top_p), (Some(0.3), Some(0.9)));
        assert!(LocalPolishField::TopK.invalid("0") && !LocalPolishField::TopK.invalid(""));
        assert!(LocalPolishField::TopP.invalid("1.5") && !LocalPolishField::TopP.invalid("1"));
        assert!(LocalPolishField::Temperature.invalid("-1") && !LocalPolishField::Temperature.invalid("3"));

        for field in LocalPolishField::ALL {
            assert!(field.apply(&mut params, ""));
        }
        assert_eq!(params, LocalPolishParams::default(), "empty fields use the recommended values again");
    }
}
