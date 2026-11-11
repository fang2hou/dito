use std::collections::BTreeMap;

use serde_json::Value;

pub(super) fn mapped_language(config: &Value, language: Option<&str>) -> Option<String> {
    let language = language.unwrap_or("auto").to_ascii_lowercase();
    let map = config["language_map"].as_object()?;
    let get = |key: &str| map.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v);
    let value = if language == "auto" {
        get("auto")
    } else {
        get(&language).or_else(|| get(language.split('-').next().unwrap_or(&language))).or_else(|| get("other"))
    };
    value.and_then(Value::as_str).map(str::to_owned)
}

/// A value that is exactly `{{language}}` is dropped, together with its key, when there is no language to send.
pub(super) fn expand(template: &Value, vars: &BTreeMap<&str, String>, language: Option<&str>) -> Option<Value> {
    Some(match template {
        Value::String(s) if s == "{{language}}" => Value::String(language?.to_owned()),
        Value::String(s) => Value::String(substitute(s, vars)),
        Value::Object(o) => {
            Value::Object(o.iter().filter_map(|(k, v)| Some((k.clone(), expand(v, vars, language)?))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().filter_map(|v| expand(v, vars, language)).collect()),
        _ => template.clone(),
    })
}

/// Substituted values are never scanned again, so a secret containing `{{model}}` stays literal.
fn substitute(text: &str, vars: &BTreeMap<&str, String>) -> String {
    let mut output = String::new();
    let mut remaining = text;
    while let Some((prefix, tail)) = remaining.split_once("{{") {
        output.push_str(prefix);
        let Some((key, next)) = tail.split_once("}}") else {
            output.push_str("{{");
            output.push_str(tail);
            return output;
        };
        match vars.get(key) {
            Some(value) => output.push_str(value),
            None => {
                output.push_str("{{");
                output.push_str(key);
                output.push_str("}}");
            }
        }
        remaining = next;
    }
    output.push_str(remaining);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_values_are_not_interpreted_as_more_placeholders() {
        let vars = BTreeMap::from([("api_key", "{{model}}".to_owned()), ("model", "actual-model".to_owned())]);
        assert_eq!(
            expand(&Value::String("Bearer {{api_key}}".into()), &vars, None),
            Some(Value::String("Bearer {{model}}".into()))
        );
    }

    #[test]
    fn language_fallback_and_auto() {
        let v = serde_json::json!({"language_map":{"ja":"JP","other":"EN","auto":null}});
        assert_eq!(mapped_language(&v, Some("ja-JP")), Some("JP".into()));
        assert_eq!(mapped_language(&v, Some("fr")), Some("EN".into()));
        assert_eq!(mapped_language(&v, None), None);
    }

    #[test]
    fn omit_language_without_altering_explicit_null() {
        let v = serde_json::json!({"language":"{{language}}","keep":null});
        assert_eq!(expand(&v, &BTreeMap::new(), None), Some(serde_json::json!({"keep":null})));
    }
}
