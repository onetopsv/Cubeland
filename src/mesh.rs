//! Dựng mesh cho một chunk: chỉ vẽ mặt tiếp giáp không khí.
//! Đỉnh gồm 6 f32: x y z r g b (đã nhân sẵn độ sáng theo mặt).

use crate::world::{hash3, Block, World, CH, CS};

pub const FLOATS_PER_VERTEX: usize = 6;

const FACES: [([i32; 3], [[f32; 3]; 4], f32); 6] = [
    ([1, 0, 0], [[1., 0., 0.], [1., 1., 0.], [1., 1., 1.], [1., 0., 1.]], 0.80),
    ([-1, 0, 0], [[0., 0., 1.], [0., 1., 1.], [0., 1., 0.], [0., 0., 0.]], 0.80),
    ([0, 1, 0], [[0., 1., 0.], [0., 1., 1.], [1., 1., 1.], [1., 1., 0.]], 1.00),
    ([0, -1, 0], [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]], 0.50),
    ([0, 0, 1], [[1., 0., 1.], [1., 1., 1.], [0., 1., 1.], [0., 0., 1.]], 0.65),
    ([0, 0, -1], [[0., 0., 0.], [0., 1., 0.], [1., 1., 0.], [1., 0., 0.]], 0.65),
];

pub fn build_chunk_mesh(world: &World, cx: i32, cz: i32) -> (Vec<f32>, Vec<u32>) {
    let mut verts: Vec<f32> = Vec::new();
    let mut idxs: Vec<u32> = Vec::new();
    let (ox, oz) = (cx * CS, cz * CS);

    for lx in 0..CS {
        for lz in 0..CS {
            for y in 0..CH {
                let (x, z) = (ox + lx, oz + lz);
                let b = world.get_block(x, y, z);
                if b == Block::Air {
                    continue;
                }
                // Biến thiên nhẹ độ sáng từng block cho đỡ phẳng.
                let var = 0.93 + 0.07 * hash3(x, y, z, 99);
                for (fi, (n, corners, shade)) in FACES.iter().enumerate() {
                    if world.get_block(x + n[0], y + n[1], z + n[2]).solid() {
                        continue;
                    }
                    let base = b.color(fi);
                    let k = shade * var;
                    let col = [base[0] * k, base[1] * k, base[2] * k];
                    let start = (verts.len() / FLOATS_PER_VERTEX) as u32;
                    for c in corners {
                        verts.extend_from_slice(&[
                            x as f32 + c[0],
                            y as f32 + c[1],
                            z as f32 + c[2],
                            col[0],
                            col[1],
                            col[2],
                        ]);
                    }
                    idxs.extend_from_slice(&[start, start + 1, start + 2, start, start + 2, start + 3]);
                }
            }
        }
    }
    (verts, idxs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> World {
        let mut w = World::new(42);
        for cx in -2..=2 {
            for cz in -2..=2 {
                w.ensure_chunk(cx, cz);
            }
        }
        w
    }

    #[test]
    fn mesh_is_non_empty_and_consistent() {
        let w = world();
        let (v, i) = build_chunk_mesh(&w, 0, 0);
        assert!(!v.is_empty());
        assert_eq!(v.len() % (FLOATS_PER_VERTEX * 4), 0);
        assert_eq!(i.len() % 6, 0);
        assert_eq!(i.len() / 6, v.len() / (FLOATS_PER_VERTEX * 4));
        let nverts = (v.len() / FLOATS_PER_VERTEX) as u32;
        assert!(i.iter().all(|&k| k < nverts));
    }

    #[test]
    fn hidden_faces_are_culled() {
        // Một khối 2x1x1 đặt trên không: 10 mặt, không phải 12.
        let mut w = World::new(1);
        w.ensure_chunk(0, 0);
        w.ensure_chunk(-1, 0);
        w.ensure_chunk(1, 0);
        w.ensure_chunk(0, -1);
        w.ensure_chunk(0, 1);
        let before = build_chunk_mesh(&w, 0, 0).1.len() / 6;
        w.set_block(4, 60, 4, Block::Stone);
        w.set_block(5, 60, 4, Block::Stone);
        let after = build_chunk_mesh(&w, 0, 0).1.len() / 6;
        assert_eq!(after - before, 10);
    }

    #[test]
    fn breaking_block_changes_mesh() {
        let mut w = world();
        let top = w.column_top(8, 8);
        let before = build_chunk_mesh(&w, 0, 0).1.len();
        w.set_block(8, top, 8, Block::Air);
        let after = build_chunk_mesh(&w, 0, 0).1.len();
        assert_ne!(before, after);
    }

    #[test]
    fn faces_point_outward() {
        // Pháp tuyến hình học của mỗi mặt (từ 3 đỉnh đầu) phải cùng hướng với n.
        for (n, c, _) in FACES.iter() {
            let a = [c[1][0] - c[0][0], c[1][1] - c[0][1], c[1][2] - c[0][2]];
            let b = [c[2][0] - c[0][0], c[2][1] - c[0][1], c[2][2] - c[0][2]];
            let cr = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let d = cr[0] * n[0] as f32 + cr[1] * n[1] as f32 + cr[2] * n[2] as f32;
            assert!(d > 0.0, "mặt {:?} quay ngược", n);
        }
    }
}
