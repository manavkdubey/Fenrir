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

    pub fn cell_count(&self) -> u32 {
        self.rows.iter().map(|row| row.count_ones()).sum()
    }

    pub fn centroid(&self) -> Vec2 {
        let mut sum = Vec2::ZERO;
        let mut count = 0;
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if self.occupied(x, y) {
                    sum += cell_center(x, y);
                    count += 1;
                }
            }
        }
        if count == 0 {
            Vec2::ZERO
        } else {
            sum / count as f32
        }
    }

    pub fn components(&self) -> Vec<Self> {
        let mut remaining = self.clone();
        let mut stack = [0; GRID_SIZE * GRID_SIZE];
        let mut components = Vec::new();
        for y in 0..GRID_SIZE {
            for x in 0..GRID_SIZE {
                if !remaining.occupied(x, y) {
                    continue;
                }
                let mut component = Self::from_rows([0; GRID_SIZE]);
                remaining.rows[y] &= !(1 << x);
                stack[0] = y * GRID_SIZE + x;
                let mut len = 1;
                while len > 0 {
                    len -= 1;
                    let index = stack[len];
                    let (cx, cy) = (index % GRID_SIZE, index / GRID_SIZE);
                    component.rows[cy] |= 1 << cx;
                    for (nx, ny) in [
                        (cx.wrapping_sub(1), cy),
                        (cx + 1, cy),
                        (cx, cy.wrapping_sub(1)),
                        (cx, cy + 1),
                    ] {
                        if remaining.occupied(nx, ny) {
                            // Claim on push: each cell enters the bounded stack at most once.
                            remaining.rows[ny] &= !(1 << nx);
                            stack[len] = ny * GRID_SIZE + nx;
                            len += 1;
                        }
                    }
                }
                components.push(component);
            }
        }
        components.sort_by_key(|grid| std::cmp::Reverse(grid.cell_count()));
        components
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
    fn diagonal_cells_are_separate_and_largest_region_is_first() {
        let mut rows = [0; GRID_SIZE];
        rows[0] = 1;
        rows[1] = 2;
        rows[10] = 7 << 10;
        let grid = MaterialGrid::from_rows(rows);
        let parts = grid.components();
        assert_eq!(
            parts
                .iter()
                .map(MaterialGrid::cell_count)
                .collect::<Vec<_>>(),
            [3, 1, 1]
        );
        assert_eq!(parts[1].centroid(), Vec2::new(-15.5, 15.5));
        assert_eq!(
            MaterialGrid::from_rows([0; GRID_SIZE]).centroid(),
            Vec2::ZERO
        );
    }

    #[test]
    fn checkerboard_exercises_maximum_four_neighbour_partition() {
        let grid = MaterialGrid::from_rows(std::array::from_fn(|y| {
            if y % 2 == 0 { 0x5555_5555 } else { 0xAAAA_AAAA }
        }));
        let parts = grid.components();
        assert_eq!(parts.len(), GRID_SIZE * GRID_SIZE / 2);
        assert!(parts.iter().all(|part| part.cell_count() == 1));
        assert!(
            MaterialGrid::from_rows([0; GRID_SIZE])
                .components()
                .is_empty()
        );
        let solid = MaterialGrid::from_rows([u32::MAX; GRID_SIZE]);
        assert_eq!(solid.components(), [solid]);
    }

    #[test]
    fn seeded_partitions_own_each_original_cell_exactly_once() {
        let mut rng = StdRng::seed_from_u64(9001);
        for _ in 0..100 {
            let grid = MaterialGrid::from_rows(std::array::from_fn(|_| rng.random()));
            let parts = grid.components();
            assert_eq!(
                parts.iter().map(MaterialGrid::cell_count).sum::<u32>(),
                grid.cell_count()
            );
            for y in 0..GRID_SIZE {
                for x in 0..GRID_SIZE {
                    assert_eq!(
                        parts.iter().filter(|part| part.occupied(x, y)).count(),
                        usize::from(grid.occupied(x, y))
                    );
                }
            }
            assert_eq!(parts, grid.components());
        }
    }

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
