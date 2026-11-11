//! Ignored by default because it touches the user's real login keychain:
//!   cargo test -p dito-platform --test keychain -- --ignored

use dito_core::secrets::SecretStore;
use dito_platform::KeychainSecrets;

#[test]
#[ignore = "touches the real login keychain"]
fn set_get_overwrite_delete() {
    let s = KeychainSecrets::with_service(&format!("com.fang2hou.dito.test.{}", std::process::id()));
    assert_eq!(s.get("asr.azure"), None);
    s.set("asr.azure", "key-1").unwrap();
    assert_eq!(s.get("asr.azure").as_deref(), Some("key-1"));
    s.set("asr.azure", "日本語キー").unwrap();
    assert_eq!(s.get("asr.azure").as_deref(), Some("日本語キー"));
    s.delete("asr.azure").unwrap();
    assert_eq!(s.get("asr.azure"), None);
    s.delete("asr.azure").unwrap();
}
