use super::{SoundAsset, SpriteAsset};
pub const FONT_ID_BASE: u32 = 256;
pub const FONT_CHARACTERS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ:-/+.%";
// Author-written pixel patterns, built in Rust and exported by make_assets.
// The atlas and manifest are inspectable, replaceable resources under assets/.
const fn glyph(c: u8) -> [u8; 7] {
    match c {
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 21, 19, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b':' => [0, 4, 4, 0, 4, 4, 0],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'+' => [0, 4, 4, 31, 4, 4, 0],
        b'.' => [0, 0, 0, 0, 0, 4, 4],
        b'%' => [25, 26, 2, 4, 8, 11, 19],
        _ => [0; 7],
    }
}
const fn atlas() -> [u8; 128 * 128 * 4] {
    let mut pixels = [0; 128 * 128 * 4];
    let mut tile = 0;
    while tile < 9 + FONT_CHARACTERS.len() {
        let mut y = 0;
        while y < 16 {
            let mut x = 0;
            while x < 16 {
                let dx = (x as i32 - 7).abs();
                let dy = (y as i32 - 7).abs();
                let on = if tile >= 9 {
                    x < 5 && y < 7 && (glyph(FONT_CHARACTERS[tile - 9])[y] & (1 << (4 - x))) != 0
                } else {
                    match tile {
                        0 => {
                            y >= 2 && y <= 13 && (dx <= (y as i32 - 2) / 2 || (y >= 10 && dx <= 6))
                        }
                        1 => dx + dy <= 7 && !(dy < 2 && dx > 2),
                        2 => (dy <= 3 && dx <= 7) || (dx <= 3 && dy <= 6) || (dx >= 5 && dy <= 5),
                        3 => (x == 6 || x == 7 || x == 8) && y >= 1 && y <= 14,
                        4 => dx * dx + dy * dy <= 20,
                        5 => {
                            (y >= 3 && y <= 6 && (x >= 2 && x <= 6 || x >= 9 && x <= 13))
                                || (y >= 6 && y <= 13 && dx <= 13 - y as i32)
                        }
                        6 => dx * dx + dy * dy <= 30 || (x == 11 && y < 3),
                        7 => dx <= 1 && dy <= 2 || dy <= 1 && dx <= 2,
                        _ => true,
                    }
                };
                if on {
                    let index = (((tile / 8) * 16 + y) * 128 + (tile % 8) * 16 + x) * 4;
                    let shade = if tile < 3 && dx > 2 { 170 } else { 255 };
                    pixels[index] = shade;
                    pixels[index + 1] = shade;
                    pixels[index + 2] = shade;
                    pixels[index + 3] = 255;
                }
                x += 1;
            }
            y += 1;
        }
        tile += 1;
    }
    pixels
}
pub static ATLAS: [u8; 128 * 128 * 4] = atlas();
pub fn sprites() -> Vec<SpriteAsset> {
    let mut sprites: Vec<_> = (0..9)
        .map(|tile| SpriteAsset {
            id: tile + 1,
            x: (tile % 8) * 16,
            y: (tile / 8) * 16,
            width: 16,
            height: 16,
        })
        .collect();
    for (index, &c) in FONT_CHARACTERS.iter().enumerate() {
        let tile = index as u32 + 9;
        sprites.push(SpriteAsset {
            id: FONT_ID_BASE + u32::from(c),
            x: (tile % 8) * 16,
            y: (tile / 8) * 16,
            width: 5,
            height: 7,
        });
    }
    sprites
}
pub fn sounds() -> Vec<SoundAsset> {
    [
        (0, 960, 50, 18),
        (1, 110, 420, 64),
        (2, 140, 180, 60),
        (1, 90, 180, 45),
        (2, 1320, 40, 12),
        (0, 220, 700, 45),
        (1, 660, 900, 50),
        (1, 80, 900, 64),
    ]
    .into_iter()
    .enumerate()
    .map(
        |(index, (waveform, frequency, duration_ms, gain_q8))| SoundAsset {
            id: index as u32 + 1,
            waveform,
            frequency,
            duration_ms,
            gain_q8,
        },
    )
    .collect()
}
