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

    match trimmed {
        "Escape" | "Key_Escape" => return KeyCaptureOutcome::Cancelled,
        "BackSpace" | "Key_BackSpace" | "Delete" | "Key_Delete" => {
            return KeyCaptureOutcome::ResetDefault;
        }
        "Control" | "Alt" | "Shift" | "Meta" | "Super" | "AltGr" | "Shift_L" | "Shift_R"
        | "Control_L" | "Control_R" | "Alt_L" | "Alt_R" => {
            return KeyCaptureOutcome::IgnoredModifierOnly;
        }
        _ => {}
    }

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

    let key_upper = trimmed.to_ascii_uppercase();
    parts.push(&key_upper);

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
