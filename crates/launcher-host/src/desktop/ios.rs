//! Browser, Files-app and document-picker handoff through UIKit, since iOS lets an app start no
//! other program: the desktop openers and picker tools cannot run here.

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
};

use block2::Block;
use dispatch2::run_on_main;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyClass, AnyObject, Bool, NSObject, NSObjectProtocol},
};
use objc2_foundation::{NSDictionary, NSString, NSURL};

// `UTType`, which the document picker's content-type initializer takes.
#[link(name = "UniformTypeIdentifiers", kind = "framework")]
unsafe extern "C" {}

/// The broadest content type, so packs, mods and skins whose extensions no app registered stay
/// selectable; the caller checks extensions.
const ANY_ITEM: &str = "public.item";

/// Hands `url` to the system: Safari for web links, the Files app for `shareddocuments:` ones.
/// Returns whether UIKit accepted a well-formed URL; the open itself completes asynchronously.
pub(super) fn open_url(url: &str) -> bool {
    let url = url.to_owned();
    run_on_main(move |main_thread| {
        let Some(url) = NSURL::URLWithString(&NSString::from_str(&url)) else {
            tracing::warn!("iOS could not parse the link {url}");
            return false;
        };
        let Some(application) = shared_application(main_thread) else {
            return false;
        };
        let options = NSDictionary::<NSString, AnyObject>::new();
        let completion: Option<&Block<dyn Fn(Bool)>> = None;
        // SAFETY: `openURL:options:completionHandler:` takes an NSURL, an options dictionary and
        // an optional block, and must run on the main thread, which `run_on_main` guarantees.
        unsafe {
            let _: () = msg_send![
                &*application,
                openURL: &*url,
                options: &*options,
                completionHandler: completion
            ];
        }
        true
    })
}

/// Shows `dir`, which must be inside the app's `Documents`, in the Files app.
pub(super) fn show_folder(dir: &Path) -> bool {
    open_url(&super::files_app_url(dir))
}

/// Reads the general pasteboard's text, or `None` when it holds none.
pub fn pasteboard_text() -> Option<String> {
    run_on_main(|main_thread| {
        let pasteboard = general_pasteboard(main_thread)?;
        // SAFETY: `string` is a nullable NSString property of UIPasteboard, read on the main
        // thread.
        let text: Option<Retained<NSString>> = unsafe { msg_send![&*pasteboard, string] };
        text.map(|text| text.to_string())
    })
}

/// Replaces the general pasteboard's contents with `text`; returns whether a pasteboard existed.
pub fn set_pasteboard_text(text: String) -> bool {
    run_on_main(move |main_thread| {
        let Some(pasteboard) = general_pasteboard(main_thread) else {
            return false;
        };
        let text = NSString::from_str(&text);
        // SAFETY: `setString:` takes a nonnull NSString and runs on the main thread.
        unsafe {
            let _: () = msg_send![&*pasteboard, setString: &*text];
        }
        true
    })
}

fn general_pasteboard(_main_thread: MainThreadMarker) -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"UIPasteboard")?;
    // SAFETY: `generalPasteboard` is a class property returning the shared pasteboard.
    unsafe { msg_send![class, generalPasteboard] }
}

fn shared_application(_main_thread: MainThreadMarker) -> Option<Retained<AnyObject>> {
    let class = AnyClass::get(c"UIApplication")?;
    // SAFETY: `sharedApplication` is a class property returning the running application.
    unsafe { msg_send![class, sharedApplication] }
}

/// What the picker delegate reports: chosen copies, or an empty list on cancellation.
type Selection = Vec<PathBuf>;

