//! Thế giới voxel: block, chunk, sinh địa hình, raycast.

use std::collections::{HashMap, HashSet};

pub const CS: i32 = 16; // cạnh chunk (x và z)
pub const CH: i32 = 64; // chiều cao thế giới
pub const SEA: i32 = 24; // mực nước biển: thấp hơn thì là cát
const VOL: usize = (CS * CS * CH) as usize;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Block {
    Air = 0,
    Grass = 1,
    Dirt = 2,
    Stone = 3,
    Sand = 4,
    Wood = 5,
    Leaves = 6,
    Planks = 7,
}

impl Block {
    /// Các block người chơi có thể đặt (chạm góc trên-phải để đổi).
    pub const PLACEABLE: [Block; 6] = [
        Block::Stone,
        Block::Dirt,
        Block::Planks,
        Block::Wood,
        Block::Sand,
        Block::Leaves,
    ];

    pub fn from_u8(v: u8) -> Block {
        match v {
            1 => Block::Grass,
            2 => Block::Dirt,
            3 => Block::Stone,
            4 => Block::Sand,
            5 => Block::Wood,
            6 => Block::Leaves,
            7 => Block::Planks,
            _ => Block::Air,
        }
    }

    pub fn solid(self) -> bool {
        self != Block::Air
    }

    /// Màu theo mặt: 0:+X 1:-X 2:+Y 3:-Y 4:+Z 5:-Z
    pub fn color(self, face: usize) -> [f32; 3] {
        match self {
            Block::Air => [0.0, 0.0, 0.0],
            Block::Grass => match face {
                2 => [0.36, 0.62, 0.25],
                3 => [0.47, 0.34, 0.22],
                _ => [0.42, 0.48, 0.24],
            },
            Block::Dirt => [0.47, 0.34, 0.22],
            Block::Stone => [0.50, 0.50, 0.52],
            Block::Sand => [0.86, 0.80, 0.55],
            Block::Wood => {
                if face == 2 || face == 3 {
                    [0.55, 0.42, 0.24]
                } else {
                    [0.40, 0.28, 0.15]
                }
            }
            Block::Leaves => [0.20, 0.50, 0.18],
            Block::Planks => [0.70, 0.55, 0.32],
        }
    }
}

pub struct Chunk {
    pub blocks: Vec<u8>,
}

fn idx(x: usize, y: usize, z: usize) -> usize {
    (y * 16 + z) * 16 + x
}

// ---------- noise ----------

pub fn hash(x: i32, z: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374_761_393)
        ^ (z as u32).wrapping_mul(668_265_263)
        ^ seed.wrapping_mul(2_246_822_519);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0xFF_FFFF) as f32 / 16_777_215.0
}

pub fn hash3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    hash(x.wrapping_add(y.wrapping_mul(7919)), z, seed)
}

