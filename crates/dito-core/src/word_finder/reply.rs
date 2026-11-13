//! Reads the reply of another AI that was given [`OTHER_AI_PROMPT`] and the user's files. Dito never saw those
//! files, so it cannot check a term against them: the reply is checked for shape only, and the user chooses.

use std::collections::HashSet;

use serde_json::Value;

use super::{Candidate, is_term};

/// Copied for every UI language as it is: the scaffold is English whatever language the files are in (ADR-0012).
pub const OTHER_AI_PROMPT: &str = include_str!("other-ai-prompt.md");

/// More than any reply holds; a paste beyond it is not one.
const MAX_REPLY_BYTES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// In the order of the reply, without repeats and without the known words.
    pub candidates: Vec<Candidate>,
    /// Entries of the reply that are not terms.
    pub skipped: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyError {
    /// No JSON object in the text.
    Unreadable,
    /// JSON that is not what the prompt asks for.
    WrongShape,
}

/// Lenient about what surrounds the JSON object (a code fence, a sentence before or after it), strict about what is
/// in it.
pub fn read_reply(reply: &str, known: &[String]) -> Result<Reply, ReplyError> {
    if reply.len() > MAX_REPLY_BYTES {
        return Err(ReplyError::Unreadable);
    }
    let (start, end) = reply.find('{').zip(reply.rfind('}')).ok_or(ReplyError::Unreadable)?;
    let object: Value =
        serde_json::from_str(reply.get(start..=end).unwrap_or_default()).map_err(|_| ReplyError::Unreadable)?;
    let words = match (&object["dito_words"], &object["words"]) {
        (Value::Number(version), Value::Array(words)) if version.as_u64() == Some(1) => words,
        _ => return Err(ReplyError::WrongShape),
    };
    let mut seen: HashSet<String> = known.iter().map(|word| word.to_lowercase()).collect();
    let mut candidates = Vec::new();
    let mut skipped = 0;
    for entry in words {
        match entry["term"].as_str().map(str::trim).filter(|term| is_term(term)) {
            Some(term) if seen.insert(term.to_lowercase()) => {
                candidates
                    .push(Candidate { term: term.to_owned(), likely: entry["likely"].as_bool().unwrap_or(false) });
            }
            Some(_) => {}
            None => skipped += 1,
        }
    }
    Ok(Reply { candidates, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_in_a_code_fence_with_words_around_it_is_read() {
        let reply = "Here you go:\n```json\n{\"dito_words\": 1, \"words\": [\
            {\"term\": \"Tokio\", \"likely\": true}, {\"term\": \" 検証環境 \"}, {\"term\": \"tokio\", \"likely\": true},\
            {\"term\": \"Bedrock\", \"likely\": true}, {\"term\": 7}, \"Northwind\",\
            {\"term\": \"これは用語ではなく、一つの文です。\"}, {\"term\": \"x\"}]}\n```\nAnything else?";
        let read = read_reply(reply, &["bedrock".to_owned()]).expect("a readable reply");
        assert_eq!(
            read.candidates,
            [Candidate { term: "Tokio".into(), likely: true }, Candidate { term: "検証環境".into(), likely: false }]
        );
        assert_eq!(read.skipped, 4, "a number, a bare string, a sentence and a single letter are not terms");
    }

    #[test]
    fn text_without_json_is_unreadable_and_other_json_is_the_wrong_shape() {
        assert_eq!(read_reply("Sure! Tokio, Northwind and Lakehouse.", &[]), Err(ReplyError::Unreadable));
        assert_eq!(read_reply("{\"dito_words\": 1, \"words\": [", &[]), Err(ReplyError::Unreadable));
        assert_eq!(read_reply("{\"terms\": [\"Tokio\"]}", &[]), Err(ReplyError::WrongShape));
        assert_eq!(read_reply("{\"dito_words\": 2, \"words\": []}", &[]), Err(ReplyError::WrongShape));
        assert_eq!(read_reply("{\"dito_words\": 1, \"words\": {}}", &[]), Err(ReplyError::WrongShape));
        assert_eq!(read_reply("{\"dito_words\": 1, \"words\": []}", &[]), Ok(Reply { candidates: vec![], skipped: 0 }));
    }
}
