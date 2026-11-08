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
