//! How an input device is connected. Opening some devices has side effects outside this app
//! (a nearby iPhone is woken as a Continuity microphone, a Bluetooth headset drops to its
//! low-quality call profile), so automatic microphone picking only opens devices that are
//! wired to this Mac. Everything else stays available for manual selection.

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::NonNull;

use objc2_core_audio::{
    AudioDeviceID, AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
    kAudioDevicePropertyTransportType, kAudioDeviceTransportTypeBuiltIn, kAudioDeviceTransportTypeFireWire,
    kAudioDeviceTransportTypePCI, kAudioDeviceTransportTypeThunderbolt, kAudioDeviceTransportTypeUSB,
    kAudioHardwareNoError, kAudioHardwarePropertyTranslateUIDToDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
};
use objc2_core_foundation::CFString;

/// Whether automatic picking may open a device with this transport type. Unknown (`None`) is refused:
/// a device whose connection cannot be determined is not worth a side effect.
pub(crate) fn allows_auto_probe(transport: Option<u32>) -> bool {
    const WIRED: [u32; 5] = [
        kAudioDeviceTransportTypeBuiltIn,
        kAudioDeviceTransportTypeUSB,
        kAudioDeviceTransportTypeThunderbolt,
        kAudioDeviceTransportTypeFireWire,
        kAudioDeviceTransportTypePCI,
    ];
    transport.is_some_and(|t| WIRED.contains(&t))
}

fn device_for_uid(uid: &str) -> Option<AudioDeviceID> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyTranslateUIDToDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let uid = CFString::from_str(uid);
    let qualifier: *const CFString = &*uid;
    let mut device: AudioDeviceID = 0;
    let mut size = size_of::<AudioDeviceID>() as u32;
    // SAFETY: the qualifier is a live CFStringRef of the size passed, and `device` is the
    // AudioDeviceID this property writes; the status is checked before the value is used.
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&address),
            size_of::<*const CFString>() as u32,
            (&qualifier as *const *const CFString).cast::<c_void>(),
            NonNull::from(&mut size),
            NonNull::from(&mut device).cast(),
        )
    };
    (status == kAudioHardwareNoError && device != 0).then_some(device)
}

/// CoreAudio transport type of the device with this UID (`kAudioDeviceTransportType…`).
pub(crate) fn transport_of(uid: &str) -> Option<u32> {
    let device = device_for_uid(uid)?;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyTransportType,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut transport = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: this property writes one u32; the status is checked before the value is used.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut transport).cast(),
        )
    };
    (status == kAudioHardwareNoError).then_some(transport)
}

#[cfg(test)]
mod tests {
    use objc2_core_audio::{
        kAudioDeviceTransportTypeAggregate, kAudioDeviceTransportTypeAirPlay, kAudioDeviceTransportTypeBluetooth,
        kAudioDeviceTransportTypeBluetoothLE, kAudioDeviceTransportTypeContinuityCaptureWired,
        kAudioDeviceTransportTypeContinuityCaptureWireless, kAudioDeviceTransportTypeUnknown,
        kAudioDeviceTransportTypeVirtual,
    };

    use super::*;

    #[test]
    fn only_wired_transports_are_probed() {
        for t in [
            kAudioDeviceTransportTypeBuiltIn,
            kAudioDeviceTransportTypeUSB,
            kAudioDeviceTransportTypeThunderbolt,
            kAudioDeviceTransportTypeFireWire,
            kAudioDeviceTransportTypePCI,
        ] {
            assert!(allows_auto_probe(Some(t)), "{t:#x}");
        }
        for t in [
            kAudioDeviceTransportTypeBluetooth,
            kAudioDeviceTransportTypeBluetoothLE,
            kAudioDeviceTransportTypeContinuityCaptureWired,
            kAudioDeviceTransportTypeContinuityCaptureWireless,
            kAudioDeviceTransportTypeUnknown,
            kAudioDeviceTransportTypeVirtual,
            kAudioDeviceTransportTypeAggregate,
            kAudioDeviceTransportTypeAirPlay,
        ] {
            assert!(!allows_auto_probe(Some(t)), "{t:#x}");
        }
        assert!(!allows_auto_probe(None));
    }
}
