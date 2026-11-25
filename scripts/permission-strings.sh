#!/usr/bin/env bash
# The system permission prompts read InfoPlist.strings, not the app's UI strings, so every UI language needs a copy.
# Sourced by bundle.sh and dev-runner.sh: write_permission_strings <Contents/Resources directory>.

write_permission_strings() {
  local resources=$1 lproj microphone system_audio
  for lproj in ja en zh-Hans zh-Hant ko mn fr; do
    case "$lproj" in
      ja)
        microphone='音声入力のためにマイクを使用します。'
        system_audio='会議の書き起こしのためにシステム音声を録音します。'
        ;;
      en)
        microphone='Dito uses the microphone for voice input.'
        system_audio='Dito records system audio to transcribe meetings.'
        ;;
      zh-Hans)
        microphone='Dito 使用麦克风进行语音输入。'
        system_audio='Dito 录制系统音频以转写会议。'
        ;;
      zh-Hant)
        microphone='Dito 使用麥克風進行語音輸入。'
        system_audio='Dito 錄製系統音訊以製作會議逐字稿。'
        ;;
      ko)
        microphone='Dito는 음성 입력에 마이크를 사용합니다.'
        system_audio='Dito는 회의 기록을 위해 시스템 오디오를 녹음합니다.'
        ;;
      mn)
        microphone='Dito дуугаар оруулахад микрофон ашиглана.'
        system_audio='Dito хурлын тэмдэглэл хөтлөхийн тулд системийн аудиог бичнэ.'
        ;;
      fr)
        microphone='Dito utilise le micro pour la dictée.'
        system_audio="Dito enregistre l'audio du système pour transcrire les réunions."
        ;;
    esac
    mkdir -p "$resources/$lproj.lproj"
    printf 'NSMicrophoneUsageDescription = "%s";\nNSAudioCaptureUsageDescription = "%s";\n' \
      "$microphone" "$system_audio" >"$resources/$lproj.lproj/InfoPlist.strings"
  done
}
