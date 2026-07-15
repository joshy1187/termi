use slint::Color;
use vt100::Color as VtColor;

const ANSI_16: [(u8, u8, u8); 16] = [
    (22, 24, 33),
    (255, 95, 87),
    (40, 200, 64),
    (254, 188, 46),
    (89, 146, 255),
    (199, 111, 255),
    (80, 200, 220),
    (214, 218, 229),
    (105, 111, 130),
    (255, 122, 114),
    (93, 224, 112),
    (255, 211, 103),
    (123, 170, 255),
    (218, 148, 255),
    (118, 222, 235),
    (249, 250, 255),
];

pub const DEFAULT_FOREGROUND: (u8, u8, u8) = (223, 226, 237);
pub const DEFAULT_BACKGROUND: (u8, u8, u8) = (8, 10, 17);

pub fn foreground(color: VtColor, bold: bool, dim: bool) -> Color {
    let (mut r, mut g, mut b) = resolve(color, DEFAULT_FOREGROUND, bold);
    if dim {
        r = ((u16::from(r) * 65) / 100) as u8;
        g = ((u16::from(g) * 65) / 100) as u8;
        b = ((u16::from(b) * 65) / 100) as u8;
    }
    Color::from_argb_u8(255, r, g, b)
}

pub fn background(color: VtColor) -> Color {
    match color {
        VtColor::Default => Color::from_argb_u8(0, 0, 0, 0),
        _ => {
            let (r, g, b) = resolve(color, DEFAULT_BACKGROUND, false);
            Color::from_argb_u8(210, r, g, b)
        }
    }
}

pub fn opaque_background(color: VtColor) -> Color {
    let (r, g, b) = resolve(color, DEFAULT_BACKGROUND, false);
    Color::from_argb_u8(255, r, g, b)
}

fn resolve(color: VtColor, default: (u8, u8, u8), bold: bool) -> (u8, u8, u8) {
    match color {
        VtColor::Default => default,
        VtColor::Rgb(r, g, b) => (r, g, b),
        VtColor::Idx(index) if index < 16 => {
            let effective = if bold && index < 8 { index + 8 } else { index };
            ANSI_16[usize::from(effective)]
        }
        VtColor::Idx(index) if index < 232 => {
            let index = index - 16;
            let r = index / 36;
            let g = (index % 36) / 6;
            let b = index % 6;
            (cube_component(r), cube_component(g), cube_component(b))
        }
        VtColor::Idx(index) => {
            let level = 8 + (index.saturating_sub(232) * 10);
            (level, level, level)
        }
    }
}

fn cube_component(value: u8) -> u8 {
    if value == 0 { 0 } else { 55 + value * 40 }
}
