//! Process-lifetime, single-threaded ownership of both comparison libraries.
//! Rust libtest can schedule callers on different threads. All PDFium calls
//! stay on one owner thread, including initialization and synchronous reentry.

use pdfium_render::prelude::{Pdfium, PdfiumLibraryBindings};
use pdfoxium_diff::library_path;
use std::any::Any;
use std::ops::Deref;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{mpsc, OnceLock};
use std::thread::{self, ThreadId};

type Panic = Box<dyn Any + Send>;
type Job = Box<dyn FnOnce(&Libraries) + Send>;

pub struct Api {
    bindings: Box<dyn PdfiumLibraryBindings>,
    // A second reference to the same DSO supplies recently added exports that
    // pdfium-render has not bound yet. Both references remain owner-local.
    symbols: libloading::Library,
}

impl Api {
    fn load(backend: &str) -> Self {
        let path = library_path(backend);
        assert!(
            path.is_file(),
            "missing comparison library: {}",
            path.display()
        );
        // SAFETY: the bundled/test-built DSO has the pinned public ABI. Its
        // references remain alive for the process lifetime, on this thread.
        let symbols = unsafe { libloading::Library::new(&path) }
            .unwrap_or_else(|error| panic!("load {}: {error}", path.display()));
        let bindings = Pdfium::bind_to_library(&path)
            .unwrap_or_else(|error| panic!("bind {}: {error}", path.display()));
        Self { bindings, symbols }
    }

    #[allow(dead_code)] // Only the recent-export scenarios need this handle.
    pub fn symbols(&self) -> &libloading::Library {
        &self.symbols
    }
}

impl AsRef<dyn PdfiumLibraryBindings> for Api {
    fn as_ref(&self) -> &(dyn PdfiumLibraryBindings + 'static) {
        self.bindings.as_ref()
    }
}

impl Deref for Api {
    type Target = dyn PdfiumLibraryBindings;

    fn deref(&self) -> &Self::Target {
        self.bindings.as_ref()
    }
}

struct Libraries {
    native: Api,
    rust: Api,
}

impl Libraries {
    fn load() -> Self {
        let libraries = Self {
            native: Api::load("native"),
            rust: Api::load("rust"),
        };
        // SAFETY: initialization happens exactly once on the owner thread;
        // no scenario can run until the startup handshake has completed.
        unsafe {
            libraries.native.FPDF_InitLibrary();
            libraries.rust.FPDF_InitLibrary();
        }
        libraries
    }
}

struct Owner {
    queue: mpsc::Sender<Job>,
    thread: ThreadId,
}

static OWNER: OnceLock<Owner> = OnceLock::new();

impl Owner {
    fn start() -> Self {
        let (queue, jobs) = mpsc::channel::<Job>();
        let (ready, startup) = mpsc::sync_channel::<Result<ThreadId, Panic>>(1);
        thread::Builder::new()
            .name("pdfium-integration-owner".into())
            .spawn(move || match catch_unwind(Libraries::load) {
                Ok(libraries) => {
                    // Intentional process-lifetime ownership. Default libtest
                    // has no suite teardown hook. Neither bindings nor DSOs
                    // may unload before native thread-local destructors run.
                    // LLVM's normal process-exit profile flushing still runs.
                    let libraries = Box::leak(Box::new(libraries));
                    if ready.send(Ok(thread::current().id())).is_err() {
                        return;
                    }
                    for job in jobs {
                        job(libraries);
                    }
                }
                Err(panic) => {
                    let _ = ready.send(Err(panic));
                }
            })
            .expect("start comparison-library owner thread");
        let thread = match startup.recv().expect("comparison-library startup stopped") {
            Ok(thread) => thread,
            Err(panic) => resume_unwind(panic),
        };
        Self { queue, thread }
    }
}

/// Run one scenario on the initialized libraries' owning thread. Create and
/// close document/page/form/callback owners inside the scenario. Callbacks can
/// reenter their passed bindings directly; recursively dispatching a scenario
/// would deadlock and is rejected. Assertion panics propagate to the caller.
#[track_caller]
pub fn with_libraries<R: Send + 'static>(
    scenario: impl FnOnce(&Api, &Api) -> R + Send + 'static,
) -> R {
    let call_site = std::panic::Location::caller();
    let owner = OWNER.get_or_init(Owner::start);
    assert_ne!(
        thread::current().id(),
        owner.thread,
        "callbacks must reenter bindings directly, not dispatch another scenario"
    );
    let (completed, result) = mpsc::sync_channel(1);
    owner
        .queue
        .send(Box::new(move |libraries| {
            let outcome = catch_unwind(AssertUnwindSafe(|| {
                scenario(&libraries.native, &libraries.rust)
            }));
            let _ = completed.send(outcome);
        }))
        .unwrap_or_else(|_| panic!("comparison-library owner stopped"));
    match result.recv().expect("comparison scenario stopped") {
        Ok(value) => value,
        Err(panic) => {
            // The permanent owner inherits the first libtest caller's output
            // capture. Replay the failure on this caller before forwarding
            // its original payload; resume_unwind does not invoke panic hooks.
            let message = panic
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic payload");
            eprintln!(
                "comparison scenario for {} at {call_site} panicked: {message}",
                thread::current().name().unwrap_or("unnamed test")
            );
            resume_unwind(panic)
        }
    }
}
