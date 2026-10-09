//! macOS: receive files opened from Finder (double-click, "Open With", dropping a file on the
//! Dock icon).
//!
//! macOS delivers these as an "open documents" Apple Event rather than as command-line
//! arguments, and winit's application delegate doesn't handle that event. So we install our
//! own Apple Event handler. AppKit's documented place to do that is
//! `applicationWillFinishLaunching`: after AppKit installs its default handlers, but before the
//! event for the file that launched the app is dispatched. We observe that notification rather
//! than replacing winit's delegate.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use eframe::egui;
use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSObject, ns_string};

/// Apple Event codes are four-character codes packed into a `u32`.
const fn four_cc(code: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*code)
}
const CORE_EVENT_CLASS: u32 = four_cc(b"aevt");
const OPEN_DOCUMENTS: u32 = four_cc(b"odoc");
const DIRECT_OBJECT: u32 = four_cc(b"----");

/// Files Finder asked us to open that the app hasn't picked up yet.
static OPENED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
/// Used to wake the UI when a file arrives while the app is idle.
static CONTEXT: OnceLock<egui::Context> = OnceLock::new();

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this class has no `Drop` impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "VeraViewOpenDocumentsHandler"]
    struct OpenDocumentsHandler;

    impl OpenDocumentsHandler {
        #[unsafe(method(applicationWillFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            let manager = NSAppleEventManager::sharedAppleEventManager();
            // SAFETY: the selector names a method of `self` with the signature the Apple Event
            // manager expects, and `self` is never deallocated (see `install`).
            unsafe {
                let _: () = msg_send![
                    &*manager,
                    setEventHandler: self,
                    andSelector: sel!(handleOpenDocuments:withReplyEvent:),
                    forEventClass: CORE_EVENT_CLASS,
                    andEventID: OPEN_DOCUMENTS
                ];
            }
        }

        #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
        fn handle_open_documents(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            // SAFETY: `paramDescriptorForKeyword:` takes an AEKeyword (a u32) and returns a
            // descriptor or nil.
            let files: Option<Retained<NSAppleEventDescriptor>> =
                unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
            let Some(files) = files else { return };

            // Usually a list of file URLs; a single file may arrive as a plain descriptor.
            let mut descriptors = Vec::new();
            let count = files.numberOfItems();
            if count > 0 {
                descriptors.extend((1..=count).filter_map(|i| files.descriptorAtIndex(i)));
            } else {
                descriptors.push(files);
            }
            let paths: Vec<PathBuf> = descriptors
                .iter()
                .filter_map(|d| d.fileURLValue())
                .filter_map(|url| url.path())
                .map(|path| PathBuf::from(path.to_string()))
                .collect();
            if paths.is_empty() {
                return;
            }
            OPENED.lock().unwrap_or_else(|e| e.into_inner()).extend(paths);
            if let Some(ctx) = CONTEXT.get() {
                ctx.request_repaint();
            }
        }
    }
);

/// Start listening for Finder's "open documents" requests. Call on the main thread before the
/// event loop starts.
pub fn install() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let handler: Retained<OpenDocumentsHandler> = unsafe { msg_send![OpenDocumentsHandler::alloc(mtm), init] };
    // SAFETY: the selector is a method of the observer taking one NSNotification argument.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &handler,
            sel!(applicationWillFinishLaunching:),
            Some(ns_string!("NSApplicationWillFinishLaunchingNotification")),
            None,
        );
    }
    // Neither the notification center nor the Apple Event manager retains its target, and
    // the handler is needed for the whole life of the app.
    std::mem::forget(handler);
}

/// Let the handler wake the UI when files arrive.
pub fn set_context(ctx: &egui::Context) {
    let _ = CONTEXT.set(ctx.clone());
}

/// Files opened from Finder since the last call.
pub fn take_opened_files() -> Vec<PathBuf> {
    std::mem::take(&mut *OPENED.lock().unwrap_or_else(|e| e.into_inner()))
}
