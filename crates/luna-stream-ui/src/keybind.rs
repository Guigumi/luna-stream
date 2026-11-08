//! Mapeamento, captura e validação de atalhos globais de teclado (Replay e Gravação).

pub const DEFAULT_REPLAY_HOTKEY: &str = "ALT+F8";
pub const DEFAULT_RECORD_HOTKEY: &str = "ALT+F9";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyCaptureOutcome {
    IgnoredModifierOnly,
    Cancelled,
    ResetDefault,
    Captured(String),
}

/// Formata o trigger XDG (`"ALT+F8"`) para exibição amigável na interface (`"Alt + F8"`).
pub fn format_hotkey_label(xdg_trigger: &str) -> String {
    let parts: Vec<&str> = xdg_trigger
        .split(['+', ' ', '-'])
        .filter(|s| !s.is_empty())
        .collect();

    let mut formatted = Vec::new();
    for part in parts {
        let lower = part.to_lowercase();
        let label = match lower.as_str() {
            "ctrl" | "control" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "shift" => "Shift".to_string(),
            "super" | "meta" => "Super".to_string(),
            f if f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit()) => {
                format!("F{}", &f[1..])
            }
            other => {
                let mut chars = other.chars();
                match chars.next() {
                    Some(first) => {
                        let upper_first = first.to_ascii_uppercase();
                        if other.len() == 1 {
                            upper_first.to_string()
                        } else {
                            format!("{upper_first}{}", chars.as_str())
                        }
                    }
                    None => String::new(),
                }
            }
        };
        if !label.is_empty() {
            formatted.push(label);
        }
    }
    formatted.join(" + ")
}

enum KeyInterpretation {
    Cancel,
    ResetDefault,
    ModifierOnly,
    Named(&'static str),
}

fn map_slint_key_name(text: &str) -> Option<KeyInterpretation> {
    match text {
        // Cancelamento (Escape)
        "\u{001b}" | "Escape" | "Key_Escape" => Some(KeyInterpretation::Cancel),

        // Restauração de padrão (Backspace / Delete)
        "\u{0008}" | "\u{007f}" | "BackSpace" | "Key_BackSpace" | "Delete" | "Key_Delete" => {
            Some(KeyInterpretation::ResetDefault)
        }

        // Modificadores puros do Slint (\u{0010}..\u{0018}) e identificadores textuais
        "\u{0010}" | "\u{0015}" | "Shift" | "Shift_L" | "Shift_R" | "ShiftL" | "ShiftR"
        | "\u{0011}" | "\u{0016}" | "Control" | "Control_L" | "Control_R" | "ControlL" | "ControlR"
        | "\u{0012}" | "\u{0013}" | "Alt" | "Alt_L" | "Alt_R" | "AltL" | "AltR" | "AltGr" | "Mode_switch"
        | "\u{0014}" | "CapsLock"
        | "\u{0017}" | "\u{0018}" | "Meta" | "Super" | "Meta_L" | "Meta_R" | "Super_L" | "Super_R" => {
            Some(KeyInterpretation::ModifierOnly)
        }

        // Teclas de função F1 a F24 (\u{F704}..\u{F71B})
        "\u{F704}" | "F1" | "f1" => Some(KeyInterpretation::Named("F1")),
        "\u{F705}" | "F2" | "f2" => Some(KeyInterpretation::Named("F2")),
        "\u{F706}" | "F3" | "f3" => Some(KeyInterpretation::Named("F3")),
        "\u{F707}" | "F4" | "f4" => Some(KeyInterpretation::Named("F4")),
        "\u{F708}" | "F5" | "f5" => Some(KeyInterpretation::Named("F5")),
        "\u{F709}" | "F6" | "f6" => Some(KeyInterpretation::Named("F6")),
        "\u{F70A}" | "F7" | "f7" => Some(KeyInterpretation::Named("F7")),
        "\u{F70B}" | "F8" | "f8" => Some(KeyInterpretation::Named("F8")),
        "\u{F70C}" | "F9" | "f9" => Some(KeyInterpretation::Named("F9")),
        "\u{F70D}" | "F10" | "f10" => Some(KeyInterpretation::Named("F10")),
        "\u{F70E}" | "F11" | "f11" => Some(KeyInterpretation::Named("F11")),
        "\u{F70F}" | "F12" | "f12" => Some(KeyInterpretation::Named("F12")),
        "\u{F710}" | "F13" | "f13" => Some(KeyInterpretation::Named("F13")),
        "\u{F711}" | "F14" | "f14" => Some(KeyInterpretation::Named("F14")),
        "\u{F712}" | "F15" | "f15" => Some(KeyInterpretation::Named("F15")),
        "\u{F713}" | "F16" | "f16" => Some(KeyInterpretation::Named("F16")),
        "\u{F714}" | "F17" | "f17" => Some(KeyInterpretation::Named("F17")),
        "\u{F715}" | "F18" | "f18" => Some(KeyInterpretation::Named("F18")),
        "\u{F716}" | "F19" | "f19" => Some(KeyInterpretation::Named("F19")),
        "\u{F717}" | "F20" | "f20" => Some(KeyInterpretation::Named("F20")),
        "\u{F718}" | "F21" | "f21" => Some(KeyInterpretation::Named("F21")),
        "\u{F719}" | "F22" | "f22" => Some(KeyInterpretation::Named("F22")),
        "\u{F71A}" | "F23" | "f23" => Some(KeyInterpretation::Named("F23")),
        "\u{F71B}" | "F24" | "f24" => Some(KeyInterpretation::Named("F24")),

        // Navegação e especiais comuns
        "\u{F700}" | "Up" => Some(KeyInterpretation::Named("UP")),
        "\u{F701}" | "Down" => Some(KeyInterpretation::Named("DOWN")),
        "\u{F702}" | "Left" => Some(KeyInterpretation::Named("LEFT")),
        "\u{F703}" | "Right" => Some(KeyInterpretation::Named("RIGHT")),
        "\u{F727}" | "Insert" => Some(KeyInterpretation::Named("INSERT")),
        "\u{F729}" | "Home" => Some(KeyInterpretation::Named("HOME")),
        "\u{F72B}" | "End" => Some(KeyInterpretation::Named("END")),
        "\u{F72C}" | "PageUp" => Some(KeyInterpretation::Named("PAGEUP")),
        "\u{F72D}" | "PageDown" => Some(KeyInterpretation::Named("PAGEDOWN")),
        "\u{F730}" | "Pause" => Some(KeyInterpretation::Named("PAUSE")),
        "\u{F731}" | "PrintScreen" => Some(KeyInterpretation::Named("PRINTSCREEN")),
        "\u{0009}" | "Tab" => Some(KeyInterpretation::Named("TAB")),
        "\u{0020}" | "Space" => Some(KeyInterpretation::Named("SPACE")),
        "\u{000a}" | "\r" | "Return" | "Enter" => Some(KeyInterpretation::Named("RETURN")),

        _ => None,
    }
}

/// Interpreta um evento de tecla do Slint (`KeyEvent`).
pub fn parse_key_event(
    text: &str,
    alt: bool,
    ctrl: bool,
    shift: bool,
    meta: bool,
) -> KeyCaptureOutcome {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return KeyCaptureOutcome::IgnoredModifierOnly;
    }

