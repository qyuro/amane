use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::{Duration, Instant};

use amane::Service;

// niri answers this connection with every event, starting with the current layouts
const EVENT_STREAM: &str = "\"EventStream\"\n";

// the language chip stays up for about this long after a switch
const FLASH: Duration = Duration::from_millis(1800);
const FADE_IN: Duration = Duration::from_millis(90);
const FADE_OUT: Duration = Duration::from_millis(220);

/// Tracks niri's current keyboard layout and remembers when it last changed.
pub struct Layout {
    names: Vec<String>,
    codes: Vec<String>,
    index: usize,
    current: String,

    changed_at: Option<Instant>,
}

impl Service for Layout {
    fn new() -> Self {
        Self {
            names: Vec::new(),
            codes: config_codes(),
            index: 0,
            current: String::new(),
            changed_at: None,
        }
    }

    // niri sends the full layout list on connect, then just the new index on
    // every switch, so the socket is read instead of asking for it again
    fn listen() {
        loop {
            if let Some(stream) = connect() {
                let mut primed = false;

                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else {
                        break;
                    };

                    let layouts = parse_layouts(&line);
                    let switched = parse_switch(&line);

                    if layouts.is_none() && switched.is_none() {
                        continue;
                    }

                    let changed = {
                        let mut layout = Layout::write();

                        if let Some((names, index)) = layouts {
                            // the first event is just the state the shell connected to
                            if !primed {
                                primed = true;
                                layout.names = names;
                                layout.index = index;
                                layout.current =
                                    code_for(&layout.names, index, &layout.codes);

                                continue;
                            }

                            if layout.names == names && layout.index == index {
                                continue;
                            }

                            layout.names = names;
                            layout.index = index;
                        } else if let Some(index) = switched {
                            if !primed {
                                primed = true;
                                layout.index = index;
                                layout.current =
                                    code_for(&layout.names, index, &layout.codes);

                                continue;
                            }

                            if layout.index == index {
                                continue;
                            }

                            layout.index = index;
                        }

                        layout.current = code_for(&layout.names, layout.index, &layout.codes);
                        layout.changed_at = Some(Instant::now());

                        true
                    };

                    if changed {
                        crate::state::touch();
                    }
                }
            }

            // niri restarted or was not there yet
            thread::sleep(Duration::from_secs(2));
        }
    }
}

impl Layout {
    pub fn current(&self) -> &str {
        &self.current
    }

    /// True while the language chip is replacing the workspaces.
    pub fn flash_active(&self) -> bool {
        self.changed_at
            .is_some_and(|start| start.elapsed() < FLASH)
    }

    /// 0 before and after the flash, 1 while it holds, with short fades.
    pub fn flash_mix(&self) -> f32 {
        let Some(start) = self.changed_at else {
            return 0.0;
        };

        let elapsed = start.elapsed();

        if elapsed >= FLASH {
            return 0.0;
        }

        let fade_in = (elapsed.as_secs_f32() / FADE_IN.as_secs_f32()).min(1.0);
        let fade_out = ((FLASH - elapsed).as_secs_f32() / FADE_OUT.as_secs_f32()).min(1.0);

        smoothstep(fade_in.min(fade_out))
    }
}

fn connect() -> Option<UnixStream> {
    let path = env::var_os("NIRI_SOCKET")?;

    let mut stream = UnixStream::connect(path).ok()?;

    stream.write_all(EVENT_STREAM.as_bytes()).ok()?;

    Some(stream)
}

/*
 * niri's line is either the answer to `niri msg --json keyboard-layouts` or a
 * KeyboardLayoutsChanged event; both hold the same names and current index:
 * {"names":["English (US)","Russian"],"current_idx":0}
 */
/*
 * an actual switch is a different event, holding only the new index:
 * {"KeyboardLayoutSwitched":{"idx":1}}
 */
fn parse_switch(text: &str) -> Option<usize> {
    if !text.contains("\"KeyboardLayoutSwitched\"") {
        return None;
    }

    let marker = "\"idx\":";
    let start = text.find(marker)? + marker.len();

    let index: usize = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;

    Some(index)
}

