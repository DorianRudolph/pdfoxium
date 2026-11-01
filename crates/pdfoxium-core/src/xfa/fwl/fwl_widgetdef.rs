// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete XFA virtual-key values and platform editing modifiers.
pub const VKEY_BACK: u32 = 0x08;
pub const VKEY_TAB: u32 = 0x09;
pub const VKEY_NEWLINE: u32 = 0x0A;
pub const VKEY_CLEAR: u32 = 0x0C;
pub const VKEY_RETURN: u32 = 0x0D;
pub const VKEY_SHIFT: u32 = 0x10;
pub const VKEY_CONTROL: u32 = 0x11;
pub const VKEY_MENU: u32 = 0x12;
pub const VKEY_PAUSE: u32 = 0x13;
pub const VKEY_CAPITAL: u32 = 0x14;
pub const VKEY_KANA: u32 = 0x15;
pub const VKEY_HANGUL: u32 = 0x15;
pub const VKEY_JUNJA: u32 = 0x17;
pub const VKEY_FINAL: u32 = 0x18;
pub const VKEY_HANJA: u32 = 0x19;
pub const VKEY_KANJI: u32 = 0x19;
pub const VKEY_ESCAPE: u32 = 0x1B;
pub const VKEY_CONVERT: u32 = 0x1C;
pub const VKEY_NONCONVERT: u32 = 0x1D;
pub const VKEY_ACCEPT: u32 = 0x1E;
pub const VKEY_MODECHANGE: u32 = 0x1F;
pub const VKEY_SPACE: u32 = 0x20;
pub const VKEY_PRIOR: u32 = 0x21;
pub const VKEY_NEXT: u32 = 0x22;
pub const VKEY_END: u32 = 0x23;
pub const VKEY_HOME: u32 = 0x24;
pub const VKEY_LEFT: u32 = 0x25;
pub const VKEY_UP: u32 = 0x26;
pub const VKEY_RIGHT: u32 = 0x27;
pub const VKEY_DOWN: u32 = 0x28;
pub const VKEY_SELECT: u32 = 0x29;
pub const VKEY_PRINT: u32 = 0x2A;
pub const VKEY_EXECUTE: u32 = 0x2B;
pub const VKEY_SNAPSHOT: u32 = 0x2C;
pub const VKEY_INSERT: u32 = 0x2D;
pub const VKEY_DELETE: u32 = 0x2E;
pub const VKEY_HELP: u32 = 0x2F;
pub const VKEY_0: u32 = 0x30;
pub const VKEY_1: u32 = 0x31;
pub const VKEY_2: u32 = 0x32;
pub const VKEY_3: u32 = 0x33;
pub const VKEY_4: u32 = 0x34;
pub const VKEY_5: u32 = 0x35;
pub const VKEY_6: u32 = 0x36;
pub const VKEY_7: u32 = 0x37;
pub const VKEY_8: u32 = 0x38;
pub const VKEY_9: u32 = 0x39;
pub const VKEY_A: u32 = 0x41;
pub const VKEY_B: u32 = 0x42;
pub const VKEY_C: u32 = 0x43;
pub const VKEY_D: u32 = 0x44;
pub const VKEY_E: u32 = 0x45;
pub const VKEY_F: u32 = 0x46;
pub const VKEY_G: u32 = 0x47;
pub const VKEY_H: u32 = 0x48;
pub const VKEY_I: u32 = 0x49;
pub const VKEY_J: u32 = 0x4A;
pub const VKEY_K: u32 = 0x4B;
pub const VKEY_L: u32 = 0x4C;
pub const VKEY_M: u32 = 0x4D;
pub const VKEY_N: u32 = 0x4E;
pub const VKEY_O: u32 = 0x4F;
pub const VKEY_P: u32 = 0x50;
pub const VKEY_Q: u32 = 0x51;
pub const VKEY_R: u32 = 0x52;
pub const VKEY_S: u32 = 0x53;
pub const VKEY_T: u32 = 0x54;
pub const VKEY_U: u32 = 0x55;
pub const VKEY_V: u32 = 0x56;
pub const VKEY_W: u32 = 0x57;
pub const VKEY_X: u32 = 0x58;
pub const VKEY_Y: u32 = 0x59;
pub const VKEY_Z: u32 = 0x5A;
pub const VKEY_LWIN: u32 = 0x5B;
pub const VKEY_COMMAND: u32 = 0x5B;
pub const VKEY_RWIN: u32 = 0x5C;
pub const VKEY_APPS: u32 = 0x5D;
pub const VKEY_SLEEP: u32 = 0x5F;
pub const VKEY_NUMPAD0: u32 = 0x60;
pub const VKEY_NUMPAD1: u32 = 0x61;
pub const VKEY_NUMPAD2: u32 = 0x62;
pub const VKEY_NUMPAD3: u32 = 0x63;
pub const VKEY_NUMPAD4: u32 = 0x64;
pub const VKEY_NUMPAD5: u32 = 0x65;
pub const VKEY_NUMPAD6: u32 = 0x66;
pub const VKEY_NUMPAD7: u32 = 0x67;
pub const VKEY_NUMPAD8: u32 = 0x68;
pub const VKEY_NUMPAD9: u32 = 0x69;
pub const VKEY_MULTIPLY: u32 = 0x6A;
pub const VKEY_ADD: u32 = 0x6B;
pub const VKEY_SEPARATOR: u32 = 0x6C;
pub const VKEY_SUBTRACT: u32 = 0x6D;
pub const VKEY_DECIMAL: u32 = 0x6E;
pub const VKEY_DIVIDE: u32 = 0x6F;
pub const VKEY_F1: u32 = 0x70;
pub const VKEY_F2: u32 = 0x71;
pub const VKEY_F3: u32 = 0x72;
pub const VKEY_F4: u32 = 0x73;
pub const VKEY_F5: u32 = 0x74;
pub const VKEY_F6: u32 = 0x75;
pub const VKEY_F7: u32 = 0x76;
pub const VKEY_F8: u32 = 0x77;
pub const VKEY_F9: u32 = 0x78;
pub const VKEY_F10: u32 = 0x79;
pub const VKEY_F11: u32 = 0x7A;
pub const VKEY_F12: u32 = 0x7B;
pub const VKEY_F13: u32 = 0x7C;
pub const VKEY_F14: u32 = 0x7D;
pub const VKEY_F15: u32 = 0x7E;
pub const VKEY_F16: u32 = 0x7F;
pub const VKEY_F17: u32 = 0x80;
pub const VKEY_F18: u32 = 0x81;
pub const VKEY_F19: u32 = 0x82;
pub const VKEY_F20: u32 = 0x83;
pub const VKEY_F21: u32 = 0x84;
pub const VKEY_F22: u32 = 0x85;
pub const VKEY_F23: u32 = 0x86;
pub const VKEY_F24: u32 = 0x87;
pub const VKEY_NUNLOCK: u32 = 0x90;
pub const VKEY_SCROLL: u32 = 0x91;
pub const VKEY_LSHIFT: u32 = 0xA0;
pub const VKEY_RSHIFT: u32 = 0xA1;
pub const VKEY_LCONTROL: u32 = 0xA2;
pub const VKEY_RCONTROL: u32 = 0xA3;
pub const VKEY_LMENU: u32 = 0xA4;
pub const VKEY_RMENU: u32 = 0xA5;
pub const VKEY_BROWSER_BACK: u32 = 0xA6;
pub const VKEY_BROWSER_FORWARD: u32 = 0xA7;
pub const VKEY_BROWSER_REFRESH: u32 = 0xA8;
pub const VKEY_BROWSER_STOP: u32 = 0xA9;
pub const VKEY_BROWSER_SEARCH: u32 = 0xAA;
pub const VKEY_BROWSER_FAVORITES: u32 = 0xAB;
pub const VKEY_BROWSER_HOME: u32 = 0xAC;
pub const VKEY_VOLUME_MUTE: u32 = 0xAD;
pub const VKEY_VOLUME_DOWN: u32 = 0xAE;
pub const VKEY_VOLUME_UP: u32 = 0xAF;
pub const VKEY_MEDIA_NEXT_TRACK: u32 = 0xB0;
pub const VKEY_MEDIA_PREV_TRACK: u32 = 0xB1;
pub const VKEY_MEDIA_STOP: u32 = 0xB2;
pub const VKEY_MEDIA_PLAY_PAUSE: u32 = 0xB3;
pub const VKEY_MEDIA_LAUNCH_MAIL: u32 = 0xB4;
pub const VKEY_MEDIA_LAUNCH_MEDIA_SELECT: u32 = 0xB5;
pub const VKEY_MEDIA_LAUNCH_APP1: u32 = 0xB6;
pub const VKEY_MEDIA_LAUNCH_APP2: u32 = 0xB7;
pub const VKEY_OEM_1: u32 = 0xBA;
pub const VKEY_OEM_PLUS: u32 = 0xBB;
pub const VKEY_OEM_COMMA: u32 = 0xBC;
pub const VKEY_OEM_MINUS: u32 = 0xBD;
pub const VKEY_OEM_PERIOD: u32 = 0xBE;
pub const VKEY_OEM_2: u32 = 0xBF;
pub const VKEY_OEM_3: u32 = 0xC0;
pub const VKEY_OEM_4: u32 = 0xDB;
pub const VKEY_OEM_5: u32 = 0xDC;
pub const VKEY_OEM_6: u32 = 0xDD;
pub const VKEY_OEM_7: u32 = 0xDE;
pub const VKEY_OEM_8: u32 = 0xDF;
pub const VKEY_OEM_102: u32 = 0xE2;
pub const VKEY_PROCESSKEY: u32 = 0xE5;
pub const VKEY_PACKET: u32 = 0xE7;
pub const VKEY_ATTN: u32 = 0xF6;
pub const VKEY_CRSEL: u32 = 0xF7;
pub const VKEY_EXSEL: u32 = 0xF8;
pub const VKEY_EREOF: u32 = 0xF9;
pub const VKEY_PLAY: u32 = 0xFA;
pub const VKEY_ZOOM: u32 = 0xFB;
pub const VKEY_NONAME: u32 = 0xFC;
pub const VKEY_PA1: u32 = 0xFD;
pub const VKEY_OEM_CLEAR: u32 = 0xFE;
pub const VKEY_UNKNOWN: u32 = 0;