    let key_name = match map_slint_key_name(trimmed) {
        Some(KeyInterpretation::Cancel) => return KeyCaptureOutcome::Cancelled,
        Some(KeyInterpretation::ResetDefault) => return KeyCaptureOutcome::ResetDefault,
        Some(KeyInterpretation::ModifierOnly) => return KeyCaptureOutcome::IgnoredModifierOnly,
        Some(KeyInterpretation::Named(name)) => name.to_string(),
        None => {
            // Ignora códigos de controle não imprimíveis
            if trimmed.chars().any(|c| c.is_control()) {
                return KeyCaptureOutcome::IgnoredModifierOnly;
            }
            trimmed.to_ascii_uppercase()
        }
    };

    let mut parts = Vec::new();
    if ctrl {
        parts.push("CTRL");
    }
    if alt {
        parts.push("ALT");
    }
    if shift {
        parts.push("SHIFT");
    }
    if meta {
        parts.push("SUPER");
    }

    parts.push(&key_name);

    KeyCaptureOutcome::Captured(parts.join("+"))
}

fn normalize_hotkey(hotkey: &str) -> String {
    let mut parts: Vec<String> = hotkey
        .split(['+', ' ', '-'])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .collect();
    parts.sort();
    parts.join("+")
}

/// Valida se o atalho proposto é não-vazio e não colide com o outro atalho ativo.
pub fn validate_hotkey_pair(candidate: &str, other_hotkey: &str) -> Result<String, &'static str> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return Err("O atalho não pode ser vazio.");
    }

    let cand_norm = normalize_hotkey(trimmed);
    let other_norm = normalize_hotkey(other_hotkey);

    if cand_norm == other_norm {
        return Err("Este atalho já está em uso pela outra ação.");
    }

    let xdg_trigger = trimmed
        .split(['+', ' ', '-'])
        .filter(|s| !s.is_empty())
        .map(|s| s.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("+");

    Ok(xdg_trigger)
}
