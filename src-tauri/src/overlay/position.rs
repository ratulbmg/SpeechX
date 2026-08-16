//! Where the pill sits: top-center on every platform, with a
//! platform-specific top margin (macOS clears the notch; Windows just
//! needs a small gap). Not wired to a live window yet — see `overlay`'s
//! module doc comment. Logic is written and tested ahead of M5 so the
//! window-creation code has this ready to call rather than needing math
//! worked out inline.
#![allow(dead_code)]

const PILL_WIDTH: f64 = 240.0;

#[cfg(target_os = "macos")]
const TOP_MARGIN: f64 = 44.0; // clears the notch

#[cfg(target_os = "windows")]
const TOP_MARGIN: f64 = 8.0;

pub fn pill_position(monitor_size: (u32, u32), scale: f64) -> (f64, f64) {
    let logical_w = monitor_size.0 as f64 / scale;
    let x = (logical_w - PILL_WIDTH) / 2.0;
    (x, TOP_MARGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_horizontally_for_a_1x_display() {
        let (x, _) = pill_position((1440, 900), 1.0);
        assert_eq!(x, (1440.0 - PILL_WIDTH) / 2.0);
    }

    #[test]
    fn accounts_for_retina_scale_factor() {
        let (x, _) = pill_position((2880, 1800), 2.0);
        assert_eq!(x, (1440.0 - PILL_WIDTH) / 2.0);
    }
}
