//! Trạng thái điều khiển cảm ứng + dựng HUD (nút, chữ, tâm ngắm).
//! Không phụ thuộc wgpu/winit nên test được.

use std::collections::HashMap;

use crate::font;
use crate::ui::{self, Action, Button};
use crate::world::Block;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Start,
    Move,
    End,
}

pub struct TouchInfo {
    pub cur: [f32; 2],
    /// Chỉ số nút đang giữ; None = ngón này đang xoay camera.
    pub button: Option<usize>,
}

pub struct Controls {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub buttons: Vec<Button>,
    pub touches: HashMap<u64, TouchInfo>,

    pub gui_hidden: bool,
    pub debug: bool,
    /// 0 = góc nhìn thứ nhất, 1 = sau lưng, 2 = phía trước mặt
    pub third: u8,
    pub mouse_mode: bool,
    pub paused: bool,
    pub sneaking: bool,
    pub selected: usize,

    pub break_req: bool,
    pub place_req: bool,
    pub toast: Option<(String, f32)>,
    pri_t: f32,
    sec_t: f32,
}

const REPEAT: f32 = 0.28;

impl Controls {
    pub fn new(width: f32, height: f32, scale: f32) -> Controls {
        Controls {
            width,
            height,
            scale,
            buttons: ui::layout(width, height, scale),
            touches: HashMap::new(),
            gui_hidden: false,
            debug: false,
            third: 0,
            mouse_mode: false,
            paused: false,
            sneaking: false,
            selected: 0,
            break_req: false,
            place_req: false,
            toast: None,
            pri_t: 0.0,
            sec_t: 0.0,
        }
    }

    pub fn resize(&mut self, width: f32, height: f32, scale: f32) {
        self.width = width;
        self.height = height;
        self.scale = scale;
        self.buttons = ui::layout(width, height, scale);
    }

    pub fn say(&mut self, msg: &str) {
        self.toast = Some((msg.to_string(), 2.0));
    }

    pub fn selected_block(&self) -> Block {
        Block::PLACEABLE[self.selected % Block::PLACEABLE.len()]
    }

    /// Nút bấm một lần (hoặc bắt đầu nhấn giữ).
    pub fn press(&mut self, a: Action) {
        match a {
            Action::Primary => self.break_req = true,
            Action::Secondary => self.place_req = true,
            Action::Sneak => self.sneaking = !self.sneaking,
            Action::Gui => self.gui_hidden = !self.gui_hidden,
            Action::Debug => self.debug = !self.debug,
            Action::Third => self.third = (self.third + 1) % 3,
            Action::Esc => self.paused = !self.paused,
            Action::Mouse => {
                self.mouse_mode = !self.mouse_mode;
                self.say(if self.mouse_mode { "MOUSE MODE: ON" } else { "MOUSE MODE: OFF" });
            }
            Action::Inv => {
                // Chưa có túi đồ thật: tạm dùng để đổi loại block đang cầm.
                self.selected = (self.selected + 1) % Block::PLACEABLE.len();
                let msg = format!("BLOCK: {}", self.selected_block().name());
                self.say(&msg);
            }
            Action::Chat => self.say("CHAT: NEEDS SERVER"),
            Action::Keyboard => self.say("KEYBOARD: NOT YET"),
            Action::Tab => self.say("PLAYER LIST: NEEDS SERVER"),
            Action::Forward
            | Action::Back
            | Action::Left
            | Action::Right
            | Action::Jump
            | Action::Zoom => {}
        }
    }

    /// Xử lý một sự kiện chạm. Trả về độ dời (dx, dy) pixel nếu ngón này đang xoay camera.
    pub fn touch(&mut self, id: u64, phase: Phase, p: [f32; 2]) -> Option<[f32; 2]> {
        match phase {
            Phase::Start => {
                let hit = ui::hit(&self.buttons, p[0], p[1])
                    .filter(|&i| !self.gui_hidden || self.buttons[i].action == Action::Gui);
                if let Some(i) = hit {
                    let a = self.buttons[i].action;
                    self.press(a);
                }
                self.touches.insert(id, TouchInfo { cur: p, button: hit });
                None
            }
            Phase::Move => {
                let look_ok = !self.mouse_mode && !self.paused;
                let ti = self.touches.get_mut(&id)?;
                let d = [p[0] - ti.cur[0], p[1] - ti.cur[1]];
                ti.cur = p;
                if ti.button.is_none() && look_ok {
                    Some(d)
                } else {
                    None
                }
            }
            Phase::End => {
                self.touches.remove(&id);
                None
            }
        }
    }

