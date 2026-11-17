use std::path::{Path, PathBuf};
use std::ptr::{self, NonNull};

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_av_foundation::{AVPlayer, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput};
use objc2_core_foundation::CFRetained;
use objc2_core_media::CMTime;
use objc2_core_video::CVPixelBuffer;
use objc2_foundation::{NSBundle, NSDictionary, NSNumber, NSString, NSURL, ns_string};

/// Two-plane 8-bit YCbCr with full range, the layout the app's renderer draws as a surface.
const PIXEL_FORMAT_420F: i32 = 0x3432_3066;

/// The first-run intro film, decoded by AVFoundation. It only decodes: the app paints every frame itself, so the
/// film shares one scene with the pages around it and there is nothing to swap when it ends.
pub struct IntroVideo {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    rate: f32,
}

impl IntroVideo {
    /// `None` when the file cannot be opened or off the main thread. Playback starts at once.
    pub fn open(path: &Path) -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let path = NSString::from_str(path.to_str()?);
        let url = NSURL::fileURLWithPath(&path);
        // SAFETY: plain AVFoundation setup with owned objects; the attributes are CoreVideo's documented keys.
        unsafe {
            let item = AVPlayerItem::playerItemWithURL(&url, mtm);
            let output = AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
                AVPlayerItemVideoOutput::alloc(),
                Some(&pixel_buffer_attributes()),
            );
            item.addOutput(&output);
            let player = AVPlayer::playerWithPlayerItem(Some(&item), mtm);
            player.play();
            Some(Self { player, item, output, rate: 1.0 })
        }
    }

    /// Playback speed: 1.0 is normal. Ignored once the film has ended.
    pub fn set_rate(&mut self, rate: f32) {
        if (self.rate - rate).abs() < f32::EPSILON || self.finished() {
            return;
        }
        self.rate = rate;
        // SAFETY: the player is alive and owned by `self`.
        unsafe { self.player.setRate(rate) };
    }

    /// The newest decoded frame, or `None` when there is no frame newer than the last one returned.
    pub fn next_frame(&self) -> Option<CFRetained<CVPixelBuffer>> {
        // SAFETY: the output is attached to the item; the item time and the null out-pointer are valid.
        unsafe {
            let time = self.item.currentTime();
            if !self.output.hasNewPixelBufferForItemTime(time) {
                return None;
            }
            let buffer = self.output.copyPixelBufferForItemTime_itemTimeForDisplay(time, ptr::null_mut())?;
            // SAFETY: the copy method returns an owned (+1) CoreVideo object.
            Some(CFRetained::from_raw(NonNull::new(Retained::into_raw(buffer))?))
        }
    }

    /// 0.0 to 1.0; for fading the sound out when the film is skipped.
    pub fn set_volume(&self, volume: f32) {
        // SAFETY: the player is alive and owned by `self`.
        unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
    }

    pub fn duration_secs(&self) -> f64 {
        // SAFETY: the item is alive.
        seconds(unsafe { self.item.duration() })
    }

    pub fn position_secs(&self) -> f64 {
        // SAFETY: the item is alive.
        seconds(unsafe { self.item.currentTime() })
    }

    /// True at the end of the film, and also when it cannot be played.
    pub fn finished(&self) -> bool {
        // SAFETY: the item is alive.
        unsafe {
            if self.item.status() == AVPlayerItemStatus::Failed {
                return true;
            }
            let duration = seconds(self.item.duration());
            duration > 0.0 && seconds(self.item.currentTime()) >= duration - 0.02
        }
    }
}

impl Drop for IntroVideo {
    fn drop(&mut self) {
        // SAFETY: the player is alive; stopping first keeps the sound from outliving the object.
        unsafe { self.player.pause() };
    }
}

fn seconds(time: CMTime) -> f64 {
    if time.timescale == 0 { 0.0 } else { time.value as f64 / f64::from(time.timescale) }
}

fn pixel_buffer_attributes() -> Retained<NSDictionary<NSString, AnyObject>> {
    let format: Retained<AnyObject> = NSNumber::new_i32(PIXEL_FORMAT_420F).into();
    let metal: Retained<AnyObject> = NSNumber::new_bool(true).into();
    let surface: Retained<AnyObject> = NSDictionary::<NSString, AnyObject>::new().into();
    NSDictionary::from_retained_objects(
        &[ns_string!("PixelFormatType"), ns_string!("MetalCompatibility"), ns_string!("IOSurfaceProperties")],
        &[format, metal, surface],
    )
}

/// The `Resources` directory of the running app bundle; `None` for an unbundled run.
pub fn bundle_resources() -> Option<PathBuf> {
    let bundle = NSBundle::mainBundle();
    if !bundle.bundlePath().to_string().ends_with(".app") {
        return None;
    }
    bundle.resourcePath().map(|path| PathBuf::from(path.to_string()))
}
