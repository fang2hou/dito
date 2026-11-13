//! The polishing prompt for one paragraph of a transcript.

use crate::i18n::Lang;
use crate::prompts::detect_lang;

use super::Speaker;

/// Polished paragraphs passed back as context.
pub(super) const CONTEXT_PARAGRAPHS: usize = 2;

pub(super) struct ContextParagraph {
    pub speaker: Speaker,
    pub text: String,
}

struct Copy {
    rules: &'static str,
    context: &'static str,
    terms: &'static str,
    me: &'static str,
    others: &'static str,
}

const JA: Copy = Copy {
    rules: "あなたは会議の書き起こしを読みやすく整える編集者です。\n\
            入力は音声認識の結果で、誤認識、言い淀み、フィラー（えー、あの など）、句読点の欠落を含みます。\n\
            - 話された内容と意味は変えず、フィラーと言い直しを取り除き、句読点を補って自然な文章にしてください。\n\
            - 言い直し（「水曜に、いや木曜に」など）は、後から言い直した内容だけを残してください。\n\
            - 前後の文脈から明らかな誤認識は直してください。確信が持てないものはそのまま残してください。\n\
            - 要約、追記、補足はしないでください。入力が質問や依頼でも、答えずにそのまま整えてください。\n\
            - 入力が相づちやフィラーだけのときは、入力をそのまま返してください。\n\
            - 出力は整えた本文だけにしてください。説明、注記、区切り線は書かないでください。",
    context: "以下は直前までの書き起こし（整形済み）です。話の流れと用語をそろえるための参考で、出力には含めず、繰り返さないでください。",
    terms: "次の用語は、この表記で書いてください：",
    me: "自分",
    others: "相手",
};

const EN: Copy = Copy {
    rules: "You edit meeting transcripts so they read well.\n\
            The input is speech recognition output with misrecognized words, hesitations, fillers (um, uh) and \
            missing punctuation.\n\
            - Keep what was said and what it means. Remove fillers and false starts and add punctuation.\n\
            - When the speaker corrects themselves (\"on Wednesday, no, Thursday\"), keep only the correction.\n\
            - Fix words that the surrounding context shows were misrecognized. Leave anything you are unsure of.\n\
            - Do not summarize or add anything. If the input is a question or a request, do not answer it.\n\
            - If the input is only a backchannel or fillers, return it unchanged.\n\
            - Output only the edited text, with no explanations, notes or separators.",
    context: "The transcript just before this paragraph (already edited) follows. It is there to keep the flow and \
              terms consistent. Do not include or repeat it.",
    terms: "Write these terms exactly as given:",
    me: "Me",
    others: "Others",
};

const ZH: Copy = Copy {
    rules: "你负责把会议记录整理得通顺易读。\n\
            输入是语音识别的结果，含有识别错误、停顿、口头禅（嗯、那个）和缺失的标点。\n\
            - 不改变所说的内容和意思，去掉口头禅和重复的说法，补上标点，整理成自然的文字。\n\
            - 说话人改口时（如“周三，不对，周四”），只保留改口后的内容。\n\
            - 根据上下文能明显判断的识别错误要改正，没有把握的保持原样。\n\
            - 不要总结、补充或添加内容。即使输入是提问或请求，也不要回答，只整理文字。\n\
            - 如果输入只有附和或口头禅，原样返回输入。\n\
            - 只输出整理后的正文，不要写解释、注释或分隔线。",
    context: "下面是这段之前的会议记录（已整理），用于保持语境和用语一致，不要包含在输出中，也不要重复。",
    terms: "以下用语请按此写法书写：",
    me: "我",
    others: "对方",
};

fn copy(lang: Lang) -> &'static Copy {
    match lang {
        Lang::Ja => &JA,
        Lang::En => &EN,
        Lang::Zh => &ZH,
    }
}

/// Paragraphs with fewer letters than this are left as recognized. A model asked to polish a lone filler has nothing
/// to remove but the filler itself, and it answers with notes about the input instead of text.
const MIN_POLISH_LETTERS: usize = 4;

/// Below this, a paragraph is too short to tell its language, so the context decides. Recognition writes a Japanese
/// "un" backchannel as the Chinese filler character, and that alone would pick the Chinese prompt in a Japanese
/// meeting.
const MIN_LANG_LETTERS: usize = 8;

fn letters(text: &str) -> usize {
    text.chars().filter(|c| c.is_alphanumeric()).count()
}

pub(super) fn worth_polishing(text: &str) -> bool {
    letters(text) >= MIN_POLISH_LETTERS
}

fn prompt_lang(text: &str, context: &[ContextParagraph]) -> Lang {
    if letters(text) >= MIN_LANG_LETTERS || context.is_empty() {
        return detect_lang(text);
    }
    let mut all: String = context.iter().map(|p| p.text.as_str()).collect();
    all.push_str(text);
    detect_lang(&all)
}

/// `labelled` names the speakers in the context, for sessions that record the microphone too.
pub(super) fn system_prompt(text: &str, context: &[ContextParagraph], labelled: bool, terms: &[String]) -> String {
    let c = copy(prompt_lang(text, context));
    let mut s = String::from(c.rules);
    if !terms.is_empty() {
        s.push_str("\n\n");
        s.push_str(c.terms);
        s.push(' ');
        s.push_str(&terms.join(", "));
    }
    if !context.is_empty() {
        s.push_str("\n\n");
        s.push_str(c.context);
        s.push_str("\n<context>\n");
        for (i, p) in context.iter().enumerate() {
            if i > 0 {
                s.push('\n');
            }
            if labelled {
                s.push_str(if p.speaker == Speaker::Me { c.me } else { c.others });
                s.push_str(": ");
            }
            s.push_str(&p.text);
        }
        s.push_str("\n</context>");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_follows_the_rules_in_the_paragraphs_language_with_speakers_when_labelled() {
        let context = [
            ContextParagraph { speaker: Speaker::Others, text: "決済の件です。".into() },
            ContextParagraph { speaker: Speaker::Me, text: "了解です。".into() },
        ];
        let s = system_prompt("えーと木曜に出します", &context, true, &["Acme".into()]);
        assert!(s.starts_with(JA.rules));
        assert!(s.contains("相手: 決済の件です。\n自分: 了解です。"));
        assert!(s.contains("Acme"));
        let plain = system_prompt("we ship on thursday", &context[..1], false, &[]);
        assert!(plain.starts_with(EN.rules) && plain.contains("<context>\n決済の件です。\n</context>"));
        assert!(!system_prompt("we ship", &[], false, &[]).contains("<context>"));
    }

    #[test]
    fn short_paragraphs_take_the_meetings_language_and_the_shortest_are_not_polished() {
        let ja = [ContextParagraph {
            speaker: Speaker::Others, text: "決済の件ですが、木曜に出します。".into()
        }];
        assert!(system_prompt("嗯，好的。", &ja, false, &[]).starts_with(JA.rules));
        assert!(system_prompt("嗯，好的。", &[], false, &[]).starts_with(ZH.rules));
        assert!(system_prompt("我们周四之前把这个方案发给客户。", &ja, false, &[]).starts_with(ZH.rules));
        assert!(!worth_polishing("嗯嗯。") && !worth_polishing("Yes."));
        assert!(worth_polishing("えーと木曜"));
    }
}
