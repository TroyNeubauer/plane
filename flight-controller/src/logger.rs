use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

use bitflare::BitflareWriter;
use embassy_executor::Spawner;
use heapless::Vec;
use plane_core::{FcOutput, MAX_FC_OUTPUT_PACKET};

#[defmt::global_logger]
struct Logger;

static ENCODER: BitflareEncoder = BitflareEncoder::new();

struct BitflareEncoder {
    /// A boolean lock
    ///
    /// Is `true` when `acquire` has been called and we have exclusive access to inner
    taken: AtomicBool,
    inner: UnsafeCell<Inner>,
    num_tasks: AtomicUsize,
}

struct Inner {
    /// We need to remember this to exit a critical section
    cs_restore: critical_section::RestoreState,
    encoder: defmt::Encoder,
    spawner: Option<Spawner>,
}

/// Creates a new async task that sends the given bytes
fn write_bytes(b: &[u8]) {
    let Ok(log_payload) = b.try_into() else {
        panic!("Log message too big: {}", b.len());
    };

    let mut packet = [0u8; MAX_FC_OUTPUT_PACKET];
    let mut writer = BitflareWriter::new(&mut packet);
    writer
        .write_payload(|dst| {
            let payload =
                postcard::to_slice(&FcOutput::DefmtLog(log_payload), dst).map_err(|_| ())?;
            Ok(payload.len())
        })
        .unwrap();
    let packet = writer.finish();

    embassy_futures::block_on(async move {
        let mut guard = crate::RADIO_SERIAL.lock().await;
        if let Some(serial) = guard.as_mut() {
            let _ = serial.write(packet).await;
        }
    });

    // TODO: do IO in background
    /*
    ENCODER.num_tasks.fetch_add(1, Ordering::AcqRel);
    if let Some(spawner) = self.spawner.as_ref() {
        spawner.spawn(token)
    }
    */
}

pub fn set_spawner(spawner: Spawner) {
    ENCODER.set_spawner(spawner);
}

impl BitflareEncoder {
    /// Create a new semihosting-based defmt-encoder
    const fn new() -> BitflareEncoder {
        BitflareEncoder {
            taken: AtomicBool::new(false),
            inner: UnsafeCell::new(Inner {
                cs_restore: critical_section::RestoreState::invalid(),
                encoder: defmt::Encoder::new(),
                spawner: None,
            }),
            num_tasks: AtomicUsize::new(0),
        }
    }

    fn set_spawner(&self, spawner: Spawner) {
        critical_section::with(|_| {
            if self.taken.load(Ordering::Relaxed) {
                panic!("set_spawner reentrantly")
            }
            // no need for CAS because we are in a critical section
            self.taken.store(true, Ordering::Relaxed);

            // Safety: We are in a critical section and there is no reentrantly
            let inner = unsafe { &mut *self.inner.get() };
            inner.spawner = Some(spawner);

            self.taken.store(false, Ordering::Relaxed);
        })
    }

    /// Acquire the defmt encoder.
    fn acquire(&self) {
        // Safety: Must be paired with corresponding call to release(), see below
        let restore = unsafe { critical_section::acquire() };

        if self.taken.load(Ordering::Relaxed) {
            panic!("defmt logger taken reentrantly")
        }
        self.taken.store(true, Ordering::Relaxed);

        // Safety: We are in a critical section and there is no reentrantly
        let inner = unsafe { &mut *self.inner.get() };
        inner.cs_restore = restore;
        inner.encoder.start_frame(|b| {
            write_bytes(b);
        });
    }

    /// Release the defmt encoder.
    unsafe fn release(&self) {
        if !self.taken.load(Ordering::Relaxed) {
            panic!("defmt release out of context")
        }

        // Safety: We are in the critical section and not being called reentrantly
        let inner = unsafe { &mut *self.inner.get() };
        inner.encoder.end_frame(|b| {
            write_bytes(b);
        });

        // Maybe always flush here?

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

        // Safety: taken is set therefore we are in a critical section
        let inner = unsafe { &mut *self.inner.get() };

        inner.encoder.write(bytes, |b| {
            write_bytes(b);
        });
    }
}

unsafe impl Sync for BitflareEncoder {}

unsafe impl defmt::Logger for Logger {
    fn acquire() {
        ENCODER.acquire();
    }

    unsafe fn flush() {
        // Data always written immediately
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