define_class!(
    // SAFETY: NSObject has no subclass requirements; UIKit calls the delegate on the main thread.
    #[unsafe(super = NSObject)]
    #[name = "CinnabarDocumentPickerDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Option<mpsc::Sender<Selection>>>]
    struct PickerDelegate;

    // SAFETY: NSObjectProtocol adds no implementation requirements.
    unsafe impl NSObjectProtocol for PickerDelegate {}

    impl PickerDelegate {
        /// Receives the copies UIKit made in the app's temporary directory.
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick(&self, _picker: &AnyObject, urls: &AnyObject) {
            self.finish(file_paths(urls));
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn was_cancelled(&self, _picker: &AnyObject) {
            self.finish(Vec::new());
        }
    }
);

impl PickerDelegate {
    fn new(sender: mpsc::Sender<Selection>, main_thread: MainThreadMarker) -> Retained<Self> {
        let delegate = Self::alloc(main_thread).set_ivars(RefCell::new(Some(sender)));
        // SAFETY: NSObject's initializer accepts this initialized subclass allocation.
        unsafe { msg_send![super(delegate), init] }
    }

    /// Answers the waiting worker once. The delegate stays retained until the next picker
    /// replaces it, so it is never released while UIKit is still calling it.
    fn finish(&self, selection: Selection) {
        if let Some(sender) = self.ivars().borrow_mut().take() {
            let _ = sender.send(selection);
        }
    }

    /// Whether a worker still waits for this picker's answer.
    fn waiting(&self) -> bool {
        self.ivars().borrow().is_some()
    }
}

thread_local! {
    /// The latest picker's delegate; UIKit's `delegate` property does not retain it.
    static ACTIVE_DELEGATE: RefCell<Option<Retained<PickerDelegate>>> = const { RefCell::new(None) };
}

/// Shows UIKit's document picker over the game and blocks this worker until the user chooses or
/// cancels. Like Android's picker, it returns private copies that the caller's `PickedFile`s
/// delete after import, an empty list on cancellation, `None` when nothing could present the
/// picker, or an error when a document exceeds `max_bytes` or could not be moved.
pub(super) fn pick_documents(
    multiple: bool,
    max_bytes: Option<u64>,
) -> Result<Option<Vec<PathBuf>>, String> {
    if MainThreadMarker::new().is_some() {
        return Err("the document picker cannot wait on the main thread".into());
    }
    let (sender, receiver) = mpsc::channel();
    if !run_on_main(move |main_thread| present_picker(multiple, sender, main_thread)) {
        return Ok(None);
    }
    // A closed channel means the delegate went away without an answer, which reads as cancel.
    let picked = receiver.recv().unwrap_or_default();
    claim(picked, max_bytes).map(Some)
}

/// Presents the picker from the topmost view controller; returns whether it was shown.
fn present_picker(
    multiple: bool,
    sender: mpsc::Sender<Selection>,
    main_thread: MainThreadMarker,
) -> bool {
    if ACTIVE_DELEGATE.with_borrow(|active| {
        active.as_deref().is_some_and(PickerDelegate::waiting)
    }) {
        tracing::warn!("a document picker is already open");
        return false;
    }
    let Some(presenter) = top_view_controller(main_thread) else {
        tracing::warn!("no view controller can present the document picker");
        return false;
    };
    let (Some(picker_class), Some(type_class)) = (
        AnyClass::get(c"UIDocumentPickerViewController"),
        AnyClass::get(c"UTType"),
    ) else {
        return false;
    };
    // SAFETY: Every message below is documented UIKit/UniformTypeIdentifiers API with the
    // argument types given here, sent on the main thread.
    unsafe {
        let identifier = NSString::from_str(ANY_ITEM);
        let item: Option<Retained<AnyObject>> =
            msg_send![type_class, typeWithIdentifier: &*identifier];
        let Some(item) = item else {
            return false;
        };
        let types = objc2_foundation::NSArray::from_retained_slice(&[item]);
        let allocated: Allocated<AnyObject> = msg_send![picker_class, alloc];
        let picker: Option<Retained<AnyObject>> = msg_send![
            allocated,
            initForOpeningContentTypes: &*types,
            asCopy: true
        ];
        let Some(picker) = picker else {
            return false;
        };
        let delegate = PickerDelegate::new(sender, main_thread);
        let _: () = msg_send![&*picker, setDelegate: &*delegate];
        let _: () = msg_send![&*picker, setAllowsMultipleSelection: multiple];
        // A swipe-down dismissal would not reach the delegate; Cancel always does.
        let _: () = msg_send![&*picker, setModalInPresentation: true];
        ACTIVE_DELEGATE.with_borrow_mut(|active| *active = Some(delegate));
        let completion: Option<&Block<dyn Fn()>> = None;
        let _: () = msg_send![
            &*presenter,
            presentViewController: &*picker,
            animated: true,
            completion: completion
        ];
    }
    true
}

/// The key window's root view controller, followed through anything it already presents.
fn top_view_controller(main_thread: MainThreadMarker) -> Option<Retained<AnyObject>> {
    let application = shared_application(main_thread)?;
    // SAFETY: These are nullable UIApplication, UIWindow and UIViewController properties read on
    // the main thread. `keyWindow` is deprecated but still names the game's only window.
    unsafe {
        let mut window: Option<Retained<AnyObject>> = msg_send![&*application, keyWindow];
        if window.is_none() {
            let windows: Option<Retained<AnyObject>> = msg_send![&*application, windows];
            window = windows.and_then(|windows| msg_send![&*windows, firstObject]);
        }
        let root: Option<Retained<AnyObject>> = msg_send![&*window?, rootViewController];
        let mut controller = root?;
        loop {
            let presented: Option<Retained<AnyObject>> =
                msg_send![&*controller, presentedViewController];
            match presented {
                Some(presented) => controller = presented,
                None => return Some(controller),
            }
        }
    }
}

/// Reads the file paths of an `NSArray<NSURL *>`.
fn file_paths(urls: &AnyObject) -> Vec<PathBuf> {
    // SAFETY: UIKit passes an NSArray of file NSURLs; `count`, `objectAtIndex:` and `path` are
    // their documented accessors.
    unsafe {
        let count: usize = msg_send![urls, count];
        (0..count)
            .filter_map(|index| {
                let url: Retained<NSURL> = msg_send![urls, objectAtIndex: index];
                url.path().map(|path| PathBuf::from(path.to_string()))
            })
            .collect()
    }
}

/// Distinguishes concurrent selections' directories within this process.
static SELECTIONS: AtomicU64 = AtomicU64::new(0);

/// Moves UIKit's copies into `selection/index` directories of their own, the layout whose
/// cleanup `PickedFile` owns, so removing a consumed copy never touches UIKit's inbox. A copy
/// over `max_bytes` fails the whole selection after every copy is removed.
fn claim(picked: Vec<PathBuf>, max_bytes: Option<u64>) -> Result<Vec<PathBuf>, String> {
    let selection = std::env::temp_dir().join("document-picker").join(format!(
        "{}-{}",
        std::process::id(),
        SELECTIONS.fetch_add(1, Ordering::Relaxed)
    ));
    let mut claimed = Vec::with_capacity(picked.len());
    let mut failure = None;
    for (index, source) in picked.into_iter().enumerate() {
        if failure.is_some() {
            let _ = std::fs::remove_file(&source);
            continue;
        }
        match claim_one(&source, &selection.join(index.to_string()), max_bytes) {
            Ok(path) => claimed.push(path),
            Err(error) => failure = Some(error),
        }
    }
    match failure {
        None => Ok(claimed),
        Some(error) => {
            let _ = std::fs::remove_dir_all(&selection);
            Err(error)
        }
    }
}

fn claim_one(source: &Path, index: &Path, max_bytes: Option<u64>) -> Result<PathBuf, String> {
    let name = source
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let size = std::fs::metadata(source)
        .map_err(|error| format!("could not read {}: {error}", source.display()))?
        .len();
    if max_bytes.is_some_and(|limit| size > limit) {
        let _ = std::fs::remove_file(source);
        return Err(format!(
            "{} is larger than the {} MiB import limit",
            name.to_string_lossy(),
            max_bytes.unwrap_or_default() / (1024 * 1024)
        ));
    }
    std::fs::create_dir_all(index)
        .map_err(|error| format!("could not create {}: {error}", index.display()))?;
    let target = index.join(name);
    if std::fs::rename(source, &target).is_err() {
        std::fs::copy(source, &target)
            .map_err(|error| format!("could not copy {}: {error}", source.display()))?;
        let _ = std::fs::remove_file(source);
    }
    Ok(target)
}

