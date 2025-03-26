use core::{borrow::BorrowMut, cell::RefCell, sync::atomic::AtomicU32};

use defmt::info;
use embassy_executor::{SpawnError, SpawnToken};
use embassy_sync::blocking_mutex::{self, raw::CriticalSectionRawMutex};
use plane_core::MAX_ASYNC_TASKS;

static TASK_NAMES: blocking_mutex::Mutex<
    CriticalSectionRawMutex,
    RefCell<heapless::FnvIndexMap<u32, defmt::Str, MAX_ASYNC_TASKS>>,
> = blocking_mutex::Mutex::new(RefCell::new(heapless::FnvIndexMap::new()));

/*
pub struct AsyncState {
    /// The number of ticks that passed since the last update
    pub ticks_total: u32,
    /// Overhead with performing this bookkeeping
    pub ticks_overhead: u32,
    pub tasks: heapless::Vec<AsyncTaskState, { MAX_ASYNC_TASKS }>,
    pub dma_actions: heapless::Vec<DmaAction, { MAX_DMA_ACTIONS }>,
}

pub struct DmaAction {
    pub name: Str,
    /// The number of ticks this DMA transfer was running for
    pub ticks_running: u32,
    /// The number of bytes copied
    pub bytes_copied: u32,
}

pub struct AsyncTaskState {
    pub name: Str,
    /// The number of ticks the CPU was running this task's poll function
    pub ticks_running: u32,
    /// The number of ticks where this task was runnable, but the CPU was consumed with other things
    pub ticks_blocked: u32,
}
*/

// _embassy_trace_executor_idle    537141224
// _embassy_trace_executor_idle    537141224
// _embassy_trace_task_ready_begin 537141224 536871384
// _embassy_trace_task_exec_begin  537141224 536871384
// _embassy_trace_task_exec_end    537141224 536871384
// _embassy_trace_executor_idle    537141224

#[unsafe(no_mangle)]
fn _embassy_trace_task_new(_executor_id: u32, task_id: u32) {
    info!("embassy_trace_task_new {} {}", _executor_id, task_id);
    TASK_NAMES.lock(|t| {
        if !t.borrow().contains_key(&task_id) {
            defmt::panic!(
                "Async task spawned without name! Make sure all async tasks use the spawn wrapper"
            );
        }
    });
}

fn task_name(task_id: u32) -> defmt::Str {
    TASK_NAMES.lock(|t| {
        t.borrow()
            .get(&task_id)
            .copied()
            .unwrap_or(defmt::intern!("<unknown>"))
    })
}

#[unsafe(no_mangle)]
fn _embassy_trace_task_exec_begin(_executor_id: u32, _task_id: u32) {
    // before poll
    info!(
        "_embassy_trace_task_exec_begin {} {} ({})",
        _executor_id,
        task_name(_task_id),
        _task_id
    );
}

#[unsafe(no_mangle)]
fn _embassy_trace_task_exec_end(_executor_id: u32, _task_id: u32) {
    // after poll
    info!(
        "_embassy_trace_task_exec_end {} {} ({})",
        _executor_id,
        task_name(_task_id),
        _task_id
    );
}

#[unsafe(no_mangle)]
fn _embassy_trace_task_ready_begin(_executor_id: u32, _task_id: u32) {
    // When a task becomes runnable
    info!(
        "_embassy_trace_task_ready_begin {} {} ({})",
        _executor_id,
        task_name(_task_id),
        _task_id
    );
}

#[unsafe(no_mangle)]
fn _embassy_trace_executor_idle(_executor_id: u32) {
    // Called when no tasks are runnable. can be unused?
    // info!("_embassy_trace_executor_idle {}", _executor_id);
}

/// Identical to [`embassy_executor::Spawner`] but forces task names to be included when spawning.
#[derive(Copy, Clone)]
pub struct Spawner(embassy_executor::Spawner);

impl From<embassy_executor::Spawner> for Spawner {
    fn from(value: embassy_executor::Spawner) -> Self {
        Self(value)
    }
}

impl Spawner {
    /// Get a Spawner for the current executor.
    ///
    /// This function is `async` just to get access to the current async
    /// context. It returns instantly, it does not block/yield.
    ///
    /// # Panics
    ///
    /// Panics if the current executor is not an Embassy executor.
    pub async fn for_current_executor() -> Self {
        Self(embassy_executor::Spawner::for_current_executor().await)
    }

    /// Spawn a task into an executor.
    ///
    /// You obtain the `token` by calling a task function (i.e. one marked with `#[embassy_executor::task]`).
    pub fn spawn<S>(&self, name: defmt::Str, token: SpawnToken<S>) -> Result<(), SpawnError> {
        let a = embassy_time::TICK_HZ;
        TASK_NAMES.lock(|tasks| {
            if tasks.borrow_mut().insert(token.id(), name).is_err() {
                return Err(SpawnError::Busy);
            }
            Ok(())
        })?;
        self.0.spawn(token)
    }

    // Used by the `embassy_executor_macros::main!` macro to throw an error when spawn
    // fails. This is here to allow conditional use of `defmt::unwrap!`
    // without introducing a `defmt` feature in the `embassy_executor_macros` package,
    // which would require use of `-Z namespaced-features`.
    /// Spawn a task into an executor, panicking on failure.
    ///
    /// # Panics
    ///
    /// Panics if the spawning fails.
    pub fn must_spawn<S>(&self, name: defmt::Str, token: SpawnToken<S>) {
        self.0.must_spawn(token);
    }

    // NOTE: disable for now, since we dont wrap SendSpawner
    // Convert this Spawner to a SendSpawner. This allows you to send the
    // spawner to other threads, but the spawner loses the ability to spawn
    // non-Send tasks.
    // pub fn make_send(&self) -> embassy_executor::SendSpawner {
    //     self.0.make_send()
    // }
}
