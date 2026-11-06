//! Process-local text rasterization settings for the native renderer.

use objc2::runtime::AnyObject;
use objc2_foundation::{NSArgumentDomain, NSMutableDictionary, NSNumber, NSString, NSUserDefaults, ns_string};

/// Keep the bundled fonts' declared stroke weights instead of adding macOS's
/// color-dependent smoothing dilation, which makes pale labels look heavier.
/// Call before creating GPUI, whose text system caches this preference.
pub fn configure_text_rendering() {
    let defaults = NSUserDefaults::standardUserDefaults();
    // The argument domain is volatile: nothing is written to the app's or system's
    // persistent preferences. Preserve any other launch-time overrides in it.
    let domain = NSMutableDictionary::<NSString, AnyObject>::new();
    // SAFETY: Foundation owns this immutable domain-name constant for the process lifetime.
    let argument_domain = unsafe { NSArgumentDomain };
    domain.addEntriesFromDictionary(&defaults.volatileDomainForName(argument_domain));
    domain.insert(ns_string!("AppleFontSmoothing"), NSNumber::new_i32(0).as_ref());
    // SAFETY: the domain contains string keys and property-list values, copied from
    // the existing argument domain plus the integer smoothing override.
    unsafe { defaults.setVolatileDomain_forName(&domain, argument_domain) };
}
