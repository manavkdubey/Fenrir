use bevy::math::Vec2;

pub const GRID_SIZE: usize = 32;
const HALF_SIZE: f32 = GRID_SIZE as f32 * 0.5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterialGrid {
    rows: [u32; GRID_SIZE],
}

impl MaterialGrid {
    pub fn from_rows(rows: [u32; GRID_SIZE]) -> Self {
        Self { rows }
    }

    pub fn occupied(&self, x: usize, y: usize) -> bool {
        x < GRID_SIZE && y < GRID_SIZE && self.rows[y] & (1 << x) != 0
    }

    pub fn is_empty(&self) -> bool {
        self.rows.iter().all(|row| *row == 0)
    }

    pub fn remove_circle(&mut self, center: Vec2, radius: f32) -> u32 {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return 0;
        }
        let mut removed = 0;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.occupied(x, y)
                    && cell_center(x, y).distance_squared(center) <= radius * radius
                {
                    self.rows[y] &= !(1 << x);
                    removed += 1;
                }
            }
        }
        removed
    }

    pub fn ray_hit(&self, start: Vec2, end: Vec2) -> Option<f32> {
        if !start.is_finite() || !end.is_finite() {
            return None;
        }
        let mut nearest: Option<f32> = None;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.occupied(x, y) {
                    let center = cell_center(x, y);
                    if let Some(t) = segment_box_hit(start, end, center - 0.5, center + 0.5) {
                        nearest = Some(nearest.map_or(t, |previous| previous.min(t)));
                    }
                }
            }
        }
        nearest
    }

    pub fn overlaps_circle(&self, center: Vec2, radius: f32) -> bool {
        if !center.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.occupied(x, y) {
                    let cell = cell_center(x, y);
                    let closest = center.clamp(cell - 0.5, cell + 0.5);
                    if closest.distance_squared(center) <= radius * radius {
                        return true;
                    }
                }
            }
        }
        false
    }
}

// Texture rows point down; simulation coordinates point up.
fn cell_center(x: usize, y: usize) -> Vec2 {
    Vec2::new(x as f32 + 0.5 - HALF_SIZE, HALF_SIZE - y as f32 - 0.5)
}

fn segment_box_hit(start: Vec2, end: Vec2, min: Vec2, max: Vec2) -> Option<f32> {
    let step = end - start;
    let mut entry: f32 = 0.0;
    let mut exit: f32 = 1.0;
    for axis in 0..2 {
        if step[axis] == 0.0 {
            if start[axis] < min[axis] || start[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - start[axis]) / step[axis];
            let b = (max[axis] - start[axis]) / step[axis];
            entry = entry.max(a.min(b));
            exit = exit.min(a.max(b));
            if entry > exit {
                return None;
            }
        }
    }
    Some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{RngExt, SeedableRng, rngs::StdRng};

    #[test]
    fn damage_is_local_repeatable_and_can_remove_every_cell() {
        let mut grid = MaterialGrid::from_rows([u32::MAX; GRID_SIZE]);
        let before = grid.clone();
        assert!(grid.remove_circle(Vec2::ZERO, 4.0) > 0);
        assert!(!grid.occupied(16, 16));
        assert!(grid.occupied(0, 0));
        assert_eq!(grid.remove_circle(Vec2::ZERO, 4.0), 0);
        assert_ne!(grid, before);
        grid.remove_circle(Vec2::ZERO, 24.0);
        assert!(grid.is_empty());
    }

    #[test]
    fn rays_pass_through_a_cut_channel_and_hit_remaining_islands() {
        let mut grid = MaterialGrid::from_rows([u32::MAX; GRID_SIZE]);
        for y in -16..=16 {
            grid.remove_circle(Vec2::new(0.0, y as f32), 2.0);
        }
        assert_eq!(grid.ray_hit(Vec2::Y * 20.0, -Vec2::Y * 20.0), None);
        assert!(
            grid.ray_hit(Vec2::new(-8.0, 20.0), Vec2::new(-8.0, -20.0))
                .is_some()
        );
        assert!(grid.occupied(0, 16) && grid.occupied(31, 16));
        assert!(!grid.overlaps_circle(Vec2::ZERO, 1.0));
        assert!(grid.overlaps_circle(Vec2::ZERO, 3.0));
    }

    #[test]
    fn single_cell_handles_edges_zero_motion_and_texture_orientation() {
        let mut rows = [0; GRID_SIZE];
        rows[0] = 1;
        let grid = MaterialGrid::from_rows(rows);
        assert_eq!(
            grid.ray_hit(Vec2::new(-20.0, 15.5), Vec2::new(-12.0, 15.5)),
            Some(0.5)
        );
        assert_eq!(
            grid.ray_hit(Vec2::new(-15.5, 15.5), Vec2::new(-15.5, 15.5)),
            Some(0.0)
        );
        assert_eq!(
            grid.ray_hit(Vec2::new(-15.5, -15.5), Vec2::new(-15.5, -15.5)),
            None
        );
        assert_eq!(
            grid.ray_hit(Vec2::new(-20.0, 16.0), Vec2::new(-12.0, 16.0)),
            Some(0.5)
        );
        assert_eq!(
            grid.ray_hit(Vec2::new(-20.0, 16.01), Vec2::new(-12.0, 16.01)),
            None
        );
    }

    #[test]
    fn seeded_damage_preserves_all_cells_outside_its_radius() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let rows = std::array::from_fn(|_| rng.random());
            let mut grid = MaterialGrid::from_rows(rows);
            let center = Vec2::new(rng.random_range(-20.0..20.0), rng.random_range(-20.0..20.0));
            let radius = rng.random_range(0.0..8.0);
            let removed = grid.remove_circle(center, radius);
            let mut expected = 0;
            for (y, row) in rows.iter().enumerate() {
                for x in 0..GRID_SIZE {
                    let occupied = row & (1 << x) != 0;
                    let damaged = cell_center(x, y).distance_squared(center) <= radius * radius;
                    assert_eq!(grid.occupied(x, y), occupied && !damaged);
                    expected += u32::from(occupied && damaged);
                }
            }
            assert_eq!(removed, expected);
        }
    }
}
