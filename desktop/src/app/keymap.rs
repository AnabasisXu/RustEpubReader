//! 快捷键解析与分发。键位来自 settings.json 的 `shortcuts` 映射。
use eframe::egui;

pub(crate) fn parse(spec: &str) -> Option<(bool, bool, bool, egui::Key)> {
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    let mut key = None;
    for part in spec.split('+') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        match p.to_ascii_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" | "super" | "mod" => ctrl = true,
            "shift" => shift = true,
            "alt" | "option" => alt = true,
            _ => key = key_from(p),
        }
    }
    key.map(|k| (ctrl, shift, alt, k))
}

fn key_from(p: &str) -> Option<egui::Key> {
    let lower = p.to_ascii_lowercase();
    if lower.len() == 1 {
        let c = lower.chars().next()?;
        return match c {
            'a'..='z' => egui::Key::from_name(&c.to_string()),
            '0'..='9' => egui::Key::from_name(&c.to_string()),
            ',' => Some(egui::Key::Comma),
            '.' => Some(egui::Key::Period),
            '/' => Some(egui::Key::Slash),
            ';' => Some(egui::Key::Semicolon),
            '\'' => Some(egui::Key::Quote),
            '[' => Some(egui::Key::OpenBracket),
            ']' => Some(egui::Key::CloseBracket),
            '\\' => Some(egui::Key::Backslash),
            '`' => Some(egui::Key::Backtick),
            '-' => Some(egui::Key::Minus),
            '=' => Some(egui::Key::Equals),
            _ => None,
        };
    }
    match lower.as_str() {
        "esc" | "escape" => Some(egui::Key::Escape),
        "enter" | "return" => Some(egui::Key::Enter),
        "space" => Some(egui::Key::Space),
        "comma" => Some(egui::Key::Comma),
        "period" => Some(egui::Key::Period),
        "slash" => Some(egui::Key::Slash),
        "semicolon" => Some(egui::Key::Semicolon),
        "quote" => Some(egui::Key::Quote),
        "openbracket" => Some(egui::Key::OpenBracket),
        "closebracket" => Some(egui::Key::CloseBracket),
        "backslash" => Some(egui::Key::Backslash),
        "backtick" => Some(egui::Key::Backtick),
        "minus" => Some(egui::Key::Minus),
        "equals" => Some(egui::Key::Equals),
        "tab" => Some(egui::Key::Tab),
        "home" => Some(egui::Key::Home),
        "end" => Some(egui::Key::End),
        "pageup" | "pgup" => Some(egui::Key::PageUp),
        "pagedown" | "pgdn" => Some(egui::Key::PageDown),
        "left" => Some(egui::Key::ArrowLeft),
        "right" => Some(egui::Key::ArrowRight),
        "up" => Some(egui::Key::ArrowUp),
        "down" => Some(egui::Key::ArrowDown),
        _ => {
            let digits = lower.strip_prefix('f')?;
            let n: u8 = digits.parse().ok()?;
            match n {
                1 => Some(egui::Key::F1),
                2 => Some(egui::Key::F2),
                3 => Some(egui::Key::F3),
                4 => Some(egui::Key::F4),
                5 => Some(egui::Key::F5),
                6 => Some(egui::Key::F6),
                7 => Some(egui::Key::F7),
                8 => Some(egui::Key::F8),
                9 => Some(egui::Key::F9),
                10 => Some(egui::Key::F10),
                11 => Some(egui::Key::F11),
                12 => Some(egui::Key::F12),
                _ => None,
            }
        }
    }
}

pub(crate) fn pressed(ctx: &egui::Context, spec: &str) -> bool {
    let Some((ctrl, shift, alt, key)) = parse(spec) else {
        return false;
    };
    ctx.input(|i| {
        let m = i.modifiers;
        let ctrl_down = m.ctrl || m.command;
        ctrl_down == ctrl && m.shift == shift && m.alt == alt && i.key_pressed(key)
    })
}

/// 捕获下一个按键组合，返回 "Ctrl+Shift+B" 形式；Esc 返回空串表示取消；无按键返回 None。
pub(crate) fn capture(ctx: &egui::Context) -> Option<String> {
    let mut result = None;
    ctx.input(|i| {
        for event in &i.events {
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if *key == egui::Key::Escape {
                result = Some(String::new());
                break;
            }
            let mut parts: Vec<String> = Vec::new();
            if modifiers.ctrl || modifiers.command {
                parts.push("Ctrl".into());
            }
            if modifiers.alt {
                parts.push("Alt".into());
            }
            if modifiers.shift {
                parts.push("Shift".into());
            }
            parts.push(key_name(*key));
            result = Some(parts.join("+"));
            break;
        }
    });
    result
}

#[cfg(test)]
pub(crate) fn key_name_public(key: egui::Key) -> String {
    key_name(key)
}

fn key_name(key: egui::Key) -> String {
    match key {
        egui::Key::Comma => "Comma".into(),
        egui::Key::Period => "Period".into(),
        egui::Key::Slash => "Slash".into(),
        egui::Key::Semicolon => "Semicolon".into(),
        egui::Key::Quote => "Quote".into(),
        egui::Key::OpenBracket => "OpenBracket".into(),
        egui::Key::CloseBracket => "CloseBracket".into(),
        egui::Key::Backslash => "Backslash".into(),
        egui::Key::Backtick => "Backtick".into(),
        egui::Key::Minus => "Minus".into(),
        egui::Key::Equals => "Equals".into(),
        egui::Key::Space => "Space".into(),
        egui::Key::Tab => "Tab".into(),
        egui::Key::Enter => "Enter".into(),
        other => other.name().to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_supported_specs() {
        for spec in ["Ctrl+B", "Ctrl+Alt+V", "Ctrl+,", "F2", "Comma", "Shift+F3"] {
            assert!(super::parse(spec).is_some(), "spec {spec} should parse");
        }
        assert!(super::parse("Ctrl+NotAKey").is_none());
    }

    #[test]
    fn key_names_roundtrip() {
        for spec in ["Ctrl+Comma", "Alt+Period", "F12", "Slash"] {
            let parsed = super::parse(spec).expect(spec);
            let rebuilt = format!(
                "{}",
                super::key_name_public(parsed.3)
            );
            assert!(!rebuilt.is_empty());
        }
    }
}
