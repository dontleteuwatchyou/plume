//! Pastille rouge avec le nombre de mentions non lues, comme l'appli Discord
//! (inject/desktop.js le lit dans le titre de la page). Dessinée ici, en
//! pixels : pas de police à embarquer.
//!
//! Où elle se pose dépend du système. Windows a une pastille par fenêtre dans
//! la barre des tâches. Les bureaux Linux n'ont rien de commun à cet endroit
//! (GNOME, KDE et Hyprland ne montrent même pas la même chose), mais ils
//! affichent tous la zone de notification par le même protocole
//! (StatusNotifierItem) : la pastille va donc sur l'icône de Plume qui s'y
//! trouve déjà.

use tauri::image::Image;
use tauri::WebviewWindow;

const SIZE: usize = 32;

/// Chiffres 3×5 (une ligne = 3 bits, de gauche à droite).
const DIGITS: [[u8; 5]; 10] = [
    [7, 5, 5, 5, 7], [2, 6, 2, 2, 7], [7, 1, 7, 4, 7], [7, 1, 7, 1, 7], [5, 5, 7, 1, 1],
    [7, 4, 7, 1, 7], [7, 4, 7, 5, 7], [7, 1, 2, 2, 2], [7, 5, 7, 5, 7], [7, 5, 7, 1, 7],
];
const PLUS: [u8; 5] = [0, 2, 7, 2, 0];

fn render(count: u32) -> Vec<u8> {
    let mut px = vec![0u8; SIZE * SIZE * 4];
    let c = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            // Bord lissé sur un pixel.
            let a = (c + 0.5 - d).clamp(0.0, 1.0);
            let i = (y * SIZE + x) * 4;
            px[i..i + 4].copy_from_slice(&[0xf2, 0x3f, 0x43, (a * 255.0) as u8]);
        }
    }
    let glyphs: Vec<[u8; 5]> = if count > 9 { vec![DIGITS[9], PLUS] } else { vec![DIGITS[count as usize]] };
    let scale = if glyphs.len() == 1 { 4 } else { 3 };
    let width = glyphs.len() * 3 * scale + (glyphs.len() - 1) * scale;
    let (x0, y0) = ((SIZE - width) / 2, (SIZE - 5 * scale) / 2);
    for (g, glyph) in glyphs.iter().enumerate() {
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..3 {
                if bits & (4 >> col) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = x0 + g * 4 * scale + col * scale + dx;
                        let y = y0 + row * scale + dy;
                        let i = (y * SIZE + x) * 4;
                        px[i..i + 4].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
                    }
                }
            }
        }
    }
    px
}

#[tauri::command]
pub fn set_badge(window: WebviewWindow, count: u32) {
    apply(&window, count);
}

#[cfg(windows)]
fn apply(window: &WebviewWindow, count: u32) {
    let icon = (count > 0).then(|| Image::new_owned(render(count), SIZE as u32, SIZE as u32));
    let _ = window.set_overlay_icon(icon);
}

/// L'icône de la zone de notification, avec la pastille dans son coin.
#[cfg(not(windows))]
fn apply(window: &WebviewWindow, count: u32) {
    use tauri::Manager;

    let app = window.app_handle();
    let Some(tray) = app.tray_by_id("tray") else { return };
    let Some(base) = app.default_window_icon() else { return };
    let icon = if count == 0 { base.clone().to_owned() } else { stamp(base, count) };
    let _ = tray.set_icon(Some(icon));
}

/// Pose la pastille sur les trois cinquièmes bas-droite de `base`, en
/// échantillonnant au plus proche : l'icône de la zone de notification fait
/// rarement 32 pixels.
#[cfg(not(windows))]
fn stamp(base: &Image<'_>, count: u32) -> Image<'static> {
    let (w, h) = (base.width() as usize, base.height() as usize);
    let mut px = base.rgba().to_vec();
    let badge = render(count);
    let side = (w.min(h) * 3 / 5).max(1);
    let (x0, y0) = (w - side, h - side);
    for y in 0..side {
        for x in 0..side {
            let src = ((y * SIZE / side) * SIZE + x * SIZE / side) * 4;
            let a = u32::from(badge[src + 3]);
            if a == 0 {
                continue;
            }
            let dst = ((y0 + y) * w + x0 + x) * 4;
            for c in 0..3 {
                px[dst + c] = ((u32::from(badge[src + c]) * a + u32::from(px[dst + c]) * (255 - a)) / 255) as u8;
            }
            px[dst + 3] = px[dst + 3].max(badge[src + 3]);
        }
    }
    Image::new_owned(px, w as u32, h as u32)
}

#[cfg(test)]
mod tests {
    #[test]
    fn dessine_sans_deborder() {
        for n in [1, 7, 9, 10, 250] {
            assert_eq!(super::render(n).len(), super::SIZE * super::SIZE * 4);
        }
    }
}
