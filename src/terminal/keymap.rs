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

    let mut bytes = if special(Key::Return) {
        vec![b'\r']
    } else if special(Key::Backspace) {
        vec![0x7f]
    } else if special(Key::Tab) {
        if shift {
            b"\x1b[Z".to_vec()
        } else {
            vec![b'\t']
        }
    } else if special(Key::Escape) {
        vec![0x1b]
    } else if special(Key::UpArrow) {
        cursor_sequence(b'A', application_cursor)
    } else if special(Key::DownArrow) {
        cursor_sequence(b'B', application_cursor)
    } else if special(Key::RightArrow) {
        cursor_sequence(b'C', application_cursor)
    } else if special(Key::LeftArrow) {
        cursor_sequence(b'D', application_cursor)
    } else if special(Key::Home) {
        b"\x1b[H".to_vec()
    } else if special(Key::End) {
        b"\x1b[F".to_vec()
    } else if special(Key::Insert) {
        b"\x1b[2~".to_vec()
    } else if special(Key::Delete) {
        b"\x1b[3~".to_vec()
    } else if special(Key::PageUp) {
        b"\x1b[5~".to_vec()
    } else if special(Key::PageDown) {
        b"\x1b[6~".to_vec()
    } else if special(Key::F1) {
        b"\x1bOP".to_vec()
    } else if special(Key::F2) {
        b"\x1bOQ".to_vec()
    } else if special(Key::F3) {
        b"\x1bOR".to_vec()
    } else if special(Key::F4) {
        b"\x1bOS".to_vec()
    } else if special(Key::F5) {
        b"\x1b[15~".to_vec()
    } else if special(Key::F6) {
        b"\x1b[17~".to_vec()
    } else if special(Key::F7) {
        b"\x1b[18~".to_vec()
    } else if special(Key::F8) {
        b"\x1b[19~".to_vec()
    } else if special(Key::F9) {
        b"\x1b[20~".to_vec()
    } else if special(Key::F10) {
        b"\x1b[21~".to_vec()
    } else if special(Key::F11) {
        b"\x1b[23~".to_vec()
    } else if special(Key::F12) {
        b"\x1b[24~".to_vec()
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

fn cursor_sequence(final_byte: u8, application_cursor: bool) -> Vec<u8> {
    if application_cursor {
        vec![0x1b, b'O', final_byte]
    } else {
        vec![0x1b, b'[', final_byte]
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
        '?' => 127,
        _ => return None,
    };

    Some(vec![code])
}

#[cfg(test)]
mod tests {
    use super::encode_control;

    #[test]
    fn encodes_control_c() {
        assert_eq!(encode_control("c"), Some(vec![3]));
    }
}
