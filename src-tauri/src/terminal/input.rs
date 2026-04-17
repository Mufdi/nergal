use termwiz::input::{KeyCode, Modifiers};

use super::types::TerminalKeyEvent;

/// Translate a browser-side `TerminalKeyEvent` into the wezterm-term key
/// pair that [`wezterm_term::Terminal::key_down`] expects.
///
/// We key off `KeyboardEvent.code` for layout-independent physical keys
/// (arrows, F-keys, Backspace, Enter, etc.) and fall back to the event's
/// `text` / `key` for printable characters, so Unicode composition and
/// non-QWERTY layouts keep working.
///
/// Returns `None` when the key cannot be meaningfully mapped (dead keys,
/// OS-level hotkeys, etc.) — callers should just drop the event in that case.
pub fn map_event(event: &TerminalKeyEvent) -> Option<(KeyCode, Modifiers)> {
    // Drop events that carry no user intent for the PTY:
    // - Lone modifier keydowns (`Control`, `Shift`, `Alt`, `Meta`) would
    //   otherwise fall through to the `Char('C')` etc. branch because the
    //   `key` string starts with those letters. That bug was letting a
    //   solo Ctrl press reach the shell as Ctrl+C and wipe the line.
    // - Dead keys (accent composition lead-ins) carry `key="Dead"`, which
    //   would be interpreted as `Char('D')`.
    // - Browser-placeholder keys like `Unidentified` and `Process`.
    if matches!(
        event.code.as_str(),
        "ControlLeft"
            | "ControlRight"
            | "ShiftLeft"
            | "ShiftRight"
            | "AltLeft"
            | "AltRight"
            | "MetaLeft"
            | "MetaRight"
            | "OSLeft"
            | "OSRight"
    ) {
        return None;
    }
    if matches!(
        event.key.as_str(),
        "Dead" | "Unidentified" | "Process" | "Control" | "Shift" | "Alt" | "Meta" | "Super"
    ) {
        return None;
    }

    let mods = modifiers_for(event);

    // Physical key mappings first — these must not be shadowed by `text`
    // because some of them (Backspace, Enter) carry no printable `key`.
    let key = match event.code.as_str() {
        "Enter" | "NumpadEnter" => KeyCode::Enter,
        "Tab" => KeyCode::Tab,
        "Backspace" => KeyCode::Backspace,
        "Escape" => KeyCode::Escape,
        "Delete" => KeyCode::Delete,
        "Insert" => KeyCode::Insert,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        "ArrowUp" => KeyCode::UpArrow,
        "ArrowDown" => KeyCode::DownArrow,
        "ArrowLeft" => KeyCode::LeftArrow,
        "ArrowRight" => KeyCode::RightArrow,
        "NumLock" => KeyCode::NumLock,
        "ScrollLock" => KeyCode::ScrollLock,
        "CapsLock" => KeyCode::CapsLock,
        "Pause" => KeyCode::Pause,
        "ContextMenu" => KeyCode::Menu,
        "PrintScreen" => KeyCode::PrintScreen,
        code if code.starts_with('F') && code[1..].chars().all(|c| c.is_ascii_digit()) => {
            let n: u8 = code[1..].parse().ok()?;
            if (1..=24).contains(&n) {
                KeyCode::Function(n)
            } else {
                return None;
            }
        }
        _ => {
            // Prefer the composed `text` if the OS produced one (handles IME,
            // AltGr, dead keys that resolved). Otherwise use `key`. Either way
            // we only accept the first grapheme — multi-char input goes
            // through the paste path instead.
            let source = event.text.as_deref().unwrap_or(&event.key);
            let ch = source.chars().next()?;
            if ch.is_control() {
                return None;
            }
            KeyCode::Char(ch)
        }
    };

    Some((key, mods))
}

