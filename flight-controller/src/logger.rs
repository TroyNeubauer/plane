//! `defmt` global logger over semihosting
//!
//! WARNING using `semihosting`'s `println!` macro or `Stdout` API will corrupt
//! `defmt` log frames so don't use those APIs.
//!
//! # Critical section implementation
//!
//! This crate uses
//! [`critical-section`](https://github.com/rust-embedded/critical-section) to
//! ensure only one thread is writing to the buffer at a time. You must import a
//! crate that provides a `critical-section` implementation suitable for the
//! current target. See the `critical-section` README for details.
//!
//! For example, for single-core privileged-mode Cortex-M targets, you can add
//! the following to your Cargo.toml.
//!
//! ```toml
//! [dependencies]
//! cortex-m = { version = "0.7.6", features = ["critical-section-single-core"]}
//! ```

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
};

#[defmt::global_logger]
struct Logger;

static ENCODER: BitflareEncoder = BitflareEncoder::new();

struct BitflareEncoder {
    /// A boolean lock
    ///
    /// Is `true` when `acquire` has been called and we have exclusive access to inner
    taken: AtomicBool,
    inner: UnsafeCell<Inner>,
}

struct Inner {
    /// We need to remember this to exit a critical section
    cs_restore: critical_section::RestoreState,
    /// A defmt::Encoder for encoding frames
    encoder: defmt::Encoder,
}

impl BitflareEncoder {
    /// Create a new semihosting-based defmt-encoder
    const fn new() -> BitflareEncoder {
        BitflareEncoder {
            taken: AtomicBool::new(false),
            inner: UnsafeCell::new(Inner {
                cs_restore: critical_section::RestoreState::invalid(),
                encoder: defmt::Encoder::new(),
            }),
        }
    }

    /// Acquire the defmt encoder.
    fn acquire(&self) {
        // Safety: Must be paired with corresponding call to release(), see below
        let restore = unsafe { critical_section::acquire() };

        // NB: You can re-enter critical sections but we need to make sure
        // no-one does that.
        if self.taken.load(Ordering::Relaxed) {
            panic!("defmt logger taken reentrantly")
        }

        // no need for CAS because we are in a critical section
        self.taken.store(true, Ordering::Relaxed);

        // Safety: accessing the cell is OK because we have acquired a critical
        // section.
        let inner = unsafe { &mut *self.inner.get() };
        inner.cs_restore = restore;
        inner.encoder.start_frame(|_b| {
            todo!();
            // if let Some(h) = handle {
            //     _ = h.write_all(b);
            // }
        });
    }

    /// Release the defmt encoder.
    unsafe fn release(&self) {
        if !self.taken.load(Ordering::Relaxed) {
            panic!("defmt release out of context")
        }

        // Safety: accessing the cell is OK because we have acquired a critical
        // section.
        let inner = unsafe { &mut *self.inner.get() };
        inner.encoder.end_frame(|_b| {
            todo!();
            // if let Some(h) = handle {
            //     _ = h.write_all(b);
            // }
        });
        let restore = inner.cs_restore;
        self.taken.store(false, Ordering::Relaxed);

        // paired with exactly one acquire call
        unsafe { critical_section::release(restore) };
    }

    /// Write bytes to the defmt encoder.
    unsafe fn write(&self, bytes: &[u8]) {
        if !self.taken.load(Ordering::Relaxed) {
            panic!("defmt write out of context")
        }

        // Safety: accessing the cell is OK because we have acquired a critical
        // section.
        let inner = unsafe { &mut *self.inner.get() };
        inner.encoder.write(bytes, |_b| {
            // if let Some(h) = handle {
            //     _ = h.write_all(b);
            // }
        });
    }
}

unsafe impl Sync for BitflareEncoder {}

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        ENCODER.acquire();
    }

    unsafe fn flush() {
        // Do nothing.
        //
        // semihosting is fundamentally blocking, and does not have I/O buffers the target can control.
        // After write returns, the host has the data, so there's nothing left to flush.
    }

    unsafe fn release() {
        unsafe {
            ENCODER.release();
        }
    }

    unsafe fn write(bytes: &[u8]) {
        unsafe {
            ENCODER.write(bytes);
        }
    }
}
