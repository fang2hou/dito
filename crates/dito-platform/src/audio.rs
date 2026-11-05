//! Muting the default output device while dictation records, via CoreAudio.

use std::ffi::c_void;

use dito_core::controller::{MusicMute, MuteSession};

type AudioObjectID = u32;
type OSStatus = i32;
type Boolean = u8;

/// Mirrors CoreAudio's `AudioObjectPropertyAddress`; field names follow the C header.
#[repr(C)]
#[allow(non_snake_case)]
struct AudioObjectPropertyAddress {
    mSelector: u32,
    mScope: u32,
    mElement: u32,
}

const NO_ERROR: OSStatus = 0;

// Four-character codes from CoreAudio's property headers, byte-swapped into host order.
const SYSTEM_OBJECT: AudioObjectID = 1;
const SEL_DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
const SEL_IS_RUNNING_SOMEWHERE: u32 = u32::from_be_bytes(*b"gone");
const SEL_MUTE: u32 = u32::from_be_bytes(*b"mute");
const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
const SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
const ELEMENT_MASTER: u32 = 0;

const IS_RUNNING: u32 = 1;
const MUTED: u32 = 1;
const UNMUTED: u32 = 0;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectHasProperty(id: AudioObjectID, address: *const AudioObjectPropertyAddress) -> Boolean;
    fn AudioObjectGetPropertyData(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: *mut u32,
        data: *mut c_void,
    ) -> OSStatus;
    fn AudioObjectSetPropertyData(
        id: AudioObjectID,
        address: *const AudioObjectPropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> OSStatus;
}

fn address(selector: u32, scope: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: scope, mElement: ELEMENT_MASTER }
}

fn get_u32(id: AudioObjectID, selector: u32, scope: u32) -> Option<u32> {
    let address = address(selector, scope);
    if unsafe { AudioObjectHasProperty(id, &address) } == 0 {
        return None;
    }
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(id, &address, 0, std::ptr::null(), &mut size, &mut value as *mut u32 as *mut c_void)
    };
    (status == NO_ERROR && size == size_of::<u32>() as u32).then_some(value)
}

fn set_u32(id: AudioObjectID, selector: u32, scope: u32, value: u32) -> bool {
    let address = address(selector, scope);
    if unsafe { AudioObjectHasProperty(id, &address) } == 0 {
        return false;
    }
    let status = unsafe {
        AudioObjectSetPropertyData(
            id,
            &address,
            0,
            std::ptr::null(),
            size_of::<u32>() as u32,
            &value as *const u32 as *const c_void,
        )
    };
    status == NO_ERROR
}

fn default_output_device() -> Option<AudioObjectID> {
    let mut device = 0u32;
    let mut size = size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            &address(SEL_DEFAULT_OUTPUT, SCOPE_GLOBAL),
            0,
            std::ptr::null(),
            &mut size,
            &mut device as *mut u32 as *mut c_void,
        )
    };
    (status == NO_ERROR && device != 0).then_some(device)
}

/// Mute is per-channel; read the master element from the output scope, falling back to global.
fn read_mute(device: AudioObjectID) -> Option<(u32, u32)> {
    [SCOPE_OUTPUT, SCOPE_GLOBAL].into_iter().find_map(|scope| get_u32(device, SEL_MUTE, scope).map(|v| (v, scope)))
}

/// Only silence a device that is actually audible: something must be playing and it must not be
/// muted already. Missing readings mean the device is not understood well enough to touch.
fn should_mute(running: Option<u32>, mute: Option<u32>) -> bool {
    running == Some(IS_RUNNING) && mute == Some(UNMUTED)
}

/// Is anything playing through the default output device?
pub fn default_output_is_active() -> bool {
    default_output_device().is_some_and(|d| get_u32(d, SEL_IS_RUNNING_SOMEWHERE, SCOPE_GLOBAL) == Some(IS_RUNNING))
}

/// Restores the previous mute state of the output device when dropped.
pub struct MusicMuteGuard {
    device: Option<(AudioObjectID, u32)>,
    was_muted: bool,
}

impl MusicMuteGuard {
    fn none() -> Self {
        Self { device: None, was_muted: true }
    }
}

impl Drop for MusicMuteGuard {
    fn drop(&mut self) {
        let Some((device, scope)) = self.device else { return };
        if !set_u32(device, SEL_MUTE, scope, self.was_muted as u32) {
            tracing::warn!("could not restore the output device's mute state");
        }
    }
}

/// Mute the default output while something is playing there; a no-op guard otherwise.
fn mute_default_output() -> MusicMuteGuard {
    let Some(device) = default_output_device() else {
        return MusicMuteGuard::none();
    };
    let running = get_u32(device, SEL_IS_RUNNING_SOMEWHERE, SCOPE_GLOBAL);
    let Some((mute, scope)) = read_mute(device) else {
        return MusicMuteGuard::none();
    };
    if !should_mute(running, Some(mute)) || !set_u32(device, SEL_MUTE, scope, MUTED) {
        return MusicMuteGuard::none();
    }
    MusicMuteGuard { device: Some((device, scope)), was_muted: false }
}

impl MuteSession for MusicMuteGuard {}

/// [`MusicMute`] backed by CoreAudio.
pub struct MacMusicMute;

impl MusicMute for MacMusicMute {
    fn mute_if_active(&self) -> Box<dyn MuteSession> {
        Box::new(mute_default_output())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutes_only_when_audible_and_unmuted() {
        assert!(should_mute(Some(IS_RUNNING), Some(UNMUTED)));
        assert!(!should_mute(Some(IS_RUNNING), Some(MUTED)), "already muted: nothing to do");
        assert!(!should_mute(Some(UNMUTED), Some(UNMUTED)), "silent device: no interference");
        assert!(!should_mute(None, Some(UNMUTED)), "unreadable state: leave the device alone");
        assert!(!should_mute(Some(IS_RUNNING), None), "no mute control: leave the device alone");
    }
}
