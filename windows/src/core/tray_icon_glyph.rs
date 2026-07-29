//! The tray glyph, drawn rather than shipped.
//!
//! macOS synthesises its menu bar icon at runtime (`StatusIcon.swift`) instead
//! of bundling an asset, and this follows suit: no `.ico`, no resource
//! compiler in the build, nothing beside the exe that can go missing from a
//! portable copy. The shape is the same as the mac one — an open "C" arc with
//! four sound-level bars across the middle — expressed as distance fields so it
//! renders cleanly at whatever size the shell asks for.
//!
//! Where this has to differ: macOS template images are recoloured by the system
//! to suit the menu bar. Windows has no equivalent, so the glyph is drawn white
//! with a dark rim, which stays legible on both light and dark taskbars.

/// The coordinate space the shape is defined in, matching `StatusIcon.swift`'s
/// 18x18 canvas so the two icons stay in step.
const CANVAS: f32 = 18.0;

const CENTER: f32 = CANVAS / 2.0;
const ARC_RADIUS: f32 = 7.5;
/// Half of the mac build's 2.1 line width.
const ARC_HALF_WIDTH: f32 = 1.05;
/// The "C" opens to the right: the sweep runs counter-clockwise from 35° to
/// 325°, leaving the 70° gap facing east.
const ARC_START_DEG: f32 = 35.0;
const ARC_END_DEG: f32 = 325.0;

/// Half of the mac build's 1.6 bar width.
const BAR_HALF_WIDTH: f32 = 0.8;
const BAR_HEIGHTS: [f32; 4] = [3.5, 7.5, 5.5, 3.0];
const BAR_SPACING: f32 = 2.3;

/// Dark rim around the white glyph, in canvas units.
const RIM: f32 = 0.55;

const GLYPH_RGB: [u8; 3] = [255, 255, 255];
const RIM_RGB: [u8; 3] = [24, 24, 24];

/// RGBA bytes for a `size`x`size` icon, row-major, ready for
/// `tray_icon::Icon::from_rgba`.
pub fn glyph_rgba(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let px_per_unit = size as f32 / CANVAS;
    // Widened before multiplying: `size` is a constant at every call site
    // today, but the product overflows u32 past 32768 and a panic here would
    // take the tray icon with it.
    let mut out = vec![0u8; (size as usize).pow(2) * 4];

    for y in 0..size {
        for x in 0..size {
            // Sample the pixel centre, in canvas units.
            let px = (x as f32 + 0.5) / px_per_unit;
            let py = (y as f32 + 0.5) / px_per_unit;

            let distance = distance_to_shape(px, py);
            // `distance` is measured from the stroke's centreline, so the
            // stroke itself is everything within half its width.
            let glyph = coverage(ARC_HALF_WIDTH - distance, px_per_unit);
            let rimmed = coverage(ARC_HALF_WIDTH + RIM - distance, px_per_unit);
            if rimmed <= 0.0 {
                continue;
            }

            let offset = ((y as usize) * (size as usize) + x as usize) * 4;
            for channel in 0..3 {
                out[offset + channel] = mix(RIM_RGB[channel], GLYPH_RGB[channel], glyph);
            }
            out[offset + 3] = (rimmed * 255.0).round() as u8;
        }
    }
    out
}

/// Distance from the nearest stroke centreline, in canvas units.
fn distance_to_shape(px: f32, py: f32) -> f32 {
    let mut nearest = distance_to_arc(px, py);

    // Bars are centred as a group on the canvas centre, like the mac version.
    let first = CENTER - (BAR_HEIGHTS.len() - 1) as f32 * BAR_SPACING / 2.0;
    for (index, height) in BAR_HEIGHTS.iter().enumerate() {
        let x = first + index as f32 * BAR_SPACING;
        // A rounded bar is a capsule: the round caps are what the radius of
        // the half-width buys, so the segment is shortened by that much.
        let half = (height / 2.0 - BAR_HALF_WIDTH).max(0.0);
        let bar = distance_to_segment(px, py, x, CENTER - half, x, CENTER + half);
        // The bars are drawn a touch thinner than the arc; shifting the
        // distance keeps one shared stroke width for the whole glyph.
        nearest = nearest.min(bar + (ARC_HALF_WIDTH - BAR_HALF_WIDTH));
    }
    nearest
}

fn distance_to_arc(px: f32, py: f32) -> f32 {
    let dx = px - CENTER;
    // Canvas y grows downward; the mac angles are measured with y upward.
    let dy = CENTER - py;
    let radius = (dx * dx + dy * dy).sqrt();

    let mut degrees = dy.atan2(dx).to_degrees();
    if degrees < 0.0 {
        degrees += 360.0;
    }
    if (ARC_START_DEG..=ARC_END_DEG).contains(&degrees) {
        return (radius - ARC_RADIUS).abs();
    }
    // Outside the sweep, the nearest point is an end cap.
    endpoint_distance(px, py, ARC_START_DEG).min(endpoint_distance(px, py, ARC_END_DEG))
}

