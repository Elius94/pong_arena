//! Classifica persistente: salva vittorie, partite e punti per ogni giocatore.
//!
//! File: `~/.pong_arena_scores.json` (USERPROFILE su Windows, HOME su Unix).
//! Formato: array JSON di oggetti `{name, wins, games, points}`.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ScoreEntry {
    pub name: String,
    pub wins: u32,
    pub games: u32,
    pub points: u32, // vite sottratte agli avversari come last_hitter
}

fn scores_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".pong_arena_scores.json")
}

// ---------------------------------------------------------------------------
// JSON minimalista (nessuna dipendenza esterna).
// ---------------------------------------------------------------------------

fn to_json(entries: &[ScoreEntry]) -> String {
    let mut s = String::from("[\n");
    for (i, e) in entries.iter().enumerate() {
        let name = e.name.replace('\\', "\\\\").replace('"', "\\\"");
        s.push_str(&format!(
            "  {{\"name\":\"{}\",\"wins\":{},\"games\":{},\"points\":{}}}",
            name, e.wins, e.games, e.points
        ));
        if i + 1 < entries.len() {
            s.push(',');
        }
        s.push('\n');
    }
    s.push_str("]\n");
    s
}

fn parse_str_field(block: &str, key: &str) -> Option<String> {
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

fn parse_u32_field(block: &str, key: &str) -> Option<u32> {
    let pat = format!("\"{}\":", key);
    let start = block.find(&pat)? + pat.len();
    let tail = block[start..].trim_start();
    let end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    tail[..end].parse().ok()
}

fn parse_entry(block: &str) -> Option<ScoreEntry> {
    let name = parse_str_field(block, "name")?;
    let wins = parse_u32_field(block, "wins")?;
    let games = parse_u32_field(block, "games")?;
    let points = parse_u32_field(block, "points")?;
    Some(ScoreEntry { name, wins, games, points })
}

fn from_json(s: &str) -> Vec<ScoreEntry> {
    let mut entries = Vec::new();
    let mut rest = s;
    while let Some(ob) = rest.find('{') {
        rest = &rest[ob + 1..];
        if let Some(cb) = rest.find('}') {
            let block = &rest[..cb];
            rest = &rest[cb + 1..];
            if let Some(e) = parse_entry(block) {
                entries.push(e);
            }
        } else {
            break;
        }
    }
    entries
}

// ---------------------------------------------------------------------------
// API pubblica.
// ---------------------------------------------------------------------------

pub fn load() -> Vec<ScoreEntry> {
    let path = scores_path();
    match std::fs::read_to_string(&path) {
        Ok(s) => from_json(&s),
        Err(_) => Vec::new(),
    }
}

pub fn save(entries: &[ScoreEntry]) {
    let path = scores_path();
    let _ = std::fs::write(path, to_json(entries));
}

/// Aggiorna la classifica a fine partita.
/// `winner_name`: nome del vincitore (stringa vuota se nessuno, e.g. disconnessione).
/// `player_names`: tutti i nomi in ordine di pid.
/// `kills`: punti segnati da ciascun pid in questa partita.
pub fn update(winner_name: &str, player_names: &[String], kills: &[i32]) {
    let mut entries = load();
    for (i, name) in player_names.iter().enumerate() {
        let pts = kills.get(i).copied().unwrap_or(0) as u32;
        let won = !winner_name.is_empty() && name == winner_name;
        if let Some(entry) = entries.iter_mut().find(|e| e.name == *name) {
            entry.games += 1;
            if won {
                entry.wins += 1;
            }
            entry.points += pts;
        } else {
            entries.push(ScoreEntry {
                name: name.clone(),
                wins: if won { 1 } else { 0 },
                games: 1,
                points: pts,
            });
        }
    }
    save(&entries);
}

pub fn print_leaderboard() {
    let mut entries = load();
    if entries.is_empty() {
        println!("\n  Nessuna partita registrata ancora.\n");
        return;
    }
    entries.sort_by(|a, b| {
        b.points
            .cmp(&a.points)
            .then(b.wins.cmp(&a.wins))
            .then(a.name.cmp(&b.name))
    });

    let col_name = 22usize;
    let width = col_name + 30;
    println!();
    println!("  \x1b[1;36m▌ CLASSIFICA PONG ARENA ▐\x1b[0m");
    println!("  {}", "═".repeat(width));
    println!(
        "  \x1b[2m{:>3}  {:<col_name$}  {:>8}  {:>7}  {:>7}\x1b[0m",
        "#", "Nome", "Vittorie", "Partite", "Punti"
    );
    println!("  {}", "─".repeat(width));
    for (i, e) in entries.iter().enumerate() {
        let color = match i {
            0 => "\x1b[1;33m", // oro
            1 => "\x1b[1;37m", // argento
            2 => "\x1b[33m",   // bronzo
            _ => "\x1b[0m",
        };
        let name = if e.name.chars().count() > col_name {
            format!("{}…", &e.name[..col_name - 1])
        } else {
            e.name.clone()
        };
        println!(
            "  {color}{:>3}  {:<col_name$}  {:>8}  {:>7}  {:>7}\x1b[0m",
            i + 1,
            name,
            e.wins,
            e.games,
            e.points
        );
    }
    println!();
    println!(
        "  \x1b[2mFile: {}\x1b[0m\n",
        scores_path().display()
    );
}