    pub fn held(&self, a: Action) -> bool {
        self.touches
            .values()
            .any(|t| t.button.map_or(false, |i| self.buttons[i].action == a))
    }

    /// [phải, tiến] trong -1..1 từ bốn nút mũi tên.
    pub fn wish(&self) -> [f32; 2] {
        let axis = |pos: Action, neg: Action| (self.held(pos) as i32 - self.held(neg) as i32) as f32;
        let (x, y) = (axis(Action::Right, Action::Left), axis(Action::Forward, Action::Back));
        let len = (x * x + y * y).sqrt();
        if len > 1.0 {
            [x / len, y / len]
        } else {
            [x, y]
        }
    }

    pub fn jump(&self) -> bool {
        self.held(Action::Jump)
    }

    pub fn zoom(&self) -> bool {
        self.held(Action::Zoom)
    }

    /// Gọi mỗi frame: tự lặp phá/đặt khi giữ nút, và đếm ngược thông báo.
    pub fn tick(&mut self, dt: f32) {
        if self.held(Action::Primary) {
            self.pri_t += dt;
            if self.pri_t >= REPEAT {
                self.pri_t = 0.0;
                self.break_req = true;
            }
        } else {
            self.pri_t = 0.0;
        }
        if self.held(Action::Secondary) {
            self.sec_t += dt;
            if self.sec_t >= REPEAT {
                self.sec_t = 0.0;
                self.place_req = true;
            }
        } else {
            self.sec_t = 0.0;
        }
        if let Some((_, t)) = &mut self.toast {
            *t -= dt;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
    }

    // ---------- HUD ----------

    /// Trả về (nền nút trong suốt, phần vẽ đặc). Mỗi đỉnh: x y z r g b, tọa độ NDC.
    pub fn hud(&self, debug_lines: &[String]) -> (Vec<f32>, Vec<f32>) {
        let (w, h, s) = (self.width, self.height, self.scale);
        let u = ui::text_unit(s);
        let white = [1.0, 1.0, 1.0];
        let mut bg: Vec<f32> = Vec::new();
        let mut fg: Vec<f32> = Vec::new();

        for (i, b) in self.buttons.iter().enumerate() {
            if self.gui_hidden && b.action != Action::Gui {
                continue;
            }
            let pressed = self.touches.values().any(|t| t.button == Some(i));
            let on = match b.action {
                Action::Sneak => self.sneaking,
                Action::Gui => self.gui_hidden,
                Action::Debug => self.debug,
                Action::Third => self.third != 0,
                Action::Mouse => self.mouse_mode,
                Action::Esc => self.paused,
                _ => false,
            };
            let col = if pressed {
                [0.9, 0.9, 0.9]
            } else if on {
                [0.3, 0.7, 0.3]
            } else {
                [0.15, 0.15, 0.15]
            };
            let r = b.rect;
            quad(&mut bg, w, h, r.x, r.y, r.x + r.w, r.y + r.h, col);
            let lu = ui::label_unit(b.label, s);
            let tw = font::text_width_units(b.label) as f32 * lu;
            let txt_col = if pressed { [0.1, 0.1, 0.1] } else { white };
            text(&mut fg, w, h, b.label, r.x + (r.w - tw) / 2.0, r.y + (r.h - 7.0 * lu) / 2.0, lu, txt_col);
        }

        if !self.gui_hidden {
            // Tâm ngắm (chỉ ở góc nhìn thứ nhất)
            if self.third == 0 {
                let (cx, cy) = (w * 0.5, h * 0.5);
                quad(&mut fg, w, h, cx - 10.0 * s, cy - s, cx + 10.0 * s, cy + s, white);
                quad(&mut fg, w, h, cx - s, cy - 10.0 * s, cx + s, cy + 10.0 * s, white);
            }
            // Ô block đang cầm, giữa phía dưới
            let size = 34.0 * s;
            let (x0, y0) = (w * 0.5 - size / 2.0, h - 22.0 * s - size);
            quad(&mut fg, w, h, x0 - 2.0 * s, y0 - 2.0 * s, x0 + size + 2.0 * s, y0 + size + 2.0 * s, white);
            quad(&mut fg, w, h, x0, y0, x0 + size, y0 + size, self.selected_block().color(2));
            // Thông tin debug
            if self.debug {
                let mut y = 10.0 * s + 2.0 * (30.0 * s + 3.0 * s) + 8.0 * s;
                for line in debug_lines {
                    text(&mut fg, w, h, line, 48.0 * s, y, u, white);
                    y += 10.0 * u;
                }
            }
        }

        if let Some((msg, _)) = &self.toast {
            let tw = font::text_width_units(msg) as f32 * u;
            text(&mut fg, w, h, msg, (w - tw) / 2.0, h * 0.22, u, white);
        }
        if self.paused {
            let pu = u * 2.0;
            let tw = font::text_width_units("PAUSED") as f32 * pu;
            text(&mut fg, w, h, "PAUSED", (w - tw) / 2.0, h * 0.4, pu, white);
        }
        (bg, fg)
    }
}

/// Thêm một hình chữ nhật (2 tam giác) với tọa độ pixel, đổi sang NDC.
#[allow(clippy::too_many_arguments)]
fn quad(v: &mut Vec<f32>, w: f32, h: f32, x0: f32, y0: f32, x1: f32, y1: f32, c: [f32; 3]) {
    let (nx0, nx1) = (x0 / w * 2.0 - 1.0, x1 / w * 2.0 - 1.0);
    let (ny0, ny1) = (1.0 - y0 / h * 2.0, 1.0 - y1 / h * 2.0);
    for (x, y) in [(nx0, ny0), (nx1, ny0), (nx1, ny1), (nx0, ny0), (nx1, ny1), (nx0, ny1)] {
        v.extend_from_slice(&[x, y, 0.0, c[0], c[1], c[2]]);
    }
}

#[allow(clippy::too_many_arguments)]
fn text(v: &mut Vec<f32>, w: f32, h: f32, t: &str, x: f32, y: f32, unit: f32, c: [f32; 3]) {
    for (gx, gy, len) in font::text_runs(t) {
        let (px, py) = (x + gx as f32 * unit, y + gy as f32 * unit);
        quad(v, w, h, px, py, px + len as f32 * unit, py + unit, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls() -> Controls {
        Controls::new(2532.0, 1170.0, 3.0)
    }

    fn center(c: &Controls, a: Action) -> [f32; 2] {
        let r = c.buttons.iter().find(|b| b.action == a).unwrap().rect;
        [r.x + r.w / 2.0, r.y + r.h / 2.0]
    }

    #[test]
    fn forward_button_sets_wish_while_held() {
        let mut c = controls();
        assert_eq!(c.wish(), [0.0, 0.0]);
        let p = center(&c, Action::Forward);
        c.touch(1, Phase::Start, p);
        assert_eq!(c.wish(), [0.0, 1.0]);
        c.touch(1, Phase::End, p);
        assert_eq!(c.wish(), [0.0, 0.0]);
    }

    #[test]
    fn two_thumbs_can_move_and_jump() {
        let mut c = controls();
        let (f, j) = (center(&c, Action::Forward), center(&c, Action::Jump));
        c.touch(1, Phase::Start, f);
        c.touch(2, Phase::Start, j);
        assert_eq!(c.wish(), [0.0, 1.0]);
        assert!(c.jump());
        c.touch(2, Phase::End, j);
        assert!(!c.jump());
        assert_eq!(c.wish(), [0.0, 1.0]);
    }

    #[test]
    fn diagonal_wish_is_normalized() {
        let mut c = controls();
        c.touch(1, Phase::Start, center(&c, Action::Forward));
        c.touch(2, Phase::Start, center(&c, Action::Right));
        let w = c.wish();
        let len = (w[0] * w[0] + w[1] * w[1]).sqrt();
        assert!((len - 1.0).abs() < 1e-5);
    }

    #[test]
    fn toggle_buttons_flip_once_per_tap() {
        let mut c = controls();
        for a in [Action::Sneak, Action::Gui] {
            let p = center(&c, a);
            c.touch(1, Phase::Start, p);
            c.touch(1, Phase::End, p);
        }
        assert!(c.sneaking && c.gui_hidden);
        c.gui_hidden = false;
        let p = center(&c, Action::Debug);
        c.touch(1, Phase::Start, p);
        assert!(c.debug);
        c.touch(1, Phase::End, p);
        assert!(c.debug, "thả tay không được tắt lại");
    }

    #[test]
    fn third_person_cycles_through_three_modes() {
        let mut c = controls();
        c.press(Action::Third);
        assert_eq!(c.third, 1);
        c.press(Action::Third);
        c.press(Action::Third);
        assert_eq!(c.third, 0);
    }

    #[test]
    fn camera_drag_only_when_not_on_button() {
        let mut c = controls();
        let empty = [1266.0, 500.0];
        assert_eq!(c.touch(1, Phase::Start, empty), None);
        let d = c.touch(1, Phase::Move, [1276.0, 495.0]).unwrap();
        assert_eq!(d, [10.0, -5.0]);
        // Ngón đặt trên nút thì không xoay camera dù kéo đi
        let f = center(&c, Action::Forward);
        c.touch(2, Phase::Start, f);
        assert_eq!(c.touch(2, Phase::Move, [f[0] + 30.0, f[1]]), None);
    }

    #[test]
    fn mouse_mode_and_pause_disable_camera_drag() {
        let mut c = controls();
        c.touch(1, Phase::Start, [1266.0, 500.0]);
        c.press(Action::Mouse);
        assert_eq!(c.touch(1, Phase::Move, [1300.0, 500.0]), None);
        c.press(Action::Mouse);
        c.press(Action::Esc);
        assert_eq!(c.touch(1, Phase::Move, [1320.0, 500.0]), None);
    }

    #[test]
    fn primary_and_secondary_fire_then_repeat() {
        let mut c = controls();
        let p = center(&c, Action::Primary);
        c.touch(1, Phase::Start, p);
        assert!(c.break_req);
        c.break_req = false;
        c.tick(0.1);
        assert!(!c.break_req);
        c.tick(0.2);
        assert!(c.break_req, "giữ nút phải tự lặp");
        c.break_req = false;
        c.touch(1, Phase::End, p);
        c.tick(1.0);
        assert!(!c.break_req);

        let q = center(&c, Action::Secondary);
        c.touch(2, Phase::Start, q);
        assert!(c.place_req);
    }

    #[test]
    fn hidden_gui_ignores_other_buttons_but_gui_still_works() {
        let mut c = controls();
        c.press(Action::Gui);
        let f = center(&c, Action::Forward);
        c.touch(1, Phase::Start, f);
        assert_eq!(c.wish(), [0.0, 0.0]);
        let g = center(&c, Action::Gui);
        c.touch(2, Phase::Start, g);
        assert!(!c.gui_hidden);
    }

    #[test]
    fn inventory_button_cycles_block_and_shows_toast() {
        let mut c = controls();
        let first = c.selected_block();
        c.press(Action::Inv);
        assert_ne!(c.selected_block(), first);
        assert!(c.toast.is_some());
        c.tick(2.5);
        assert!(c.toast.is_none());
    }

    #[test]
    fn hud_vertices_are_well_formed_and_in_ndc() {
        let mut c = controls();
        c.debug = true;
        c.say("HELLO");
        c.touch(1, Phase::Start, center(&c, Action::Forward));
        let (bg, fg) = c.hud(&["XYZ: 1.0 2.0 3.0".to_string(), "FPS: 60".to_string()]);
        assert!(!bg.is_empty() && !fg.is_empty());
        assert_eq!(bg.len() % 36, 0, "mỗi hình chữ nhật = 6 đỉnh x 6 số");
        assert_eq!(fg.len() % 36, 0);
        for v in bg.chunks(6).chain(fg.chunks(6)) {
            assert!(v[0] >= -1.001 && v[0] <= 1.001 && v[1] >= -1.001 && v[1] <= 1.001, "ngoài NDC: {:?}", v);
        }
    }

    #[test]
    fn hud_stays_within_gpu_buffer_budget() {
        // main.rs cấp bộ đệm 40000 đỉnh; ca xấu nhất (bật mọi thứ) phải nhỏ hơn.
        let mut c = controls();
        c.debug = true;
        c.paused = true;
        c.say("PLAYER LIST: NEEDS SERVER");
        let lines: Vec<String> = (0..8).map(|_| "XYZ: -123.45 -67.89 -100.00".to_string()).collect();
        let (bg, fg) = c.hud(&lines);
        let verts = (bg.len() + fg.len()) / 6;
        assert!(verts < 40_000, "{} đỉnh", verts);
    }

    #[test]
    fn gui_hidden_draws_only_gui_button() {
        let mut c = controls();
        c.gui_hidden = true;
        let (bg, _) = c.hud(&[]);
        assert_eq!(bg.len(), 36); // đúng 1 nền nút
    }
}