pub const KEY_FLAG_CTRL: u32 = 1;
pub const KEY_FLAG_ALT: u32 = 2;
pub const KEY_FLAG_SHIFT: u32 = 4;
pub const KEY_FLAG_COMMAND: u32 = 8;
pub const KEY_FLAG_LBUTTON: u32 = 16;
pub const KEY_FLAG_RBUTTON: u32 = 32;
pub const KEY_FLAG_MBUTTON: u32 = 64;
pub fn is_platform_shortcut_key(flags: u32) -> bool {
    let editing_modifier = if cfg!(target_vendor = "apple") {
        KEY_FLAG_COMMAND
    } else {
        KEY_FLAG_CTRL
    };
    flags & editing_modifier != 0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shortcut_modifier_preserves_platform_choice() {
        let expected = if cfg!(target_vendor = "apple") {
            KEY_FLAG_COMMAND
        } else {
            KEY_FLAG_CTRL
        };
        assert!(!is_platform_shortcut_key(0));
        for flags in 0..=127 {
            assert_eq!(is_platform_shortcut_key(flags), flags & expected != 0);
        }
        assert_eq!(VKEY_RETURN, 13);
        assert_eq!(VKEY_COMMAND, VKEY_LWIN);
        assert_eq!(VKEY_UNKNOWN, 0);
    }
}
