use super::Lang;

pub fn sample_utterance(lang: Lang) -> &'static str {
    match lang.prompt_language() {
        Lang::Ja => "えーと 明日の3時に会議、いや4時に会議があるので、資料を、あの、事前に共有しておいてください",
        Lang::Zh => "嗯，明天三点开会，不对，是四点开会，所以请把资料，那个，提前共享一下",
        _ => "Um, we have a meeting at 3 tomorrow, no, make that 4, so please, uh, share the materials beforehand",
    }
}
