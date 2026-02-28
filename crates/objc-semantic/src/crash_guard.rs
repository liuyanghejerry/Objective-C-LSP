//! SIGSEGV / SIGBUS crash guard for libclang calls.
//!
//! libclang may SIGSEGV when given malformed input or wrong SDK flags.
//! This module installs a signal handler that uses `sigsetjmp`/`siglongjmp`
//! to recover from such crashes gracefully, returning an `Err` instead of
//! terminating the whole LSP server process.
//!
//! # Safety
//! `siglongjmp` from a signal handler is well-defined on POSIX (macOS / Linux).
//! We store a raw pointer to the stack-allocated jump buffer in a thread-local
//! so that the signal handler can call `siglongjmp` back to the exact frame
//! that called `sigsetjmp`.  The pointer is valid for the entire duration of
//! `with_crash_guard` because the buffer lives on that function's stack frame.
//!
//! MUST NOT hold Mutex locks inside the guarded closure — longjmp past a
//! locked Mutex will deadlock on next acquisition.

#[cfg(unix)]
mod imp {
    use anyhow::{bail, Result};
    use std::cell::Cell;

    // ── Raw C bindings ───────────────────────────────────────────────────────
    // libc on macOS does not expose sigjmp_buf / sigsetjmp / siglongjmp,
    // so we declare them ourselves.
    // On macOS arm64 sizeof(sigjmp_buf) == 196 bytes (~49 ints).
    // Using [i32; 64] == 256 bytes gives comfortable headroom on both
    // arm64 and x86_64 macOS without over-allocating.

    type SigJmpBuf = [libc::c_int; 64];

    extern "C" {
        fn sigsetjmp(env: *mut SigJmpBuf, savemask: libc::c_int) -> libc::c_int;
        fn siglongjmp(env: *mut SigJmpBuf, val: libc::c_int) -> !;
    }

    // ── Thread-local state ───────────────────────────────────────────────────
    //
    // We store a RAW POINTER to the stack-allocated SigJmpBuf rather than
    // a copy of it.  Copying the buffer after sigsetjmp fills it is unsafe
    // because the buffer encodes the exact stack-frame address; a copy would
    // point siglongjmp at the wrong frame.

    thread_local! {
        /// Raw pointer to the active SigJmpBuf on the stack of with_crash_guard.
        /// NULL when no guard is active.
        static JMP_BUF_PTR: Cell<*mut SigJmpBuf> = const { Cell::new(std::ptr::null_mut()) };
        static IN_GUARD: Cell<bool> = const { Cell::new(false) };
    }

    // ── Signal handler ───────────────────────────────────────────────────────
    //
    // Registered WITHOUT SA_SIGINFO so the kernel calls it with the standard
    // single-int signature `fn(sig: c_int)`.

    extern "C" fn crash_handler(sig: libc::c_int) {
        let has_guard = IN_GUARD.with(|g| g.get());
        if has_guard {
            let ptr = JMP_BUF_PTR.with(|c| c.get());
            if !ptr.is_null() {
                // SAFETY: ptr points to a valid SigJmpBuf on the stack of
                // the active with_crash_guard call.  siglongjmp unwinds back
                // to the sigsetjmp checkpoint in that frame.
                unsafe { siglongjmp(ptr, sig) };
            }
        }
        // No active guard — reinstate default handler and re-raise so the
        // kernel delivers the original crash (core dump / debugger break).
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    fn install_handlers() {
        static INSTALLED: std::sync::Once = std::sync::Once::new();
        INSTALLED.call_once(|| unsafe {
            let mut sa: libc::sigaction = std::mem::zeroed();
            // Use sa_handler (not SA_SIGINFO) so the kernel passes a single
            // int argument matching our handler's signature.
            sa.sa_sigaction = crash_handler as usize;
            sa.sa_flags = 0; // deliberately no SA_SIGINFO
            libc::sigfillset(&mut sa.sa_mask); // block all signals during handler
            libc::sigaction(libc::SIGSEGV, &sa, std::ptr::null_mut());
            libc::sigaction(libc::SIGBUS, &sa, std::ptr::null_mut());
        });
    }

    // ── Public API ───────────────────────────────────────────────────────────

    /// Run `f` inside a SIGSEGV / SIGBUS crash guard.
    ///
    /// Returns `Err` if the closure causes a segfault or bus error.
    /// Returns the closure's result otherwise.
    pub fn with_crash_guard<T, F>(f: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        install_handlers();

        // Nested guard: the outer one already covers it.
        if IN_GUARD.with(|g| g.get()) {
            return f();
        }

        // Stack-allocate the jump buffer.  It MUST stay on this stack frame
        // for the entire duration of the guard — do not move or copy it.
        let mut buf: SigJmpBuf = [0; 64];

        // Publish the pointer before arming the guard flag so the signal
        // handler always sees a valid pointer when IN_GUARD is true.
        JMP_BUF_PTR.with(|c| c.set(&mut buf as *mut _));
        IN_GUARD.with(|g| g.set(true));

        // SAFETY: sigsetjmp saves the register/stack state into `buf`.
        // On the first call it returns 0; if siglongjmp is called from the
        // signal handler it returns the signal number (non-zero).
        let rc = unsafe { sigsetjmp(&mut buf as *mut _, 1) };

        if rc == 0 {
            // Normal path: run the closure.
            let result = f();
            // Disarm.
            IN_GUARD.with(|g| g.set(false));
            JMP_BUF_PTR.with(|c| c.set(std::ptr::null_mut()));
            result
        } else {
            // Signal path: we got here via siglongjmp from crash_handler.
            IN_GUARD.with(|g| g.set(false));
            JMP_BUF_PTR.with(|c| c.set(std::ptr::null_mut()));
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
