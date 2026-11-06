//! Flow-field navigation. A single Dijkstra from the player and the reactor produces a
//! direction per cell, so thousands of zombies can path for the price of one lookup each.
//! Player structures are passable at a high cost: the horde detours around a short wall
//! but chews straight through a long one.
use super::map::{Map, HALF};
use glam::Vec2;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const CELL: f32 = 1.0;
pub const DIM: usize = (2.0 * HALF / CELL) as usize;

pub struct Nav {
    blocked: Vec<bool>,
    extra: Vec<u16>,
    pub dist: Vec<u32>,
    flow: Vec<[i8; 2]>,
    heap: BinaryHeap<Reverse<(u32, u32)>>,
}

const NEIGH: [(i32, i32, u32); 8] = [
    (1, 0, 10), (-1, 0, 10), (0, 1, 10), (0, -1, 10),
    (1, 1, 14), (1, -1, 14), (-1, 1, 14), (-1, -1, 14),
];

pub fn cell_of(p: Vec2) -> (usize, usize) {
    let f = |v: f32| (((v + HALF) / CELL) as i32).clamp(0, DIM as i32 - 1) as usize;
    (f(p.x), f(p.y))
}

impl Nav {
    pub fn new(map: &Map) -> Self {
        let mut blocked = vec![false; DIM * DIM];
        for z in 0..DIM {
            for x in 0..DIM {
                let p = Vec2::new(x as f32 + 0.5 - HALF, z as f32 + 0.5 - HALF);
                blocked[z * DIM + x] = map.is_blocked(p, 0.34);
            }
        }
        Nav {
            blocked,
            extra: vec![0; DIM * DIM],
            dist: vec![u32::MAX; DIM * DIM],
            flow: vec![[0, 0]; DIM * DIM],
            heap: BinaryHeap::with_capacity(4096),
        }
    }

    pub fn clear_extra(&mut self) {
        self.extra.iter_mut().for_each(|c| *c = 0);
    }

    /// Mark world area as costing `cost` extra to traverse.
    pub fn add_cost(&mut self, min: Vec2, max: Vec2, cost: u16) {
        let (x0, z0) = cell_of(min);
        let (x1, z1) = cell_of(max - Vec2::splat(0.01));
        for z in z0..=z1 {
            for x in x0..=x1 {
                let c = &mut self.extra[z * DIM + x];
                *c = c.saturating_add(cost);
            }
        }
    }

    /// Recompute the field from weighted seeds (cell position, starting cost).
    pub fn recompute(&mut self, seeds: &[(Vec2, u32)]) {
        self.dist.iter_mut().for_each(|d| *d = u32::MAX);
        self.heap.clear();
        for &(p, c) in seeds {
            let (x, z) = cell_of(p);
            let i = z * DIM + x;
            if c < self.dist[i] {
                self.dist[i] = c;
                self.heap.push(Reverse((c, i as u32)));
            }
        }
        let dim = DIM as i32;
        while let Some(Reverse((d, idx))) = self.heap.pop() {
            let i = idx as usize;
            if d > self.dist[i] {
                continue;
            }
            let (x, z) = ((i % DIM) as i32, (i / DIM) as i32);
            for &(dx, dz, w) in &NEIGH {
                let (nx, nz) = (x + dx, z + dz);
                if nx < 0 || nz < 0 || nx >= dim || nz >= dim {
                    continue;
                }
                let ni = (nz * dim + nx) as usize;
                if self.blocked[ni] {
                    continue;
                }
                if dx != 0 && dz != 0 {
                    // No squeezing diagonally past corners.
                    if self.blocked[(z * dim + nx) as usize] || self.blocked[(nz * dim + x) as usize] {
                        continue;
                    }
                }
                let nd = d + w + self.extra[ni] as u32;
                if nd < self.dist[ni] {
                    self.dist[ni] = nd;
                    self.heap.push(Reverse((nd, ni as u32)));
                }
            }
        }
        // Each cell's flow points to its lowest neighbour.
        for z in 0..dim {
            for x in 0..dim {
                let i = (z * dim + x) as usize;
                let mut best = self.dist[i];
                let mut dir = [0i8, 0i8];
                for &(dx, dz, _) in &NEIGH {
                    let (nx, nz) = (x + dx, z + dz);
                    if nx < 0 || nz < 0 || nx >= dim || nz >= dim {
                        continue;
                    }
                    if dx != 0 && dz != 0 && (self.blocked[(z * dim + nx) as usize] || self.blocked[(nz * dim + x) as usize]) {
                        continue;
                    }
                    let nd = self.dist[(nz * dim + nx) as usize];
                    if nd < best {
                        best = nd;
                        dir = [dx as i8, dz as i8];
                    }
                }
                self.flow[i] = dir;
            }
        }
    }

    /// Smoothed flow direction at a world position (bilinear over the four nearest cells).
    pub fn direction(&self, p: Vec2) -> Vec2 {
        let (x, z) = cell_of(p);
        let f = self.flow[z * DIM + x];
        let (fx, fz) = (p.x + HALF - x as f32 - 0.5, p.y + HALF - z as f32 - 0.5);
        let (ox, oz) = (if fx > 0.0 { 1i32 } else { -1 }, if fz > 0.0 { 1i32 } else { -1 });
        let get = |ax: i32, az: i32| -> Option<Vec2> {
            let (nx, nz) = (x as i32 + ax, z as i32 + az);
            if nx < 0 || nz < 0 || nx >= DIM as i32 || nz >= DIM as i32 {
                return None;
            }
            let i = (nz as usize) * DIM + nx as usize;
            if self.blocked[i] {
                return None;
            }
            let v = self.flow[i];
            Some(Vec2::new(v[0] as f32, v[1] as f32).normalize_or_zero())
        };
        let base = Vec2::new(f[0] as f32, f[1] as f32).normalize_or_zero();
        let (wx, wz) = (fx.abs(), fz.abs());
        let mut sum = base * (1.0 - wx) * (1.0 - wz);
        sum += get(ox, 0).unwrap_or(base) * wx * (1.0 - wz);
        sum += get(0, oz).unwrap_or(base) * (1.0 - wx) * wz;
        sum += get(ox, oz).unwrap_or(base) * wx * wz;
        sum.normalize_or_zero()
    }

    pub fn reachable(&self, p: Vec2) -> bool {
        let (x, z) = cell_of(p);
        self.dist[z * DIM + x] != u32::MAX
    }

    pub fn is_blocked_cell(&self, p: Vec2) -> bool {
        let (x, z) = cell_of(p);
        self.blocked[z * DIM + x]
    }
}
