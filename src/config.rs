//! Configurazione utente persistente: nickname, avatar, ultimo server.
//! File: `~/.pong_arena_config.json` (USERPROFILE su Windows, HOME su Unix).

use std::path::PathBuf;

pub const AVATARS: &[&str] = &[
    "★", "♦", "●", "▲", "■", "♠", "♣", "♥", "◆", "☯",
    "♞", "✦", "⚙", "∞", "Ω", "Σ", "Δ", "✿", "☀", "☁",
];

pub struct Config {
    pub nickname: String,
    pub avatar: String,
    pub last_server: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            nickname: String::new(),
            avatar: AVATARS[0].to_string(),
            last_server: String::new(),
        }
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pong_arena_config.json")
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn unescape_field(block: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\":\"", key);
    let start = block.find(&pat)? + pat.len();
    let tail = &block[start..];
    let mut result = String::new();
    let mut chars = tail.chars();
    loop {
        match chars.next()? {
            '"' => break,
            '\\' => match chars.next()? {
                '"' => result.push('"'),
                '\\' => result.push('\\'),
                'n' => result.push('\n'),
                c => result.push(c),
            },
            c => result.push(c),
        }
    }
    Some(result)
}

impl Config {
    pub fn load() -> Self {
        let path = config_path();
        let s = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Config::default(),
        };
        Config {
            nickname: unescape_field(&s, "nickname").unwrap_or_default(),
            avatar: unescape_field(&s, "avatar")
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| AVATARS[0].to_string()),
            last_server: unescape_field(&s, "last_server").unwrap_or_default(),
        }
    }

    pub fn save(&self) {
        let path = config_path();
        let json = format!(
            "{{\"nickname\":\"{}\",\"avatar\":\"{}\",\"last_server\":\"{}\"}}\n",
            escape(&self.nickname),
            escape(&self.avatar),
            escape(&self.last_server),
        );
        let _ = std::fs::write(path, json);
    }
}
