use bevy::{math::Vec2, prelude::Resource};

pub const FIELD_RADIUS: f32 = 220.0;
pub const DEPLOY_RANGE: f32 = 300.0;
pub const ACTIVE_DURATION: f32 = 2.0;
pub const COOLDOWN: f32 = 8.0;
pub const MAX_ACCELERATION: f32 = 700.0;
const CORE_RADIUS: f32 = 24.0;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum GravityPhase {
    #[default]
    Ready,
    Active {
        center: Vec2,
        remaining: f32,
    },
    Cooldown {
        remaining: f32,
    },
}

#[derive(Resource, Default)]
pub struct GravityWell {
    phase: GravityPhase,
}

impl GravityWell {
    pub fn phase(&self) -> GravityPhase {
        self.phase
    }

    pub fn activate(&mut self, origin: Vec2, target: Vec2) -> bool {
        if self.phase != GravityPhase::Ready || !origin.is_finite() || !target.is_finite() {
            return false;
        }
        let offset = target - origin;
        if !offset.is_finite() {
            return false;
        }
        let center = origin + offset.clamp_length_max(DEPLOY_RANGE);
        self.phase = GravityPhase::Active {
            center,
            remaining: ACTIVE_DURATION,
        };
        true
    }

    pub fn tick(&mut self, mut delta: f32) {
        if !delta.is_finite() || delta < 0.0 {
            return;
        }
        loop {
            match self.phase {
                GravityPhase::Ready => return,
                GravityPhase::Active { center, remaining } if delta < remaining => {
                    self.phase = GravityPhase::Active {
                        center,
                        remaining: remaining - delta,
                    };
                    return;
                }
                GravityPhase::Active { remaining, .. } => {
                    delta -= remaining;
                    self.phase = GravityPhase::Cooldown {
                        remaining: COOLDOWN,
                    };
                }
                GravityPhase::Cooldown { remaining } if delta < remaining => {
                    self.phase = GravityPhase::Cooldown {
                        remaining: remaining - delta,
                    };
                    return;
                }
                GravityPhase::Cooldown { .. } => {
                    self.phase = GravityPhase::Ready;
                    return;
                }
            }
        }
    }

    pub fn acceleration(&self, position: Vec2) -> Vec2 {
        let GravityPhase::Active { center, .. } = self.phase else {
            return Vec2::ZERO;
        };
        let offset = center - position;
        let distance = offset.length();
        if !distance.is_finite() || distance >= FIELD_RADIUS {
            return Vec2::ZERO;
        }
        // Dividing by the core radius tapers force to zero without a singularity.
        offset / distance.max(CORE_RADIUS) * MAX_ACCELERATION * (1.0 - distance / FIELD_RADIUS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployment_is_clamped_and_active_or_cooling_fields_cannot_retrigger() {
        let mut well = GravityWell::default();
        assert!(well.activate(Vec2::new(30.0, 10.0), Vec2::new(1030.0, 10.0)));
        assert_eq!(
            well.phase(),
            GravityPhase::Active {
                center: Vec2::new(330.0, 10.0),
                remaining: ACTIVE_DURATION
            }
        );
        assert!(!well.activate(Vec2::ZERO, Vec2::ZERO));
        well.tick(ACTIVE_DURATION);
        assert_eq!(
            well.phase(),
            GravityPhase::Cooldown {
                remaining: COOLDOWN
            }
        );
        assert!(!well.activate(Vec2::ZERO, Vec2::ZERO));
        assert_eq!(well.acceleration(Vec2::X), Vec2::ZERO);
        well.tick(COOLDOWN);
        assert_eq!(well.phase(), GravityPhase::Ready);
        assert!(well.activate(Vec2::ZERO, Vec2::ZERO));
    }

    #[test]
    fn force_is_finite_bounded_inward_and_zero_outside_the_radius() {
        let mut well = GravityWell::default();
        well.activate(Vec2::ZERO, Vec2::ZERO);
        assert_eq!(well.acceleration(Vec2::ZERO), Vec2::ZERO);
        assert_eq!(well.acceleration(Vec2::X * FIELD_RADIUS), Vec2::ZERO);
        assert_eq!(
            well.acceleration(Vec2::X * (FIELD_RADIUS + 1.0)),
            Vec2::ZERO
        );
        for step in 1..2200 {
            let position = Vec2::new(step as f32 * 0.1, 0.0);
            let acceleration = well.acceleration(position);
            assert!(acceleration.is_finite());
            assert!(acceleration.length() <= MAX_ACCELERATION);
            assert!(acceleration.x < 0.0 && acceleration.y == 0.0);
        }
        assert!(well.acceleration(Vec2::X * 0.001).length() < well.acceleration(Vec2::X).length());
    }

    #[test]
    fn clock_carries_boundary_overshoot_and_rejects_invalid_inputs() {
        let mut well = GravityWell::default();
        assert!(!well.activate(Vec2::NAN, Vec2::ZERO));
        assert!(!well.activate(Vec2::ZERO, Vec2::INFINITY));
        well.activate(Vec2::ZERO, Vec2::ZERO);
        well.tick(ACTIVE_DURATION + 0.5);
        assert_eq!(
            well.phase(),
            GravityPhase::Cooldown {
                remaining: COOLDOWN - 0.5
            }
        );
        let before = well.phase();
        well.tick(f32::NAN);
        well.tick(-1.0);
        assert_eq!(well.phase(), before);
        well.tick(100.0);
        assert_eq!(well.phase(), GravityPhase::Ready);
    }
}
