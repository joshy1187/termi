use slint::{SharedString, platform::Key};

pub fn encode_key(
    text: &str,
    control: bool,
    alt: bool,
    shift: bool,
    application_cursor: bool,
) -> Option<Vec<u8>> {
    let special = |key: Key| -> bool {
        let value: SharedString = key.into();
        text == value.as_str()
    };

    // Slint reports modifier transitions as individual key events. They update
    // its modifier state but are not terminal input themselves. Forwarding
    // their internal control-code representation corrupts the next typed
    // character, which is especially hard to notice at a hidden password
    // prompt.
    if [
        Key::Shift,
        Key::ShiftR,
        Key::Control,
        Key::ControlR,
        Key::Alt,
        Key::AltGr,
        Key::Meta,
        Key::MetaR,
        Key::CapsLock,
    ]
    .into_iter()
    .any(special)
    {
        return None;
    }

    let modifier = modifier_parameter(control, alt, shift);

    if special(Key::UpArrow) {
        return Some(cursor_sequence(b'A', application_cursor, modifier));
    }
    if special(Key::DownArrow) {
        return Some(cursor_sequence(b'B', application_cursor, modifier));
    }
    if special(Key::RightArrow) {
        return Some(cursor_sequence(b'C', application_cursor, modifier));
    }
    if special(Key::LeftArrow) {
        return Some(cursor_sequence(b'D', application_cursor, modifier));
    }
    if special(Key::Home) {
        return Some(cursor_sequence(b'H', application_cursor, modifier));
    }
    if special(Key::End) {
        return Some(cursor_sequence(b'F', application_cursor, modifier));
    }
    if special(Key::Insert) {
        return Some(csi_tilde(2, modifier));
    }
    if special(Key::Delete) {
        return Some(csi_tilde(3, modifier));
    }
    if special(Key::PageUp) {
        return Some(csi_tilde(5, modifier));
    }
    if special(Key::PageDown) {
        return Some(csi_tilde(6, modifier));
    }
    for (key, final_byte) in [
        (Key::F1, b'P'),
        (Key::F2, b'Q'),
        (Key::F3, b'R'),
        (Key::F4, b'S'),
    ] {
        if special(key) {
            return Some(function_key(final_byte, modifier));
        }
    }
    for (key, number) in [
        (Key::F5, 15),
        (Key::F6, 17),
        (Key::F7, 18),
        (Key::F8, 19),
        (Key::F9, 20),
        (Key::F10, 21),
        (Key::F11, 23),
        (Key::F12, 24),
    ] {
        if special(key) {
            return Some(csi_tilde(number, modifier));
        }
    }

    let mut bytes = if special(Key::Return) {
        vec![b'\r']
    } else if special(Key::Backspace) {
        vec![if control { 0x08 } else { 0x7f }]
    } else if special(Key::Tab) {
        if shift {
            b"\x1b[Z".to_vec()
        } else {
            vec![b'\t']
        }
    } else if special(Key::Escape) {
        vec![0x1b]
    } else if control {
        encode_control(text)?
    } else if !text.is_empty() {
        text.as_bytes().to_vec()
    } else {
        return None;
    };

    if alt && !special(Key::Escape) {
        bytes.insert(0, 0x1b);
    }

    Some(bytes)
}

fn modifier_parameter(control: bool, alt: bool, shift: bool) -> Option<u8> {
    let value = 1 + u8::from(shift) + 2 * u8::from(alt) + 4 * u8::from(control);
    (value > 1).then_some(value)
}

fn cursor_sequence(final_byte: u8, application_cursor: bool, modifier: Option<u8>) -> Vec<u8> {
    if let Some(modifier) = modifier {
        format!("\x1b[1;{modifier}{}", char::from(final_byte)).into_bytes()
    } else if application_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
    }
}

fn csi_tilde(number: u8, modifier: Option<u8>) -> Vec<u8> {
    match modifier {
        Some(modifier) => format!("\x1b[{number};{modifier}~").into_bytes(),
        None => format!("\x1b[{number}~").into_bytes(),
    }
}

fn function_key(final_byte: u8, modifier: Option<u8>) -> Vec<u8> {
    if let Some(modifier) = modifier {
        format!("\x1b[1;{modifier}{}", char::from(final_byte)).into_bytes()
    } else {
        vec![0x1b, b'O', final_byte]
    }
}

fn encode_control(text: &str) -> Option<Vec<u8>> {
    let mut chars = text.chars();
    let character = chars.next()?;
    if chars.next().is_some() {
        return None;
    }

    let code = match character {
        '@' | ' ' => 0,
        'a'..='z' => character as u8 - b'a' + 1,
        'A'..='Z' => character as u8 - b'A' + 1,
        '[' => 27,
        '\\' => 28,
        ']' => 29,
        '^' => 30,
        '_' => 31,
        '/' | '7' => 31,
        '2' => 0,
        '3' => 27,
        '4' => 28,
        '5' => 29,
        '6' => 30,
        '8' => 127,
        '?' => 127,
        _ => return None,
    };

    Some(vec![code])
}

#[cfg(test)]
mod tests {
    use slint::{SharedString, platform::Key};

    use super::{encode_control, encode_key};

    #[test]
    fn encodes_control_c() {
        assert_eq!(encode_control("c"), Some(vec![3]));
    }

    #[test]
    fn ignores_modifier_key_events() {
        for key in [
            Key::Shift,
            Key::ShiftR,
            Key::Control,
            Key::ControlR,
            Key::Alt,
            Key::AltGr,
            Key::Meta,
            Key::MetaR,
            Key::CapsLock,
        ] {
            let key: SharedString = key.into();
            assert_eq!(
                encode_key(key.as_str(), false, false, false, false),
                None,
                "modifier key must not become PTY input"
            );
        }
    }

    #[test]
    fn encodes_common_control_aliases() {
        assert_eq!(encode_control("/"), Some(vec![31]));
        assert_eq!(encode_control("2"), Some(vec![0]));
    }

    #[test]
    fn encodes_modified_cursor_key() {
        let up: SharedString = Key::UpArrow.into();
        assert_eq!(
            encode_key(up.as_str(), true, false, true, false),
            Some(b"\x1b[1;6A".to_vec())
        );
    }

    #[test]
    fn encodes_home_in_application_cursor_mode() {
        let home: SharedString = Key::Home.into();
        assert_eq!(
            encode_key(home.as_str(), false, false, false, true),
            Some(b"\x1bOH".to_vec())
        );
    }
}
