//! Bounded, window-local geometry for the optional native desktop previews.
#![cfg_attr(not(windows), allow(dead_code))]
use serde::{Deserialize, Serialize};

pub const MAX_PREVIEWS: usize = 32;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub id: String,
    pub process_id: String,
    pub process_birth: String,
}
impl Target {
    pub fn numbers(&self) -> Option<(u64, u32, u64)> {
        fn decimal(value: &str) -> Option<u64> {
            if value.is_empty() || value.len() > 20 || !value.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            value.parse().ok().filter(|&n| n != 0)
        }
        Some((
            decimal(&self.id)?,
            u32::try_from(decimal(&self.process_id)?).ok()?,
            decimal(&self.process_birth)?,
        ))
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub target: Target,
    pub rect: Bounds,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(default)]
    pub generation: String,
    #[serde(default)]
    pub revision: String,
    pub items: Vec<Item>,
}

/// Partly clipped slots are left to the HTML placeholder. A native thumbnail
/// must never bleed into a title bar, another control, or outside this window.
pub fn destination(rect: Bounds, scale: f64, width: u32, height: u32) -> Option<[i32; 4]> {
    let values = [rect.x, rect.y, rect.width, rect.height, scale];
    if values.iter().any(|value| !value.is_finite())
        || !(0.5..=8.0).contains(&scale)
        || rect.x < 0.0
        || rect.y < 0.0
        || rect.width < 8.0
        || rect.height < 8.0
        || width > i32::MAX as u32
        || height > i32::MAX as u32
    {
        return None;
    }
    let right = (rect.x + rect.width) * scale;
    let bottom = (rect.y + rect.height) * scale;
    if right > f64::from(width) || bottom > f64::from(height) {
        return None;
    }
    Some([
        (rect.x * scale).ceil() as i32,
        (rect.y * scale).ceil() as i32,
        right.floor() as i32,
        bottom.floor() as i32,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_preserves_native_birth_without_javascript_number_rounding() {
        let target: Target = serde_json::from_str(
            r#"{"id":"393448","process_id":"34208","process_birth":"134390888966056401"}"#,
        )
        .unwrap();
        assert_eq!(target.numbers(), Some((393448, 34208, 134390888966056401)));
        for id in ["0", "-1", "1e5", " 1", "18446744073709551616"] {
            assert!(Target { id: id.into(), ..target.clone() }.numbers().is_none());
        }
        assert!(Target { process_id: "4294967296".into(), ..target }.numbers().is_none());
    }

    #[test]
    fn scale_conversion_stays_inside_the_owned_client_and_rejects_clipping() {
        let rect = Bounds { x: 10.0, y: 20.0, width: 100.0, height: 80.0 };
        assert_eq!(destination(rect, 1.5, 1000, 800), Some([15, 30, 165, 150]));
        for changed in [
            Bounds { x: -1.0, ..rect },
            Bounds { x: f64::NAN, ..rect },
            Bounds { width: f64::INFINITY, ..rect },
            Bounds { width: 1.0, ..rect },
            Bounds { y: 799.0, ..rect },
            Bounds { x: 999.0, ..rect },
        ] {
            assert!(destination(changed, 1.0, 1000, 800).is_none());
        }
        assert!(destination(rect, 0.0, 1000, 800).is_none());
    }
}
