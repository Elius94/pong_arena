//! Salvataggio e caricamento dei replay di partita.
//!
//! Ogni partita ospitata viene registrata come sequenza di snapshot (`FRAME`).
//! I file vengono salvati in `~/.pong_arena_replays/<timestamp>.par`.

use std::io;
use std::path::{Path, PathBuf};

pub fn replays_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pong_arena_replays")
}

// ---------------------------------------------------------------------------
// Formattazione timestamp Unix → stringa leggibile (UTC approssimativo).
// ---------------------------------------------------------------------------
fn month_days(m: u32) -> u32 {
    [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m.saturating_sub(1) as usize).min(11)]
}

pub fn format_ts(ts: u64) -> String {
    let secs  = (ts % 60) as u32;
    let mins  = ((ts / 60) % 60) as u32;
    let hours = ((ts / 3600) % 24) as u32;
    let mut days = ts / 86400;
    let mut year = 1970u32;
    loop {
        let leap = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) { 366 } else { 365 };
        if days < leap { break; }
        days -= leap;
        year += 1;
    }
    let mut month = 1u32;
    loop {
        let md = month_days(month);
        if days < md as u64 { break; }
        days -= md as u64;
        month += 1;
    }
    let day = days as u32 + 1;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hours, mins, secs)
}

// ---------------------------------------------------------------------------
// API pubblica.
// ---------------------------------------------------------------------------

/// Metadati di un replay (letti scansionando l'header senza caricare tutti i frame).
#[allow(dead_code)]
pub struct ReplayInfo {
    pub path: PathBuf,
    pub timestamp: u64,
    pub display: String,
    pub frame_count: usize,
    pub names: Vec<String>,
}

/// Scansiona la directory replay e restituisce la lista ordinata per data (più recente prima).
pub fn list() -> Vec<ReplayInfo> {
    let dir = replays_dir();
    let mut infos = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return infos,
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("par") {
            continue;
        }
        let ts = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let (names, frame_count) = scan_header(&path);
        let players: String = if names.is_empty() {
            "?".to_string()
        } else {
            names.iter().take(4).cloned().collect::<Vec<_>>().join(" vs ")
        };
        let display = format!("{}  ·  {}  ({} frame)", format_ts(ts), players, frame_count);
        infos.push(ReplayInfo { path, timestamp: ts, display, frame_count, names });
    }
    infos.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    infos
}

fn scan_header(path: &Path) -> (Vec<String>, usize) {
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return (Vec::new(), 0),
    };
    let mut names = Vec::new();
    let mut frame_count = 0usize;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("NAMES ") {
            names = rest.split('|').map(|s| s.to_string()).collect();
        } else if line.starts_with("FRAME ") {
            frame_count += 1;
        }
    }
    (names, frame_count)
}

/// Dati completi di un replay.
pub struct ReplayData {
    pub names: Vec<String>,
    pub frames: Vec<String>,
}

/// Carica un replay da file.
pub fn load(path: &Path) -> io::Result<ReplayData> {
    let content = std::fs::read_to_string(path)?;
    let mut names = Vec::new();
    let mut frames = Vec::new();
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("NAMES ") {
            names = rest.split('|').map(|s| s.to_string()).collect();
        } else if let Some(rest) = line.strip_prefix("FRAME ") {
            frames.push(format!("{}\n", rest));
        }
    }
    Ok(ReplayData { names, frames })
}

/// Salva un replay su file; restituisce il percorso del file creato.
pub fn save(frames: &[String], names: &[String]) -> io::Result<PathBuf> {
    if frames.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "nessun frame"));
    }
    let dir = replays_dir();
    std::fs::create_dir_all(&dir)?;

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let path = dir.join(format!("{}.par", ts));

    let mut content = String::from("REPLAY pong_arena v1\n");
    content.push_str(&format!("NAMES {}\n", names.join("|")));
    for frame in frames {
        content.push_str("FRAME ");
        content.push_str(frame.trim_end_matches('\n'));
        content.push('\n');
    }

    std::fs::write(&path, &content)?;
    Ok(path)
}

/// Elimina un file replay.
pub fn delete(path: &Path) -> io::Result<()> {
    std::fs::remove_file(path)
}