fn modifiers_for(event: &TerminalKeyEvent) -> Modifiers {
    let mut mods = Modifiers::NONE;
    if event.ctrl {
        mods |= Modifiers::CTRL;
    }
    if event.shift {
        mods |= Modifiers::SHIFT;
    }
    if event.alt {
        mods |= Modifiers::ALT;
    }
    if event.meta {
        mods |= Modifiers::SUPER;
    }
    mods
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(code: &str, key: &str) -> TerminalKeyEvent {
        TerminalKeyEvent {
            code: code.into(),
            key: key.into(),
            text: None,
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
        }
    }

    #[test]
    fn printable_char_maps_via_text_then_key() {
        let (kc, _) = map_event(&evt("KeyA", "a")).unwrap();
        assert!(matches!(kc, KeyCode::Char('a')));

        let mut with_text = evt("KeyA", "a");
        with_text.text = Some("á".into());
        let (kc, _) = map_event(&with_text).unwrap();
        assert!(matches!(kc, KeyCode::Char('á')));
    }

    #[test]
    fn physical_keys_bypass_text_fallback() {
        let (kc, _) = map_event(&evt("Backspace", "Backspace")).unwrap();
        assert!(matches!(kc, KeyCode::Backspace));

        let (kc, _) = map_event(&evt("Enter", "Enter")).unwrap();
        assert!(matches!(kc, KeyCode::Enter));

        let (kc, _) = map_event(&evt("ArrowLeft", "ArrowLeft")).unwrap();
        assert!(matches!(kc, KeyCode::LeftArrow));
    }

    #[test]
    fn function_keys_parse_number() {
        let (kc, _) = map_event(&evt("F1", "F1")).unwrap();
        assert!(matches!(kc, KeyCode::Function(1)));

        let (kc, _) = map_event(&evt("F12", "F12")).unwrap();
        assert!(matches!(kc, KeyCode::Function(12)));

        // Out-of-range (F25+) must not map; the arm explicitly rejects it.
        assert!(map_event(&evt("F25", "F25")).is_none());
    }

    #[test]
    fn modifiers_compose_bitflags() {
        let mut e = evt("KeyA", "a");
        e.ctrl = true;
        e.shift = true;
        let (_, mods) = map_event(&e).unwrap();
        assert!(mods.contains(Modifiers::CTRL));
        assert!(mods.contains(Modifiers::SHIFT));
        assert!(!mods.contains(Modifiers::ALT));
    }

    #[test]
    fn control_chars_without_physical_mapping_are_rejected() {
        // Some browsers deliver weird composed keys with `key = "\u{0}"` etc.
        // when a dead sequence resolves; drop them instead of forwarding
        // bogus `Char('\0')` to the shell.
        let e = evt("Unidentified", "\u{0}");
        assert!(map_event(&e).is_none());
    }

    #[test]
    fn lone_modifier_keydowns_are_rejected() {
        // Pressing a bare Control / Shift / Alt / Meta key must not reach
        // the PTY — if we let `key="Control"` fall into the Char fallback
        // we end up sending Char('C') with CTRL mod, which wezterm encodes
        // as `\x03` (SIGINT) and wipes the shell line.
        for (code, key) in [
            ("ControlLeft", "Control"),
            ("ControlRight", "Control"),
            ("ShiftLeft", "Shift"),
            ("AltLeft", "Alt"),
            ("MetaLeft", "Meta"),
            ("OSLeft", "Meta"),
        ] {
            let mut e = evt(code, key);
            e.ctrl = key == "Control";
            e.shift = key == "Shift";
            e.alt = key == "Alt";
            e.meta = key == "Meta";
            assert!(
                map_event(&e).is_none(),
                "bare {} must not map to any KeyCode",
                key
            );
        }
    }

    #[test]
    fn dead_keys_are_rejected() {
        // The accent lead-in on Spanish / Latin layouts. Must not send
        // `Char('D')` to the PTY.
        let e = evt("BracketRight", "Dead");
        assert!(map_event(&e).is_none());
    }
}
