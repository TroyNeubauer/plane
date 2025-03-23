use core::time::Duration;
use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct ByteRateCounter {
    samples: VecDeque<(Duration, usize)>,
    sample_interval: Duration,
    total: usize,
}

impl Default for ByteRateCounter {
    fn default() -> Self {
        Self {
            samples: VecDeque::new(),
            sample_interval: Duration::from_secs(1),
            total: 0,
        }
    }
}

impl ByteRateCounter {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn over_internal(sample_interval: Duration) -> Self {
        Self {
            samples: VecDeque::new(),
            sample_interval,
            total: 0,
        }
    }

    pub fn update(&mut self, count: usize) {
        let now = get_timestamp();
        self.samples.push_back((now, count));
        self.total += count;

        self.trim(now);
    }

    pub fn rate_f64(&mut self) -> f64 {
        self.trim(get_timestamp());

        self.total as f64 / self.sample_interval.as_secs_f64()
    }

    pub fn count(&mut self) -> usize {
        self.trim(get_timestamp());

        self.total
    }

    fn trim(&mut self, now: Duration) {
        while let Some(s) = self.samples.front() {
            if now.saturating_sub(s.0) > self.sample_interval {
                if let Some((_time, count)) = self.samples.pop_front() {
                    self.total -= count;
                }
            } else {
                break;
            }
        }
    }
}

fn get_timestamp() -> Duration {
    let now = SystemTime::now();
    now.duration_since(UNIX_EPOCH).unwrap_or_default()
}
