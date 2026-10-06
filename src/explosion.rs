use std::collections::VecDeque;

use bevy::{math::Vec2, prelude::Resource};

pub const BLAST_RADIUS: f32 = 120.0;
pub const SECONDARY_RADIUS: f32 = 60.0;
pub const PULSE_DURATION: f32 = 0.35;
pub const MAX_CHAIN_BLASTS: usize = 12;
pub const MAX_DEPTH: u8 = 3;
pub const MAX_PER_TICK: usize = 2;
pub const MAX_ACTIVE_CHAINS: usize = 4;
pub const MAX_PULSES: usize = 32;
pub const MIN_TRIGGER_CELLS: u32 = 16;
pub const SECONDARY_DELAY: f64 = 0.08;
const MAX_PENDING: usize = MAX_CHAIN_BLASTS * MAX_ACTIVE_CHAINS;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blast {
    pub center: Vec2,
    chain: u64,
    depth: u8,
}

impl Blast {
    pub fn radius(self) -> f32 {
        if self.depth == 0 {
            BLAST_RADIUS
        } else {
            SECONDARY_RADIUS
        }
    }

    pub fn is_secondary(self) -> bool {
        self.depth > 0
    }

    pub fn velocity(self, position: Vec2) -> Vec2 {
        let offset = position - self.center;
        let distance = offset.length();
        let radius = self.radius();
        if !distance.is_finite() || distance >= radius {
            return Vec2::ZERO;
        }
        let strength = if self.is_secondary() { 150.0 } else { 250.0 };
        offset / distance.max(24.0) * strength * (1.0 - distance / radius)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlastPulse {
    pub blast: Blast,
    pub remaining: f32,
}

struct ScheduledBlast {
    blast: Blast,
    due: f64,
}

struct Chain {
    id: u64,
    reserved: usize,
}

#[derive(Resource, Default)]
pub struct Explosions {
    clock: f64,
    next_chain: u64,
    chains: Vec<Chain>,
    pending: Vec<ScheduledBlast>,
    pulses: VecDeque<BlastPulse>,
    processed_this_tick: usize,
}

impl Explosions {
    pub fn advance(&mut self, delta: f32) {
        if !delta.is_finite() || delta < 0.0 {
            return;
        }
        self.clock += f64::from(delta);
        self.processed_this_tick = 0;
        for pulse in &mut self.pulses {
            pulse.remaining -= delta;
        }
        self.pulses.retain(|pulse| pulse.remaining > 0.0);
    }

    pub fn start_chain(&mut self, center: Vec2) -> bool {
        if !center.is_finite()
            || self.chains.len() >= MAX_ACTIVE_CHAINS
            || self.pending.len() >= MAX_PENDING
        {
            return false;
        }
        let Some(next) = self.next_chain.checked_add(1) else {
            return false;
        };
        let id = self.next_chain;
        self.next_chain = next;
        self.chains.push(Chain { id, reserved: 1 });
        self.insert(
            Blast {
                center,
                chain: id,
                depth: 0,
            },
            self.clock,
        );
        true
    }

    pub fn schedule_secondary(&mut self, parent: Blast, center: Vec2) -> bool {
        if !center.is_finite() || parent.depth >= MAX_DEPTH || self.pending.len() >= MAX_PENDING {
            return false;
        }
        let Some(chain) = self
            .chains
            .iter_mut()
            .find(|chain| chain.id == parent.chain)
        else {
            return false;
        };
        if chain.reserved >= MAX_CHAIN_BLASTS {
            return false;
        }
        // Reserve on enqueue; processing a blast never replenishes its chain budget.
        chain.reserved += 1;
        self.insert(
            Blast {
                center,
                chain: parent.chain,
                depth: parent.depth + 1,
            },
            self.clock + SECONDARY_DELAY,
        );
        true
    }

    fn insert(&mut self, blast: Blast, due: f64) {
        let position = self.pending.partition_point(|event| event.due <= due);
        self.pending.insert(position, ScheduledBlast { blast, due });
    }

    pub fn take_ready(&mut self) -> Vec<Blast> {
        let mut ready = Vec::new();
        while self.processed_this_tick < MAX_PER_TICK {
            if self
                .pending
                .first()
                .is_none_or(|event| event.due > self.clock)
            {
                break;
            }
            let event = self.pending.remove(0);
            self.processed_this_tick += 1;
            if self.pulses.len() == MAX_PULSES {
                self.pulses.pop_front();
            }
            self.pulses.push_back(BlastPulse {
                blast: event.blast,
                remaining: PULSE_DURATION,
            });
            ready.push(event.blast);
        }
        ready
    }

    pub fn finish_tick(&mut self) {
        // Retire only after the returned batch has had a chance to enqueue children.
        self.chains.retain(|chain| {
            self.pending
                .iter()
                .any(|event| event.blast.chain == chain.id)
        });
    }

    pub fn pulses(&self) -> impl Iterator<Item = &BlastPulse> {
        self.pulses.iter()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_is_reserved_on_enqueue_and_depth_cannot_exceed_three() {
        let mut queue = Explosions::default();
        assert!(queue.start_chain(Vec2::ZERO));
        let root = queue.take_ready()[0];
        for _ in 1..MAX_CHAIN_BLASTS {
            assert!(queue.schedule_secondary(root, Vec2::X));
        }
        assert!(!queue.schedule_secondary(root, Vec2::X));
        assert_eq!(queue.pending_count(), MAX_CHAIN_BLASTS - 1);
        let mut queue = Explosions::default();
        queue.start_chain(Vec2::ZERO);
        let mut parent = queue.take_ready()[0];
        for depth in 1..=MAX_DEPTH {
            assert!(queue.schedule_secondary(parent, Vec2::X));
            queue.advance(0.1);
            parent = queue.take_ready()[0];
            assert_eq!(parent.depth, depth);
        }
        assert!(!queue.schedule_secondary(parent, Vec2::X));
    }

    #[test]
    fn tick_cap_is_shared_across_chains_and_new_primaries_do_not_wait_behind_future_children() {
        let mut queue = Explosions::default();
        queue.start_chain(Vec2::ZERO);
        let root = queue.take_ready()[0];
        queue.schedule_secondary(root, Vec2::X);
        queue.start_chain(Vec2::Y);
        assert_eq!(queue.take_ready()[0].center, Vec2::Y);
        assert!(queue.take_ready().is_empty());
        queue.advance(0.04);
        assert!(queue.take_ready().is_empty());
        queue.advance(0.041);
        assert_eq!(queue.take_ready()[0].center, Vec2::X);
    }

    #[test]
    fn dense_overlapping_chains_have_bounded_total_work_storage_and_pulses() {
        let mut queue = Explosions::default();
        for _ in 0..MAX_ACTIVE_CHAINS {
            assert!(queue.start_chain(Vec2::ZERO));
        }
        assert!(!queue.start_chain(Vec2::ZERO));
        let mut processed = 0;
        for _ in 0..100 {
            queue.advance(0.1);
            let blasts = queue.take_ready();
            assert!(blasts.len() <= MAX_PER_TICK);
            processed += blasts.len();
            for blast in blasts {
                for _ in 0..MAX_CHAIN_BLASTS {
                    queue.schedule_secondary(blast, Vec2::X);
                }
            }
            assert!(queue.pending_count() <= MAX_PENDING);
            assert!(queue.pulses().count() <= MAX_PULSES);
            queue.finish_tick();
        }
        assert_eq!(processed, MAX_ACTIVE_CHAINS * MAX_CHAIN_BLASTS);
        assert_eq!(queue.pending_count(), 0);
        assert_eq!(queue.pulses().count(), 0);
        assert!(queue.chains.is_empty());
    }

    #[test]
    fn visual_cap_drops_oldest_pulses_without_retaining_completed_chains() {
        let mut queue = Explosions::default();
        for i in 0..MAX_PULSES + 5 {
            queue.advance(0.001);
            assert!(queue.start_chain(Vec2::new(i as f32, 0.0)));
            assert_eq!(queue.take_ready().len(), 1);
            queue.finish_tick();
        }
        assert_eq!(queue.pulses().count(), MAX_PULSES);
        assert_eq!(queue.pulses.front().unwrap().blast.center.x, 5.0);
        assert_eq!(
            queue.pulses.back().unwrap().blast.center.x,
            (MAX_PULSES + 4) as f32
        );
        assert!(queue.chains.is_empty() && queue.pending.is_empty());
    }

    #[test]
    fn pulse_expiry_and_invalid_time_cannot_requeue_a_blast() {
        let mut queue = Explosions::default();
        assert!(!queue.start_chain(Vec2::NAN));
        queue.start_chain(Vec2::ZERO);
        let root = queue.take_ready()[0];
        assert!(!queue.schedule_secondary(root, Vec2::INFINITY));
        assert_eq!(queue.pulses().count(), 1);
        queue.finish_tick();
        queue.advance(f32::NAN);
        assert_eq!(queue.pulses().count(), 1);
        queue.advance(PULSE_DURATION + 0.01);
        assert_eq!(queue.pulses().count(), 0);
        assert!(queue.take_ready().is_empty());
        assert!(!queue.schedule_secondary(root, Vec2::ZERO));
    }

    #[test]
    fn outward_velocity_is_bounded_with_zero_at_the_center_and_range_boundary() {
        for depth in 0..=MAX_DEPTH {
            let blast = Blast {
                center: Vec2::ZERO,
                chain: 0,
                depth,
            };
            assert_eq!(blast.velocity(Vec2::ZERO), Vec2::ZERO);
            assert_eq!(blast.velocity(Vec2::X * blast.radius()), Vec2::ZERO);
            assert_eq!(blast.velocity(Vec2::NAN), Vec2::ZERO);
            for i in 1..1000 {
                let velocity = blast.velocity(Vec2::X * (blast.radius() * i as f32 / 1000.0));
                assert!(velocity.is_finite() && velocity.x > 0.0 && velocity.length() <= 250.0);
            }
        }
    }
}
