//! Key → position bindings: data types, virtual-key labels, map operations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::POINT;

/// A single key-binding entry: virtual-key code and screen position.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub vk: u32,
    pub x: i32,
    pub y: i32,
}

impl Binding {
    pub fn point(&self) -> POINT {
        POINT {
            x: self.x,
            y: self.y,
        }
    }
}

/// Convert a Windows virtual-key code to a human-readable label.
///
/// Handles common cases: `A`–`Z`, `0`–`9`, `F1`–`F24`, and a handful of
/// other well-known keys. Unknown VKs are rendered as `VK_<hex>`.
pub fn vk_to_label(vk: u32) -> String {
    match vk {
        0x01 => "LBtn".into(),   // VK_LBUTTON
        0x02 => "RBtn".into(),   // VK_RBUTTON
        0x03 => "Cancel".into(), // VK_CANCEL
        0x08 => "Back".into(),   // VK_BACK
        0x09 => "Tab".into(),
        0x0C => "Clear".into(),
        0x0D => "Enter".into(),
        0x10 => "Shift".into(),
        0x11 => "Ctrl".into(),
        0x12 => "Alt".into(),
        0x13 => "Pause".into(),
        0x14 => "Caps".into(),
        0x1B => "Esc".into(),
        0x20 => "Space".into(),
        0x21 => "PgUp".into(),
        0x22 => "PgDn".into(),
        0x23 => "End".into(),
        0x24 => "Home".into(),
        0x25 => "←".into(),
        0x26 => "↑".into(),
        0x27 => "→".into(),
        0x28 => "↓".into(),
        0x2C => "PrtSc".into(),
        0x2D => "Ins".into(),
        0x2E => "Del".into(),
        0x5B => "LWin".into(),
        0x5C => "RWin".into(),
        0x60..=0x69 => format!("Num{}", vk - 0x60), // Numpad 0-9
        0x6A => "Num*".into(),
        0x6B => "Num+".into(),
        0x6D => "Num-".into(),
        0x6E => "Num.".into(),
        0x6F => "Num/".into(),
        0x70..=0x87 => format!("F{}", vk - 0x6F), // F1 - F24
        0x90 => "NumLk".into(),
        0x91 => "ScrLk".into(),
        0xA0 => "LShift".into(),
        0xA1 => "RShift".into(),
        0xA2 => "LCtrl".into(),
        0xA3 => "RCtrl".into(),
        0xA4 => "LAlt".into(),
        0xA5 => "RAlt".into(),
        // Printable ASCII range
        0x30..=0x39 => char::from_u32(vk).unwrap().to_string(), // 0-9
        0x41..=0x5A => char::from_u32(vk).unwrap().to_string(), // A-Z
        c => format!("VK_{:X}", c),
    }
}

/// Insert or update a binding in the map, returning any previous position.
pub fn upsert(bindings: &mut HashMap<u32, POINT>, vk: u32, pos: POINT) -> Option<POINT> {
    bindings.insert(vk, pos)
}
