//! Bố cục nút cảm ứng kiểu launcher Minecraft: nút chức năng phía trên,
//! D-pad 3x3 bên trái dưới, nút nhảy + ESC bên phải dưới.

use crate::font;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Sneak,
    Primary,
    Secondary,
    Gui,
    Inv,
    Debug,
    Chat,
    Keyboard,
    Tab,
    Zoom,
    Third,
    Mouse,
    Esc,
}

impl Action {
    /// Nút "giữ": có tác dụng khi còn đang chạm. Các nút khác bấm một lần là xong.
    pub fn is_hold(self) -> bool {
        matches!(
            self,
            Action::Forward
                | Action::Back
                | Action::Left
                | Action::Right
                | Action::Jump
                | Action::Primary
                | Action::Secondary
                | Action::Zoom
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && self.x + self.w > o.x && self.y < o.y + o.h && self.y + self.h > o.y
    }
}

#[derive(Clone, Debug)]
pub struct Button {
    pub action: Action,
    pub label: &'static str,
    pub rect: Rect,
}

/// Kích thước 1 "ô" của phông chữ, tính bằng pixel thật.
pub fn text_unit(scale: f32) -> f32 {
    (1.2 * scale).round().max(1.0)
}

/// Mũi tên và hình thoi vẽ to gấp đôi cho dễ nhìn.
pub fn label_unit(label: &str, scale: f32) -> f32 {
    let u = text_unit(scale);
    if label.chars().count() == 1 && "^~<>*".contains(label) {
        u * 2.0
    } else {
        u
    }
}

fn btn_width(label: &str, scale: f32) -> f32 {
    font::text_width_units(label) as f32 * text_unit(scale) + 16.0 * scale
}

/// Tính vị trí toàn bộ nút theo kích thước màn hình (pixel) và hệ số scale.
pub fn layout(w: f32, h: f32, s: f32) -> Vec<Button> {
    let margin_x = 48.0 * s; // chừa chỗ cho tai thỏ / viền
    let top = 10.0 * s;
    let bottom = 22.0 * s; // chừa chỗ cho thanh home
    let gap = 3.0 * s;
    let bh = 30.0 * s;
    let mut v: Vec<Button> = Vec::new();

    // Hàng nút phía trên-trái
    let top_row = [
        (Action::Debug, "DEBUG"),
        (Action::Chat, "CHAT"),
        (Action::Keyboard, "KEYBOARD"),
        (Action::Tab, "TAB"),
        (Action::Zoom, "OPTI-ZOOM"),
    ];
    let mut x = margin_x;
    for (action, label) in top_row {
        let bw = btn_width(label, s);
        v.push(Button { action, label, rect: Rect { x, y: top, w: bw, h: bh } });
        x += bw + gap;
    }
    let bw = btn_width("3RD", s);
    v.push(Button {
        action: Action::Third,
        label: "3RD",
        rect: Rect { x: margin_x, y: top + bh + gap, w: bw, h: bh },
    });

    // Nút Mouse phía trên-phải
    let bw = btn_width("MOUSE", s);
    v.push(Button {
        action: Action::Mouse,
        label: "MOUSE",
        rect: Rect { x: w - margin_x - bw, y: top, w: bw, h: bh },
    });

    // D-pad 3x3 phía dưới-trái
    let cell = 48.0 * s;
    let x0 = margin_x;
    let y0 = h - bottom - 3.0 * cell;
    let grid = [
        [(Action::Primary, "PRI"), (Action::Forward, "^"), (Action::Secondary, "SEC")],
        [(Action::Left, "<"), (Action::Sneak, "*"), (Action::Right, ">")],
        [(Action::Gui, "GUI"), (Action::Back, "~"), (Action::Inv, "INV")],
    ];
    for (r, row) in grid.iter().enumerate() {
        for (c, (action, label)) in row.iter().enumerate() {
            v.push(Button {
                action: *action,
                label,
                rect: Rect {
                    x: x0 + c as f32 * cell + s,
                    y: y0 + r as f32 * cell + s,
                    w: cell - 2.0 * s,
                    h: cell - 2.0 * s,
                },
            });
        }
    }

    // ESC và nút nhảy phía dưới-phải
    let esc_w = btn_width("ESC", s) + 10.0 * s;
    let esc = Rect { x: w - margin_x - esc_w, y: h - bottom - bh, w: esc_w, h: bh };
    v.push(Button { action: Action::Esc, label: "ESC", rect: esc });
    let jump = 64.0 * s;
    v.push(Button {
        action: Action::Jump,
        label: "JUMP",
        rect: Rect { x: w - margin_x - jump, y: esc.y - jump - 8.0 * s, w: jump, h: jump },
    });

    v
}

/// Nút nằm tại điểm (x, y); nút vẽ sau được ưu tiên.
pub fn hit(buttons: &[Button], x: f32, y: f32) -> Option<usize> {
    (0..buttons.len()).rev().find(|&i| buttons[i].rect.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREENS: [(f32, f32, f32); 3] = [
        (2532.0, 1170.0, 3.0), // iPhone 14/15 ngang
        (2796.0, 1290.0, 3.0), // iPhone Pro Max ngang
        (1334.0, 750.0, 2.0),  // iPhone SE ngang
    ];

    #[test]
    fn all_buttons_inside_screen() {
        for (w, h, s) in SCREENS {
            for b in layout(w, h, s) {
                let r = b.rect;
                assert!(r.x >= 0.0 && r.y >= 0.0, "{:?} ra ngoài trái/trên", b.action);
                assert!(r.x + r.w <= w && r.y + r.h <= h, "{:?} ra ngoài phải/dưới ({}x{})", b.action, w, h);
            }
        }
    }

    #[test]
    fn no_buttons_overlap() {
        for (w, h, s) in SCREENS {
            let bs = layout(w, h, s);
            for i in 0..bs.len() {
                for j in (i + 1)..bs.len() {
                    assert!(
                        !bs[i].rect.overlaps(&bs[j].rect),
                        "{:?} chồng lên {:?} ({}x{})",
                        bs[i].action,
                        bs[j].action,
                        w,
                        h
                    );
                }
            }
        }
    }

    #[test]
    fn every_action_exists_once() {
        let bs = layout(2532.0, 1170.0, 3.0);
        for a in [
            Action::Forward, Action::Back, Action::Left, Action::Right, Action::Jump, Action::Sneak,
            Action::Primary, Action::Secondary, Action::Gui, Action::Inv, Action::Debug, Action::Chat,
            Action::Keyboard, Action::Tab, Action::Zoom, Action::Third, Action::Mouse, Action::Esc,
        ] {
            assert_eq!(bs.iter().filter(|b| b.action == a).count(), 1, "{:?}", a);
        }
    }

    #[test]
    fn dpad_arrows_are_where_expected() {
        let bs = layout(2532.0, 1170.0, 3.0);
        let g = |a: Action| bs.iter().find(|b| b.action == a).unwrap().rect;
        assert!(g(Action::Forward).y < g(Action::Back).y);
        assert!(g(Action::Left).x < g(Action::Right).x);
        assert!(g(Action::Primary).x < g(Action::Secondary).x);
    }

    #[test]
    fn hit_finds_button_center() {
        let bs = layout(2532.0, 1170.0, 3.0);
        for (i, b) in bs.iter().enumerate() {
            let (cx, cy) = (b.rect.x + b.rect.w / 2.0, b.rect.y + b.rect.h / 2.0);
            assert_eq!(hit(&bs, cx, cy), Some(i));
        }
        assert_eq!(hit(&bs, 1200.0, 500.0), None); // giữa màn hình là vùng xoay camera
    }

    #[test]
    fn label_fits_in_button() {
        for (w, h, s) in SCREENS {
            for b in layout(w, h, s) {
                let tw = font::text_width_units(b.label) as f32 * label_unit(b.label, s);
                let th = 7.0 * label_unit(b.label, s);
                assert!(tw <= b.rect.w && th <= b.rect.h, "chữ {:?} tràn nút", b.label);
            }
        }
    }

    #[test]
    fn hold_actions() {
        assert!(Action::Jump.is_hold());
        assert!(Action::Primary.is_hold());
        assert!(!Action::Gui.is_hold());
        assert!(!Action::Sneak.is_hold());
    }
}
