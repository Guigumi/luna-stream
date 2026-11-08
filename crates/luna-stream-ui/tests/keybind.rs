use luna_stream_ui::keybind::{
    KeyCaptureOutcome, format_hotkey_label, parse_key_event, validate_hotkey_pair,
};

#[test]
fn test_modifier_only_and_escape_and_backspace() {
    // Tecla modificadora solta não fecha nem grava
    assert_eq!(
        parse_key_event("Control", false, true, false, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );
    assert_eq!(
        parse_key_event("Alt", true, false, false, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );
    assert_eq!(
        parse_key_event("Shift", false, false, true, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );

    // Escape cancela a edição
    assert_eq!(
        parse_key_event("Escape", false, false, false, false),
        KeyCaptureOutcome::Cancelled
    );

    // Backspace / Delete restaura o padrão
    assert_eq!(
        parse_key_event("BackSpace", false, false, false, false),
        KeyCaptureOutcome::ResetDefault
    );
    assert_eq!(
        parse_key_event("Delete", false, false, false, false),
        KeyCaptureOutcome::ResetDefault
    );
}

#[test]
fn test_capture_and_format_hotkey_label() {
    let outcome = parse_key_event("F8", true, false, false, false);
    assert_eq!(outcome, KeyCaptureOutcome::Captured("ALT+F8".into()));
    assert_eq!(format_hotkey_label("ALT+F8"), "Alt + F8");

    let outcome_multi = parse_key_event("R", false, true, true, false);
    assert_eq!(
        outcome_multi,
        KeyCaptureOutcome::Captured("CTRL+SHIFT+R".into())
    );
    assert_eq!(format_hotkey_label("CTRL+SHIFT+R"), "Ctrl + Shift + R");
}

#[test]
fn test_validate_hotkey_pair_collision() {
    assert_eq!(validate_hotkey_pair("ALT+F8", "ALT+F9").unwrap(), "ALT+F8");
    assert_eq!(
        validate_hotkey_pair("Alt + F8", "ALT+F8").unwrap_err(),
        "Este atalho já está em uso pela outra ação."
    );
    assert_eq!(
        validate_hotkey_pair("", "ALT+F9").unwrap_err(),
        "O atalho não pode ser vazio."
    );
}

#[test]
fn test_slint_key_codes_and_alt_plus_letter() {
    // Slint Key::Alt (\u{0012}) não deve capturar como tecla final
    assert_eq!(
        parse_key_event("\u{0012}", true, false, false, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );
    // Slint Key::Control (\u{0011})
    assert_eq!(
        parse_key_event("\u{0011}", false, true, false, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );
    // Slint Key::Shift (\u{0010})
    assert_eq!(
        parse_key_event("\u{0010}", false, false, true, false),
        KeyCaptureOutcome::IgnoredModifierOnly
    );
    // Slint Key::Meta (\u{0017})
    assert_eq!(
        parse_key_event("\u{0017}", false, false, false, true),
        KeyCaptureOutcome::IgnoredModifierOnly
    );

    // Escape (\u{001b})
    assert_eq!(
        parse_key_event("\u{001b}", false, false, false, false),
        KeyCaptureOutcome::Cancelled
    );

    // Backspace (\u{0008}) e Delete (\u{007f})
    assert_eq!(
        parse_key_event("\u{0008}", false, false, false, false),
        KeyCaptureOutcome::ResetDefault
    );
    assert_eq!(
        parse_key_event("\u{007f}", false, false, false, false),
        KeyCaptureOutcome::ResetDefault
    );

    // ALT + G: Slint envia "g" com alt=true
    assert_eq!(
        parse_key_event("g", true, false, false, false),
        KeyCaptureOutcome::Captured("ALT+G".into())
    );

    // ALT + F8: Slint envia "\u{F70B}" com alt=true
    assert_eq!(
        parse_key_event("\u{F70B}", true, false, false, false),
        KeyCaptureOutcome::Captured("ALT+F8".into())
    );

    // F8 sozinho: Slint envia "\u{F70B}" com modifiers=false
    assert_eq!(
        parse_key_event("\u{F70B}", false, false, false, false),
        KeyCaptureOutcome::Captured("F8".into())
    );
}