fn value_noise(seed: u32, x: f32, z: f32) -> f32 {
    let xi = x.floor();
    let zi = z.floor();
    let fx = x - xi;
    let fz = z - zi;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let (xi, zi) = (xi as i32, zi as i32);
    let a = hash(xi, zi, seed);
    let b = hash(xi + 1, zi, seed);
    let c = hash(xi, zi + 1, seed);
    let d = hash(xi + 1, zi + 1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sz
}

fn fbm(seed: u32, x: f32, z: f32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for i in 0..4u32 {
        sum += amp * value_noise(seed.wrapping_add(i * 1013), x * freq, z * freq);
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

pub fn terrain_height(seed: u32, x: i32, z: i32) -> i32 {
    let n = fbm(seed, x as f32 / 40.0, z as f32 / 40.0);
    ((16.0 + n * 30.0) as i32).clamp(2, CH - 10)
}

fn generate_chunk(seed: u32, cx: i32, cz: i32) -> Chunk {
    let mut blocks = vec![0u8; VOL];
    for lx in 0..16usize {
        for lz in 0..16usize {
            let wx = cx * CS + lx as i32;
            let wz = cz * CS + lz as i32;
            let h = terrain_height(seed, wx, wz);
            for y in 0..=h {
                let b = if y == h {
                    if h > SEA { Block::Grass } else { Block::Sand }
                } else if y >= h - 3 {
                    if h > SEA { Block::Dirt } else { Block::Sand }
                } else {
                    Block::Stone
                };
                blocks[idx(lx, y as usize, lz)] = b as u8;
            }
        }
    }
    // Cây: chỉ đặt cách mép chunk 3 ô để tán lá không tràn sang chunk khác.
    for lx in 3..13usize {
        for lz in 3..13usize {
            let wx = cx * CS + lx as i32;
            let wz = cz * CS + lz as i32;
            let h = terrain_height(seed, wx, wz);
            if h > SEA + 1 && hash(wx, wz, seed ^ 0xABCD) > 0.985 {
                let trunk = 4 + (hash(wx, wz, seed ^ 0x77) * 2.0) as i32;
                for t in 1..=trunk {
                    blocks[idx(lx, (h + t) as usize, lz)] = Block::Wood as u8;
                }
                for dy in (trunk - 1)..=(trunk + 1) {
                    let r = if dy == trunk + 1 { 1 } else { 2 };
                    for dx in -r..=r {
                        for dz in -r..=r {
                            let (x, z) = (lx as i32 + dx, lz as i32 + dz);
                            let y = h + dy;
                            if y >= CH {
                                continue;
                            }
                            let i = idx(x as usize, y as usize, z as usize);
                            if blocks[i] == 0 {
                                blocks[i] = Block::Leaves as u8;
                            }
                        }
                    }
                }
            }
        }
    }
    Chunk { blocks }
}

// ---------- world ----------

pub struct World {
    pub seed: u32,
    chunks: HashMap<(i32, i32), Chunk>,
    /// Chunk bị sửa, cần dựng lại mesh.
    pub dirty: HashSet<(i32, i32)>,
}

fn split(x: i32, z: i32) -> ((i32, i32), (usize, usize)) {
    (
        (x.div_euclid(CS), z.div_euclid(CS)),
        (x.rem_euclid(CS) as usize, z.rem_euclid(CS) as usize),
    )
}

impl World {
    pub fn new(seed: u32) -> World {
        World { seed, chunks: HashMap::new(), dirty: HashSet::new() }
    }

    pub fn has_chunk(&self, cx: i32, cz: i32) -> bool {
        self.chunks.contains_key(&(cx, cz))
    }

    pub fn ensure_chunk(&mut self, cx: i32, cz: i32) {
        if !self.chunks.contains_key(&(cx, cz)) {
            let c = generate_chunk(self.seed, cx, cz);
            self.chunks.insert((cx, cz), c);
        }
    }

    /// Chunk chưa tải coi như Air (dùng cho mesh/raycast).
    pub fn get_block(&self, x: i32, y: i32, z: i32) -> Block {
        if y < 0 {
            return Block::Stone;
        }
        if y >= CH {
            return Block::Air;
        }
        let (k, (lx, lz)) = split(x, z);
        match self.chunks.get(&k) {
            Some(c) => Block::from_u8(c.blocks[idx(lx, y as usize, lz)]),
            None => Block::Air,
        }
    }

    /// Dùng cho va chạm: chunk chưa tải coi như đặc để người chơi không rơi xuyên.
    pub fn collision_solid(&self, x: i32, y: i32, z: i32) -> bool {
        if y < 0 {
            return true;
        }
        if y >= CH {
            return false;
        }
        let (k, (lx, lz)) = split(x, z);
        match self.chunks.get(&k) {
            Some(c) => c.blocks[idx(lx, y as usize, lz)] != 0,
            None => true,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, b: Block) -> bool {
        if y < 0 || y >= CH {
            return false;
        }
        let (k, (lx, lz)) = split(x, z);
        let Some(c) = self.chunks.get_mut(&k) else {
            return false;
        };
        c.blocks[idx(lx, y as usize, lz)] = b as u8;
        self.dirty.insert(k);
        if lx == 0 {
            self.dirty.insert((k.0 - 1, k.1));
        }
        if lx == 15 {
            self.dirty.insert((k.0 + 1, k.1));
        }
        if lz == 0 {
            self.dirty.insert((k.0, k.1 - 1));
        }
        if lz == 15 {
            self.dirty.insert((k.0, k.1 + 1));
        }
        true
    }

    /// Y của block đặc cao nhất tại cột (x, z); -1 nếu cột trống.
    pub fn column_top(&self, x: i32, z: i32) -> i32 {
        for y in (0..CH).rev() {
            if self.get_block(x, y, z).solid() {
                return y;
            }
        }
        -1
    }

    /// Bắn tia (DDA). Trả về (ô bị trúng, ô trống ngay trước nó).
    pub fn raycast(&self, o: [f32; 3], d: [f32; 3], max: f32) -> Option<([i32; 3], [i32; 3])> {
        let mut cell = [o[0].floor() as i32, o[1].floor() as i32, o[2].floor() as i32];
        let mut prev = cell;
        let mut step = [0i32; 3];
        let mut tmax = [f32::INFINITY; 3];
        let mut tdelta = [f32::INFINITY; 3];
        for i in 0..3 {
            if d[i] > 0.0 {
                step[i] = 1;
                tdelta[i] = 1.0 / d[i];
                tmax[i] = ((cell[i] + 1) as f32 - o[i]) / d[i];
            } else if d[i] < 0.0 {
                step[i] = -1;
                tdelta[i] = -1.0 / d[i];
                tmax[i] = (o[i] - cell[i] as f32) / -d[i];
            }
        }
        loop {
            if self.get_block(cell[0], cell[1], cell[2]).solid() {
                return Some((cell, prev));
            }
            let mut a = 0;
            if tmax[1] < tmax[a] {
                a = 1;
            }
            if tmax[2] < tmax[a] {
                a = 2;
            }
            if tmax[a] > max {
                return None;
            }
            prev = cell;
            cell[a] += step[a];
            tmax[a] += tdelta[a];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_around_origin() -> World {
        let mut w = World::new(42);
        for cx in -2..=2 {
            for cz in -2..=2 {
                w.ensure_chunk(cx, cz);
            }
        }
        w
    }

    #[test]
    fn generation_is_deterministic() {
        let a = generate_chunk(7, 3, -4);
        let b = generate_chunk(7, 3, -4);
        assert_eq!(a.blocks, b.blocks);
    }

    #[test]
    fn terrain_has_variation_and_bedrock_is_solid() {
        let w = world_around_origin();
        let mut lo = i32::MAX;
        let mut hi = i32::MIN;
        for x in -30..30 {
            for z in -30..30 {
                let t = w.column_top(x, z);
                lo = lo.min(t);
                hi = hi.max(t);
                assert!(w.get_block(x, 0, z).solid());
            }
        }
        assert!(hi - lo >= 5, "địa hình quá phẳng: {}..{}", lo, hi);
    }

    #[test]
    fn raycast_down_hits_ground() {
        let w = world_around_origin();
        let top = w.column_top(5, 5);
        let (hit, prev) = w.raycast([5.5, 60.0, 5.5], [0.0, -1.0, 0.0], 100.0).unwrap();
        assert_eq!(hit, [5, top, 5]);
        assert_eq!(prev, [5, top + 1, 5]);
    }

    #[test]
    fn raycast_misses_when_too_far() {
        let w = world_around_origin();
        assert!(w.raycast([5.5, 60.0, 5.5], [0.0, -1.0, 0.0], 3.0).is_none());
    }

    #[test]
    fn raycast_diagonal_and_negative_coords() {
        let w = world_around_origin();
        let r = w.raycast([-3.2, 62.0, -7.7], [0.3, -1.0, 0.2], 100.0);
        let (hit, prev) = r.expect("phải trúng đất");
        assert!(w.get_block(hit[0], hit[1], hit[2]).solid());
        assert!(!w.get_block(prev[0], prev[1], prev[2]).solid());
    }

    #[test]
    fn set_block_marks_neighbor_dirty_on_border() {
        let mut w = world_around_origin();
        assert!(w.set_block(0, 10, 3, Block::Air));
        assert!(w.dirty.contains(&(0, 0)));
        assert!(w.dirty.contains(&(-1, 0)));
        assert_eq!(w.get_block(0, 10, 3), Block::Air);
        assert!(!w.set_block(1000, 10, 3, Block::Air)); // chunk chưa tải
    }

    #[test]
    fn negative_coordinates_map_to_correct_chunk() {
        let mut w = world_around_origin();
        assert!(w.set_block(-1, 5, -1, Block::Planks));
        assert_eq!(w.get_block(-1, 5, -1), Block::Planks);
        assert!(w.dirty.contains(&(-1, -1)));
    }
}