fn parse_layouts(text: &str) -> Option<(Vec<String>, usize)> {
    let names = "\"names\":[";
    let start = text.find(names)? + names.len();
    let end = start + text[start..].find(']')?;

    let list = &text[start..end];

    let names: Vec<String> = if list.trim().is_empty() {
        Vec::new()
    } else {
        list.split("\",\"")
            .map(|name| name.trim_matches('"').trim().to_string())
            .collect()
    };

    let index = "\"current_idx\":";
    let start = end + text[end..].find(index)? + index.len();

    let index: usize = text[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()?;

    Some((names, index))
}

/*
 * niri only reports the layout names, but its config also lists the codes in
 * the same order, like `layout "us,ru"`; reading those is what lets the bar
 * print ru/us instead of Russian/English
 */
fn config_codes() -> Vec<String> {
    let Some(home) = env::var_os("HOME") else {
        return Vec::new();
    };

    let config = env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(home).join(".config"))
        .join("niri");

    let Ok(entries) = std::fs::read_dir(config) else {
        return Vec::new();
    };

    for entry in entries.flatten() {
        if entry.path().extension().and_then(|extension| extension.to_str()) != Some("kdl") {
            continue;
        }

        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };

        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("layout") else {
                continue;
            };

            // `layout { ... }` and comments have no quoted code list
            let Some(rest) = rest.trim_start().strip_prefix('"') else {
                continue;
            };

            let Some((list, _)) = rest.split_once('"') else {
                continue;
            };

            let codes: Vec<String> = list
                .split(',')
                .map(|code| {
                    code.trim()
                        .split('-')
                        .next()
                        .unwrap_or("")
                        .to_ascii_lowercase()
                })
                .filter(|code| !code.is_empty())
                .collect();

            if !codes.is_empty() {
                return codes;
            }
        }
    }

    Vec::new()
}

fn code_for(names: &[String], index: usize, codes: &[String]) -> String {
    if codes.len() == names.len() {
        if let Some(code) = codes.get(index) {
            return code.clone();
        }
    }

    names
        .get(index)
        .map(|name| name_code(name))
        .unwrap_or_default()
}

// a fallback for when the config is somewhere this could not read
fn name_code(name: &str) -> String {
    let lower = name.to_lowercase();

    if let Some(start) = lower.rfind('(') {
        if let Some(end) = lower[start..].find(')') {
            let code = &lower[start + 1..start + end];

            if code.len() == 2 {
                return String::from(code);
            }
        }
    }

    for (needle, code) in [
        ("russian", "ru"),
        ("english", "us"),
        ("ukrainian", "ua"),
        ("german", "de"),
        ("french", "fr"),
        ("spanish", "es"),
        ("italian", "it"),
        ("polish", "pl"),
        ("portuguese", "pt"),
        ("turkish", "tr"),
        ("czech", "cz"),
        ("dutch", "nl"),
        ("swedish", "se"),
        ("japanese", "jp"),
        ("korean", "kr"),
        ("chinese", "cn"),
    ] {
        if lower.contains(needle) {
            return String::from(code);
        }
    }

    lower.chars().take(2).collect()
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_layout_names_and_index() {
        let line =
            r#"{"names":["English (US)","Russian"],"current_idx":1}"#;

        assert_eq!(
            parse_layouts(line),
            Some((vec![String::from("English (US)"), String::from("Russian")], 1))
        );

        let line = r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)","Russian"],"current_idx":0}}}"#;

        assert_eq!(
            parse_layouts(line),
            Some((vec![String::from("English (US)"), String::from("Russian")], 0))
        );
    }

    #[test]
    fn reads_a_switch_event() {
        assert_eq!(parse_switch(r#"{"KeyboardLayoutSwitched":{"idx":1}}"#), Some(1));
        assert_eq!(parse_switch(r#"{"KeyboardLayoutSwitched":{"idx":0}}"#), Some(0));

        // the full list event is not a switch, and has its index under another name
        assert_eq!(
            parse_switch(
                r#"{"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["English (US)"],"current_idx":0}}}"#
            ),
            None
        );
    }

    #[test]
    fn names_become_codes() {
        assert_eq!(name_code("English (US)"), "us");
        assert_eq!(name_code("Russian"), "ru");
        assert_eq!(name_code("German"), "de");
    }
}
