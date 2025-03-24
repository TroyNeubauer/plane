static TASK_LIST: heapless::FnvIndexMap<u32, defmt::Str, 16> = heapless::FnvIndexMap::new();

/// Identical to [`embassy_executor::Spawner`] but forces task names to be included when spawning.
#[derive(Copy, Clone)]
pub struct Spawner(embassy_executor::Spawner);

impl From<embassy_executor::Spawner> for Spawner {
    fn from(value: embassy_executor::Spawner) -> Self {
        Self(value)
    }
}

impl Spawner {}
