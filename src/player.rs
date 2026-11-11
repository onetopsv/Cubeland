//! Người chơi: di chuyển, trọng lực, va chạm AABB.

use crate::world::World;

const HALF_W: f32 = 0.3;
const HEIGHT: f32 = 1.8;
const EYE: f32 = 1.62;
const SPEED: f32 = 4.6;
const GRAVITY: f32 = 28.0;
const JUMP: f32 = 9.0;
const MAX_FALL: f32 = 15.0;

pub struct Player {
    pub pos: [f32; 3], // vị trí chân
    pub vel_y: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub on_ground: bool,
    pub sneaking: bool,
}

impl Player {
    pub fn new(pos: [f32; 3]) -> Player {
        Player { pos, vel_y: 0.0, yaw: 0.0, pitch: 0.0, on_ground: false, sneaking: false }
    }

    pub fn eye(&self) -> [f32; 3] {
        let drop = if self.sneaking { 0.3 } else { 0.0 };
        [self.pos[0], self.pos[1] + EYE - drop, self.pos[2]]
    }

    /// yaw = 0 nhìn về -Z; yaw tăng thì quay sang phải.
    pub fn dir(&self) -> [f32; 3] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [sy * cp, sp, -cy * cp]
    }

    fn collides(world: &World, p: [f32; 3]) -> bool {
        let x0 = (p[0] - HALF_W).floor() as i32;
        let x1 = (p[0] + HALF_W - 1e-4).floor() as i32;
        let y0 = p[1].floor() as i32;
        let y1 = (p[1] + HEIGHT - 1e-4).floor() as i32;
        let z0 = (p[2] - HALF_W).floor() as i32;
        let z1 = (p[2] + HALF_W - 1e-4).floor() as i32;
        for x in x0..=x1 {
            for y in y0..=y1 {
                for z in z0..=z1 {
                    if world.collision_solid(x, y, z) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Hộp va chạm của người chơi có chồng lên ô [x,y,z] không.
    pub fn intersects_block(&self, c: [i32; 3]) -> bool {
        let p = self.pos;
        p[0] - HALF_W < (c[0] + 1) as f32
            && p[0] + HALF_W > c[0] as f32
            && p[1] < (c[1] + 1) as f32
            && p[1] + HEIGHT > c[1] as f32
            && p[2] - HALF_W < (c[2] + 1) as f32
            && p[2] + HALF_W > c[2] as f32
    }

    /// wish = [phải, tiến] trong khoảng -1..1.
    pub fn update(&mut self, world: &World, dt: f32, wish: [f32; 2], jump: bool) {
        let dt = dt.min(0.05);
        let (s, c) = self.yaw.sin_cos();
        let fwd = [s, -c];
        let right = [c, s];
        let speed = if self.sneaking { SPEED * 0.35 } else { SPEED };
        let vx = (right[0] * wish[0] + fwd[0] * wish[1]) * speed;
        let vz = (right[1] * wish[0] + fwd[1] * wish[1]) * speed;

        let mut blocked = false;
        let nx = self.pos[0] + vx * dt;
        if !Self::collides(world, [nx, self.pos[1], self.pos[2]]) {
            self.pos[0] = nx;
        } else if vx != 0.0 {
            blocked = true;
        }
        let nz = self.pos[2] + vz * dt;
        if !Self::collides(world, [self.pos[0], self.pos[1], nz]) {
            self.pos[2] = nz;
        } else if vz != 0.0 {
            blocked = true;
        }

        // Tự nhảy qua bậc cao 1 ô khi đang đi vào tường.
        let mut want_jump = jump;
        if blocked && self.on_ground {
            let ahead = [self.pos[0] + vx * dt * 2.0, self.pos[1] + 1.0, self.pos[2] + vz * dt * 2.0];
            if !Self::collides(world, ahead) {
                want_jump = true;
            }
        }
        if want_jump && self.on_ground {
            self.vel_y = JUMP;
            self.on_ground = false;
        }

        self.vel_y = (self.vel_y - GRAVITY * dt).max(-MAX_FALL);
        let ny = self.pos[1] + self.vel_y * dt;
        if !Self::collides(world, [self.pos[0], ny, self.pos[2]]) {
            self.pos[1] = ny;
            self.on_ground = false;
        } else {
            if self.vel_y < 0.0 {
                self.on_ground = true;
                self.pos[1] = self.pos[1].floor(); // dính đúng mặt đất
            }
            self.vel_y = 0.0;
        }
    }
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
    fn falls_and_lands_on_ground() {
        let w = world();
        let top = w.column_top(8, 8);
        let mut p = Player::new([8.5, (top + 10) as f32, 8.5]);
        for _ in 0..300 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0], false);
        }
        assert!(p.on_ground);
        assert!((p.pos[1] - (top + 1) as f32).abs() < 1e-3, "y={} top={}", p.pos[1], top);
    }

    #[test]
    fn jump_goes_up_then_lands() {
        let w = world();
        let top = w.column_top(8, 8);
        let mut p = Player::new([8.5, (top + 1) as f32, 8.5]);
        for _ in 0..30 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0], false);
        }
        let base = p.pos[1];
        p.update(&w, 1.0 / 60.0, [0.0, 0.0], true);
        let mut peak = base;
        for _ in 0..120 {
            p.update(&w, 1.0 / 60.0, [0.0, 0.0], false);
            peak = peak.max(p.pos[1]);
        }
        assert!(peak - base > 1.0, "độ cao nhảy {}", peak - base);
        assert!(p.on_ground);
    }

    #[test]
    fn cannot_walk_through_wall() {
        let mut w = world();
        let top = w.column_top(8, 8);
        // Tường cao 3 ô cách 2 ô về phía -Z
        for y in 1..=3 {
            w.set_block(8, top + y, 6, crate::world::Block::Stone);
            w.set_block(9, top + y, 6, crate::world::Block::Stone);
            w.set_block(7, top + y, 6, crate::world::Block::Stone);
        }
        let mut p = Player::new([8.5, (top + 1) as f32, 8.5]);
        for _ in 0..300 {
            p.update(&w, 1.0 / 60.0, [0.0, 1.0], false); // yaw 0: đi về -Z
        }
        assert!(p.pos[2] > 7.0, "xuyên tường: z={}", p.pos[2]);
    }

    #[test]
    fn walking_forward_moves_toward_minus_z() {
        let w = world();
        let top = w.column_top(8, 8);
        let mut p = Player::new([8.5, (top + 1) as f32, 8.5]);
        let z0 = p.pos[2];
        for _ in 0..10 {
            p.update(&w, 1.0 / 60.0, [0.0, 1.0], false);
        }
        assert!(p.pos[2] < z0);
    }

    #[test]
    fn sneaking_is_slower_and_lowers_the_eye() {
        let w = world();
        let top = w.column_top(8, 8);
        let start = [8.5, (top + 1) as f32, 8.5];
        let mut a = Player::new(start);
        let mut b = Player::new(start);
        b.sneaking = true;
        for _ in 0..60 {
            a.update(&w, 1.0 / 60.0, [0.0, 1.0], false);
            b.update(&w, 1.0 / 60.0, [0.0, 1.0], false);
        }
        assert!((start[2] - b.pos[2]) < (start[2] - a.pos[2]) * 0.5);
        // So độ cao mắt tính từ chân (hai người đứng ở độ cao đất khác nhau).
        assert!((b.eye()[1] - b.pos[1]) < (a.eye()[1] - a.pos[1]) - 0.2);
    }

    #[test]
    fn dir_matches_yaw_convention() {
        let mut p = Player::new([0.0; 3]);
        let d = p.dir();
        assert!((d[2] + 1.0).abs() < 1e-5);
        p.yaw = std::f32::consts::FRAC_PI_2; // quay phải 90° → nhìn về +X
        let d = p.dir();
        assert!((d[0] - 1.0).abs() < 1e-5);
    }
}
