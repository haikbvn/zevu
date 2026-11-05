use std::ffi::c_void;
use std::sync::Arc;

use accessibility::util::ax_call;
use accessibility_sys::{
    AXError,
    AXObserverAddNotification,
    AXObserverCallback,
    AXObserverCreate,
    AXObserverGetRunLoopSource,
    AXObserverRef,
    AXUIElementSetMessagingTimeout,
    kAXErrorSuccess,
    pid_t,
};
use core_foundation::base::{
    CFRelease,
    TCFType,
};
use core_foundation::runloop::{
    CFRunLoopAddSource,
    CFRunLoopGetMain,
    CFRunLoopRemoveSource,
    kCFRunLoopDefaultMode,
};
use core_foundation::string::{
    CFString,
    CFStringRef,
};

use super::UIElement;

pub struct AXObserver<T> {
    inner: AXObserverRef,
    pub(super) application: UIElement,
    callback_data: Arc<T>,
}

// SAFETY: AX references support cross-thread IPC; callback data must also be thread safe.
unsafe impl<T: Send + Sync> Send for AXObserver<T> {}
unsafe impl<T: Send + Sync> Sync for AXObserver<T> {}

impl<T> AXObserver<T> {
    pub unsafe fn create(
        pid: pid_t,
        application: UIElement,
        data: T,
        callback: AXObserverCallback,
    ) -> Result<Self, AXError> {
        let observer = ax_call(|x: *mut AXObserverRef| AXObserverCreate(pid, callback, x))?;

        CFRunLoopAddSource(
            CFRunLoopGetMain(),
            AXObserverGetRunLoopSource(observer),
            kCFRunLoopDefaultMode,
        );

        Ok(Self {
            inner: observer,
            application,
            callback_data: Arc::new(data),
        })
    }

    pub unsafe fn subscribe(&self, ax_event: &str) -> Result<(), AXError> {
        // This API returns a status only; ax_call requires an initialized output value.
        let callback_data = Arc::as_ptr(&self.callback_data);
        let status = AXObserverAddNotification(
            self.inner,
            self.application.get_ref(),
            CFString::from(ax_event).as_CFTypeRef() as CFStringRef,
            callback_data as *const _ as *mut c_void,
        );
        if status == kAXErrorSuccess {
            Ok(())
        } else {
            Err(status)
        }
    }

    pub unsafe fn set_registration_timeout(&self) -> Result<(), AXError> {
        // A cold AX server can take several seconds to answer its first request.
        // Registration runs off the UI thread; ordinary UI queries retain their 250 ms limit.
        let status = AXUIElementSetMessagingTimeout(self.application.get_ref(), 15.0);
        if status == kAXErrorSuccess {
            Ok(())
        } else {
            Err(status)
        }
    }
}

impl<T> Drop for AXObserver<T> {
    fn drop(&mut self) {
        unsafe {
            CFRunLoopRemoveSource(
                CFRunLoopGetMain(),
                AXObserverGetRunLoopSource(self.inner),
                kCFRunLoopDefaultMode,
            );
            // Stop notifications before callback_data is dropped.
            CFRelease(self.inner.cast());
        }
    }
}
