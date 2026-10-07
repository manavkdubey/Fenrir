use std::{collections::VecDeque, time::Instant};

use bevy::prelude::Resource;

pub const SAMPLE_WINDOW: usize = 240;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorkCounts {
    pub bullet_pairs: u64,
    pub narrow_candidates: u64,
    pub projectile_cell_tests: u64,
    pub blast_pairs: u64,
    pub blast_cell_tests: u64,
    pub blasts: u64,
    pub secondary_triggers: u64,
    pub split_bodies: u64,
    pub new_fragments: u64,
}

impl WorkCounts {
    pub fn accumulate(&mut self, other: Self) {
        self.bullet_pairs += other.bullet_pairs;
        self.narrow_candidates += other.narrow_candidates;
        self.projectile_cell_tests += other.projectile_cell_tests;
        self.blast_pairs += other.blast_pairs;
        self.blast_cell_tests += other.blast_cell_tests;
        self.blasts += other.blasts;
        self.secondary_triggers += other.secondary_triggers;
        self.split_bodies += other.split_bodies;
        self.new_fragments += other.new_fragments;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TickSample {
    pub work: WorkCounts,
    pub simulation_ms: f64,
    pub collision_ms: f64,
    pub explosion_ms: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub count: usize,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

impl Summary {
    pub fn from_samples(samples: impl IntoIterator<Item = f64>) -> Option<Self> {
        let mut sorted: Vec<_> = samples
            .into_iter()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .collect();
        if sorted.is_empty() {
            return None;
        }
        sorted.sort_by(f64::total_cmp);
        let quantile = |percent: f64| sorted[(percent * sorted.len() as f64).ceil() as usize - 1];
        Some(Self {
            count: sorted.len(),
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: quantile(0.50),
            p95: quantile(0.95),
            p99: quantile(0.99),
        })
    }
}

#[derive(Resource, Default)]
pub struct Performance {
    pub current: TickSample,
    pub completed_ticks: u64,
    started: Option<Instant>,
    ticks: VecDeque<f64>,
    frames: VecDeque<f64>,
}

impl Performance {
    pub fn begin_tick(&mut self) {
        self.current = TickSample::default();
        self.started = Some(Instant::now());
    }

    pub fn finish_tick(&mut self) {
        if let Some(started) = self.started.take() {
            self.current.simulation_ms = started.elapsed().as_secs_f64() * 1000.0;
            Self::push(&mut self.ticks, self.current.simulation_ms);
            self.completed_ticks += 1;
        }
    }

    pub fn record_frame(&mut self, seconds: f64) {
        if seconds.is_finite() && seconds > 0.0 {
            Self::push(&mut self.frames, seconds * 1000.0);
        }
    }

    fn push(samples: &mut VecDeque<f64>, value: f64) {
        if samples.len() == SAMPLE_WINDOW {
            samples.pop_front();
        }
        samples.push_back(value);
    }

    pub fn simulation_summary(&self) -> Option<Summary> {
        Summary::from_samples(self.ticks.iter().copied())
    }

    pub fn frame_summary(&self) -> Option<Summary> {
        Summary::from_samples(self.frames.iter().copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_nearest_rank_and_ignore_invalid_measurements() {
        assert_eq!(Summary::from_samples([]), None);
        let stats =
            Summary::from_samples((1..=100).map(f64::from).chain([f64::NAN, -1.0])).unwrap();
        assert_eq!(stats.count, 100);
        assert_eq!(stats.mean, 50.5);
        assert_eq!((stats.p50, stats.p95, stats.p99), (50.0, 95.0, 99.0));
        assert_eq!(Summary::from_samples([7.0]).unwrap().p99, 7.0);
    }

    #[test]
    fn windows_are_bounded_and_tick_work_is_reset_without_resetting_history() {
        let mut performance = Performance::default();
        for _ in 0..SAMPLE_WINDOW + 20 {
            performance.begin_tick();
            performance.current.work.bullet_pairs = 5;
            performance.finish_tick();
            performance.record_frame(0.016);
        }
        assert_eq!(
            performance.simulation_summary().unwrap().count,
            SAMPLE_WINDOW
        );
        assert_eq!(performance.frame_summary().unwrap().count, SAMPLE_WINDOW);
        performance.begin_tick();
        assert_eq!(performance.current.work, WorkCounts::default());
        performance.finish_tick();
        let completed = performance.completed_ticks;
        performance.finish_tick();
        assert_eq!(performance.completed_ticks, completed);
        performance.record_frame(f64::NAN);
        performance.record_frame(0.0);
        assert_eq!(performance.frame_summary().unwrap().count, SAMPLE_WINDOW);
    }
}
