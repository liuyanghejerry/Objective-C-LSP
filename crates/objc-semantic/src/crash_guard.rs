//! SIGSEGV / SIGBUS crash guard for libclang calls.
//!
//! libclang may SIGSEGV when given malformed input or wrong SDK flags.
//! This module installs a signal handler that uses `sigsetjmp`/`siglongjmp`
//! to recover from such crashes gracefully, returning an `Err` instead of
//! terminating the whole LSP server process.
//!
//! # Safety
//! `siglongjmp` from a signal handler is well-defined on POSIX (macOS / Linux).
//! We store the jump buffer in a thread-local so concurrent threads are safe.
//!
//! MUST NOT hold Mutex locks inside the guarded closure — longjmp past a
//! locked Mutex will deadlock on next acquisition.

#[cfg(unix)]
mod imp {
    use anyhow::{bail, Result};
    use std::cell::Cell;

    // ── Raw C bindings ───────────────────────────────────────────────────────
    // libc on macOS does not expose sigjmp_buf / sigsetjmp / siglongjmp,
    // so we declare them ourselves. On arm64 macOS sigjmp_buf is 64 ints;
    // using [i32; 64] is safe on both arm64 and x86_64.

    type SigJmpBuf = [libc::c_int; 64];

    extern "C" {
        fn sigsetjmp(env: *mut SigJmpBuf, savemask: libc::c_int) -> libc::c_int;
        fn siglongjmp(env: *mut SigJmpBuf, val: libc::c_int) -> !;
    }

    // ── Thread-local state ───────────────────────────────────────────────────

    thread_local! {
        static JMP_BUF: Cell<Option<SigJmpBuf>> = const { Cell::new(None) };
        static IN_GUARD: Cell<bool> = const { Cell::new(false) };
    }

    // ── Signal handler ───────────────────────────────────────────────────────

    extern "C" fn crash_handler(sig: libc::c_int) {
        let has_guard = IN_GUARD.with(|g| g.get());
        if has_guard {
            JMP_BUF.with(|cell| {
                if let Some(mut buf) = cell.get() {
                    // SAFETY: siglongjmp to the sigsetjmp frame active on this thread.
                    unsafe { siglongjmp(&mut buf as *mut _, sig) };
                }
            });
        }
        // No active guard — reinstate default and re-raise.
        unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = libc::SIG_DFL;
            libc::sigaction(sig, &sa, std::ptr::null_mut());
            libc::raise(sig);
        }
    }

    fn install_handlers() {
        static INSTALLED: std::sync::Once = std::sync::Once::new();
        INSTALLED.call_once(|| unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            sa.sa_sigaction = crash_handler as usize;
            sa.sa_flags = libc::SA_SIGINFO;
            libc::sigaction(libc::SIGSEGV, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGBUS, &sa, std::ptr::null_mut());
        });
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Run `f` inside a SIGSEGV / SIGBUS crash guard.
    ///
    /// Returns `Err` if the closure causes a segfault or bus error.
    pub fn with_crash_guard<T, F>(f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        install_handlers();

        // Nested guard: the outer one already covers it.
        if IN_GUARD.with(|g| g.get()) {
            return f();
        }

        let mut buf: SigJmpBuf = [0; 64];

        IN_GUARD.with(|g| g.set(true));
        JMP_BUF.with(|b| b.set(Some(buf)));

        // SAFETY: sigsetjmp stores state; siglongjmp returns here with rc != 0.
        let rc = unsafe { sigsetjmp(&mut buf as *mut _, 1) };
        JMP_BUF.with(|b| b.set(Some(buf)));

        if rc == 0 {
            let r = f();
            IN_GUARD.with(|g| g.set(false));
            JMP_BUF.with(|b| b.set(None));
            r
        } else {
            IN_GUARD.with(|g| g.set(false));
            JMP_BUF.with(|b| b.set(None));
            let sig_name = if rc == libc::SIGSEGV {
                "SIGSEGV"
            } else if rc == libc::SIGBUS {
                "SIGBUS"
            } else {
                "unknown signal"
            };
            tracing::warn!(
                "libclang crashed with {} — file skipped (wrong SDK flags?)",
                sig_name
            );
            bail!(
                "libclang crashed with {} — file skipped (wrong SDK flags?)",
                sig_name
            )
        }
    }
}

#[cfg(not(unix))]
mod imp {
    use anyhow::Result;
    pub fn with_crash_guard<T, F: FnOnce() -> Result<T>>(f: F) -> Result<T> {
        f()
    }
}

pub use imp::with_crash_guard;