fn endpoint_distance(px: f32, py: f32, degrees: f32) -> f32 {
    let radians = degrees.to_radians();
    let ex = CENTER + ARC_RADIUS * radians.cos();
    let ey = CENTER - ARC_RADIUS * radians.sin();
    ((px - ex).powi(2) + (py - ey).powi(2)).sqrt()
}

fn distance_to_segment(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (abx, aby) = (bx - ax, by - ay);
    let (apx, apy) = (px - ax, py - ay);
    let length_squared = abx * abx + aby * aby;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        ((apx * abx + apy * aby) / length_squared).clamp(0.0, 1.0)
    };
    ((apx - abx * t).powi(2) + (apy - aby * t).powi(2)).sqrt()
}

/// Antialiasing: one pixel of linear falloff either side of the edge.
fn coverage(signed_distance_units: f32, px_per_unit: f32) -> f32 {
    (signed_distance_units * px_per_unit + 0.5).clamp(0.0, 1.0)
}

fn mix(from: u8, to: u8, amount: f32) -> u8 {
    (from as f32 + (to as f32 - from as f32) * amount).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Alpha of the pixel covering a point given in canvas units.
    fn alpha_at(rgba: &[u8], size: u32, canvas_x: f32, canvas_y: f32) -> u8 {
        let px_per_unit = size as f32 / CANVAS;
        let x = (canvas_x * px_per_unit) as u32;
        let y = (canvas_y * px_per_unit) as u32;
        rgba[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn buffer_is_rgba_sized() {
        for size in [16, 24, 32, 64] {
            assert_eq!(glyph_rgba(size).len(), (size * size * 4) as usize);
        }
    }

    #[test]
    fn a_zero_size_still_yields_a_usable_buffer() {
        // `Icon::from_rgba` rejects a length mismatch, so degenerating to 1x1
        // matters more than honouring a nonsensical request.
        assert_eq!(glyph_rgba(0).len(), 4);
    }

    #[test]
    fn corners_are_transparent() {
        let size = 32;
        let rgba = glyph_rgba(size);
        for (x, y) in [(0, 0), (size - 1, 0), (0, size - 1), (size - 1, size - 1)] {
            let alpha = rgba[((y * size + x) * 4 + 3) as usize];
            assert_eq!(alpha, 0, "corner ({x},{y}) should be empty");
        }
    }

    #[test]
    fn the_arc_is_drawn_on_the_left() {
        let size = 64;
        let rgba = glyph_rgba(size);
        // Due west of centre, on the arc's radius.
        assert_eq!(alpha_at(&rgba, size, CENTER - ARC_RADIUS, CENTER), 255);
    }

    #[test]
    fn the_c_opens_to_the_right() {
        let size = 64;
        let rgba = glyph_rgba(size);
        // Due east is the middle of the 70° gap, which is what makes it a "C"
        // rather than an "O".
        assert_eq!(alpha_at(&rgba, size, CENTER + ARC_RADIUS, CENTER), 0);
    }

    #[test]
    fn the_tallest_bar_reaches_further_than_the_shortest() {
        let size = 64;
        let rgba = glyph_rgba(size);
        let first = CENTER - (BAR_HEIGHTS.len() - 1) as f32 * BAR_SPACING / 2.0;

        // Bar 1 is 7.5 tall, bar 3 is 3.0: a point 2.5 above centre is inside
        // the first and clear of the last.
        let tallest_x = first + BAR_SPACING;
        let shortest_x = first + 3.0 * BAR_SPACING;
        assert_eq!(alpha_at(&rgba, size, tallest_x, CENTER - 2.5), 255);
        assert_eq!(alpha_at(&rgba, size, shortest_x, CENTER - 2.5), 0);
    }

    #[test]
    fn the_glyph_is_white_with_a_dark_rim() {
        let size = 64;
        let rgba = glyph_rgba(size);
        let px_per_unit = size as f32 / CANVAS;

        let x = ((CENTER - ARC_RADIUS) * px_per_unit) as u32;
        let y = (CENTER * px_per_unit) as u32;
        let offset = ((y * size + x) * 4) as usize;
        assert_eq!(&rgba[offset..offset + 3], &GLYPH_RGB);

        // Just outside the stroke: still painted, but dark.
        let rim_x = ((CENTER - ARC_RADIUS - ARC_HALF_WIDTH - RIM / 2.0) * px_per_unit) as u32;
        let rim_offset = ((y * size + rim_x) * 4) as usize;
        assert!(rgba[rim_offset + 3] > 0, "the rim should be painted");
        assert!(
            rgba[rim_offset] < 128,
            "the rim should be dark, got {}",
            rgba[rim_offset]
        );
    }
}
