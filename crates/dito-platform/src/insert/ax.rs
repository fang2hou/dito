use std::ffi::c_void;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};

type AXUIElementRef = *const c_void;

const AX_SUCCESS: i32 = 0;
/// `kAXValueTypeCFRange` from AXValue.h: the wrapper type of `AXSelectedTextRange` values.
const AX_VALUE_TYPE_CF_RANGE: u32 = 4;

#[repr(C)]
struct CFRange {
    location: isize,
    length: isize,
}

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    safe fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    safe fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: *mut CFTypeRef) -> i32;
    fn AXUIElementCopyAttributeNames(element: AXUIElementRef, names: *mut CFArrayRef) -> i32;
    fn AXUIElementSetAttributeValue(element: AXUIElementRef, attribute: CFStringRef, value: CFTypeRef) -> i32;
    safe fn AXUIElementGetTypeID() -> usize;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> i32;
    fn AXUIElementGetPid(element: AXUIElementRef, pid: *mut i32) -> i32;
    fn AXUIElementCopyParameterizedAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        parameter: CFTypeRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueCreate(value_type: u32, value: *const c_void) -> CFTypeRef;
    fn AXValueGetValue(value: CFTypeRef, the_type: u32, value: *mut c_void) -> bool;
}

/// A retained accessibility element, released when dropped.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AxElement(CFType);

impl AxElement {
    pub(super) fn system_wide() -> Option<Self> {
        Self::adopt(AXUIElementCreateSystemWide()).map(Self::bounded)
    }

    pub(super) fn application(pid: i32) -> Option<Self> {
        Self::adopt(AXUIElementCreateApplication(pid)).map(Self::bounded)
    }

    fn adopt(created: AXUIElementRef) -> Option<Self> {
        // SAFETY: the Create functions return a +1 retained element or null; ownership passes to the wrapper.
        (!created.is_null()).then(|| Self(unsafe { CFType::wrap_under_create_rule(created) }))
    }

    fn bounded(self) -> Self {
        // SAFETY: self owns a live AX element; the timeout is in seconds.
        unsafe { AXUIElementSetMessagingTimeout(self.raw(), 0.15) };
        self
    }

    fn raw(&self) -> AXUIElementRef {
        self.0.as_CFTypeRef()
    }

    pub(super) fn attribute(&self, name: &str) -> Option<CFType> {
        let attribute = CFString::new(name);
        let mut value: CFTypeRef = std::ptr::null();
        // SAFETY: `self` holds a live element, `attribute` is a valid CFString and `value` a valid out-pointer.
        let status = unsafe { AXUIElementCopyAttributeValue(self.raw(), attribute.as_concrete_TypeRef(), &mut value) };
        // SAFETY: on success the call returns a +1 retained object, which `wrap_under_create_rule` adopts.
        (status == AX_SUCCESS && !value.is_null()).then(|| unsafe { CFType::wrap_under_create_rule(value) })
    }

    pub(super) fn string(&self, name: &str) -> Option<String> {
        self.attribute(name)?.downcast::<CFString>().map(|s| s.to_string())
    }

    pub(super) fn text_for_range(&self, start: usize, length: usize) -> Option<String> {
        let range = CFRange { location: start.try_into().ok()?, length: length.try_into().ok()? };
        let attribute = CFString::new("AXStringForRange");
        // SAFETY: AXValueCreate copies this CFRange into a retained AXValue.
        let value = unsafe { AXValueCreate(AX_VALUE_TYPE_CF_RANGE, (&raw const range).cast()) };
        if value.is_null() {
            return None;
        }
        // SAFETY: the non-null Create result is owned by this scope.
        let value = unsafe { CFType::wrap_under_create_rule(value) };
        let mut result = std::ptr::null();
        // SAFETY: the element, attribute, and range are live; result is a valid out-pointer.
        let status = unsafe {
            AXUIElementCopyParameterizedAttributeValue(
                self.raw(),
                attribute.as_concrete_TypeRef(),
                value.as_CFTypeRef(),
                &mut result,
            )
        };
        if status != AX_SUCCESS || result.is_null() {
            return None;
        }
        // SAFETY: a successful Copy call returns an owned result.
        unsafe { CFType::wrap_under_create_rule(result) }.downcast::<CFString>().map(|text| text.to_string())
    }

    /// Whether the attribute is the boolean true (`AXFocused` and friends come back as CFBoolean).
    pub(super) fn flag(&self, name: &str) -> bool {
        self.attribute(name).is_some_and(|v| v.downcast::<CFBoolean>() == Some(CFBoolean::true_value()))
    }

    pub(super) fn element(&self, name: &str) -> Option<Self> {
        self.attribute(name).filter(|value| value.type_of() == AXUIElementGetTypeID()).map(Self)
    }

    pub(super) fn children(&self) -> Vec<Self> {
        let Some(array) = self.attribute("AXChildren").and_then(|v| v.downcast::<CFArray>()) else {
            return Vec::new();
        };
        // SAFETY: the array's items are live CF objects; the get rule retains each one for the wrapper.
        array
            .iter()
            .map(|child| unsafe { CFType::wrap_under_get_rule(*child) })
            .filter(|child| child.type_of() == AXUIElementGetTypeID())
            .map(Self)
            .collect()
    }

    pub(super) fn selection(&self) -> Option<AxSelection> {
        self.attribute("AXSelectedTextRange").map(AxSelection)
    }

    pub(super) fn attribute_names(&self) -> Option<Vec<String>> {
        let mut names: CFArrayRef = std::ptr::null();
        // SAFETY: `self` holds a live element and `names` is a valid out-pointer.
        let status = unsafe { AXUIElementCopyAttributeNames(self.raw(), &mut names) };
        if status != AX_SUCCESS || names.is_null() {
            return None;
        }
        // SAFETY: on success the call returns a +1 retained CFArray of CFStrings, which is adopted here.
        let names = unsafe { CFArray::<CFString>::wrap_under_create_rule(names) };
        Some(names.iter().map(|name| name.to_string()).collect())
    }

    pub(super) fn set_true(&self, name: &str) -> bool {
        let attribute = CFString::new(name);
        // SAFETY: `self` holds a live element and both the attribute name and the CFBoolean value are valid.
        let status = unsafe {
            AXUIElementSetAttributeValue(
                self.raw(),
                attribute.as_concrete_TypeRef(),
                CFBoolean::true_value().as_CFTypeRef(),
            )
        };
        status == AX_SUCCESS
    }

    pub(super) fn pid(&self) -> i32 {
        let mut pid = 0;
        // SAFETY: `self` holds a live element and `pid` is a valid out-pointer.
        unsafe { AXUIElementGetPid(self.raw(), &mut pid) };
        pid
    }
}

/// The `AXSelectedTextRange` value of a text element.
pub(super) struct AxSelection(CFType);

impl AxSelection {
    /// UTF-16 offset where the selection starts, which is the caret when nothing is selected.
    pub(super) fn location(&self) -> Option<usize> {
        let mut range = CFRange { location: 0, length: 0 };
        // SAFETY: the value is the AXValue returned for AXSelectedTextRange and `range` is a valid CFRange
        // out-buffer; the call writes through it only when the value's type matches.
        let ok = unsafe { AXValueGetValue(self.0.as_CFTypeRef(), AX_VALUE_TYPE_CF_RANGE, (&raw mut range).cast()) };
        ok.then(|| usize::try_from(range.location).ok()).flatten()
    }
}
