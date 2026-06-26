# Pong Arena

<img width="1145" height="853" alt="Animazione" src="https://github.com/user-attachments/assets/b624863d-e5f6-4ef3-94ff-1f1bf104b1f1" />

<img width="1918" height="1147" alt="Animazione3" src="https://github.com/user-attachments/assets/b9fd12c8-b0a7-4abe-baa1-cd9fbbb5eb0a" />

Pong **LAN adattivo** scritto in Rust, da terminale. Il campo **cambia forma in base a quanti si collegano**:

- **2 giocatori → Pong classico**: campo rettangolare, racchette a sinistra e a destra, lati corti come muri.
- **3+ giocatori → arena poligonale ("schiaccia 7")**: il campo diventa un poligono regolare a N lati e **ogni giocatore difende un lato**. Parti con **7 vite** (configurabile); ogni volta che la palla passa dal tuo lato ne perdi una. A 0 vite sei **eliminato** e il tuo lato si **chiude** (diventa muro). **Vince l'ultimo rimasto.**

Un solo motore fisico gestisce entrambe le modalità: l'arena è una lista di "muri" (segmenti con normale interna) e basta cambiare quanti sono e chi li possiede.

Niente engine, niente dipendenze grafiche: solo `crossterm` per il terminale. Rendering a **mezzo blocco** (`▀`) per raddoppiare la risoluzione verticale e colori a 24 bit.

---

## Compilare

```bash
cd pong_arena
cargo build --release
```

L'eseguibile è `target/release/pong_arena`.

---

<img width="1917" height="1150" alt="image" src="https://github.com/user-attachments/assets/08c18a69-cd1e-449a-b3df-ec14627db615" />

## Menu interattivo

Lanciando `./pong_arena` senza argomenti si apre un **menu TUI** con logo animato e navigazione a tastiera:

| Voce | Descrizione |
|------|-------------|
| **Host** | Configura porta, numero di bot e vite iniziali, poi avvia la lobby |
| **Join** | Inserisci IP e porta dell'host e connettiti |
| **Scopri server LAN** | Ascolta i broadcast UDP dell'host e mostra la lista: seleziona un server e connettiti con Invio |
| **Classifica** | Tabella globale con vittorie, partite giocate e punti per ogni nickname, ordinata per punti |
| **Replay partite** | Lista dei replay salvati; `Invio` per guardare, `D` per eliminare |
| **Nickname / Avatar** | Imposta il tuo nome e icona (persistiti in `~/.pong_arena_config.json`) |
| **Esci** | Chiudi il programma |

Dopo ogni partita o replay il programma **torna automaticamente al menu**.

---

## Giocare in LAN

Sullo stesso PC che fa da **host**:

```bash
./pong_arena host                 # duello: aspetta 1 avversario
./pong_arena host --port 4000     # porta diversa
./pong_arena host --bots 3        # arena a 4 lati: tu + 3 bot
./pong_arena host --lives 5       # 5 vite per giocatore
```

L'host apre una **lobby**: i giocatori si collegano, tu vedi il conteggio e quando sei pronto premi **Invio** per avviare. Comunica agli altri il tuo IP LAN (`ip addr` su Linux/macOS, `ipconfig` su Windows) — oppure usa **Scopri server LAN** dal menu.

Dagli altri PC, per **unirsi**:

```bash
./pong_arena join 192.168.1.20            # IP dell'host
./pong_arena join 192.168.1.20 --port 4000
```

> Stessa rete locale, stessa porta (default **7878**). Se non si collegano, controlla il firewall sull'host.

### Provare da soli

Non hai più macchine sottomano? Usa i **bot**:

```bash
./pong_arena host --bots 4    # arena a 5 lati, tu contro 4 IA
```

I bot inseguono la proiezione della palla sul proprio lato: bastano per vedere subito la modalità poligonale e per riempire i posti in una partita vera.

### Scoperta automatica LAN

L'host trasmette un **beacon UDP** ogni 2 secondi sulla porta **5558**. Aprendo *Scopri server LAN* dal menu il client ascolta questi beacon e mostra la lista dei server attivi con indirizzo, porta e numero di giocatori. Seleziona un server e premi Invio per connetterti senza digitare l'IP.

---

## Comandi

| Tasto | Azione |
|-------|--------|
| `←` / `→` · `A` / `D` · `W` / `S` | muovi la racchetta lungo il tuo lato |
| `SPACE` | spara un proiettile (congela la racchetta avversaria) |
| `G` | lancia una granata (congela tutti gli avversari) |
| `R` | rivincita (a fine partita) |
| `ESC` / `Q` | **pausa** (partita solo) · **abbandona** (partita in rete) |
| `Ctrl-C` | esci immediatamente |

Ogni giocatore vede **il proprio lato in basso**: i comandi sono coerenti dal tuo punto di vista, qualunque lato ti tocchi.

> **Movimento fluido:** per il rilascio-tasto in tempo reale serve un terminale moderno (kitty, WezTerm, Ghostty, foot, Windows Terminal). Altrove si ricade su autorepeat + timeout: funziona, ma la racchetta è un filo meno reattiva.

---

## Pausa e abbandono

Durante una partita **solo** (host + soli bot) `ESC`/`Q` apre un overlay di pausa:
- il tempo si ferma mentre il menu è aperto;
- *Continua* riprende, *Esci* termina la sessione.

In una partita **in rete** `ESC`/`Q` apre una finestra di conferma abbandono:
- **host**: *"Sì — kick tutti"* chiude la partita per tutti;
- **guest**: *"Sì, abbandona"* disconnette solo il giocatore locale;
- il gioco continua in background mentre si decide.

---

## Spettatori (late-join)

Chi si collega **dopo** l'avvio di una partita diventa automaticamente **spettatore**: vede il gioco in diretta con una barra informativa in basso. Al termine del round può partecipare normalmente alla rivincita.

---

## Power-up e armi

### Armi (sempre disponibili)

Ogni giocatore ha un caricatore di **proiettili** (barra `█░` in basso a destra) e un contatore di **granate** `◆x N`.

- **Proiettile** (`SPACE`): sparato dalla racchetta, congela la racchetta del giocatore che colpisce per qualche secondo. Il caricatore si ricarica automaticamente.
- **Granata** (`G`): esplode nell'arena e congela tutti gli avversari contemporaneamente. Chi viene colpito vede un **overlay lampeggiante** `⚡ GRANATA Ns ⚡` con il conto alla rovescia. Le granate si raccolgono con i power-up.
- **Sniper** (power-up): attiva la modalità sniper — i proiettili attraversano l'arena ad altissima velocità e paralizzano chiunque tocchino (compresi i muri di rimbalzo).

### Power-up (appaiono nell'arena)

Oggetti colorati compaiono nel campo a intervalli regolari (massimo 3 alla volta). La **palla li attiva al contatto** e assegna l'effetto al proprietario della racchetta che ha lanciato la palla. In alternativa puoi **sparare** direttamente al power-up per raccoglierlo senza rischiare di lasciarlo all'avversario.

| Colore | Nome | Effetto |
|--------|------|---------|
| 🟡 Oro | **Multiball** | Aggiunge una palla extra nell'arena |
| 🟣 Viola | **Paralysis** | Congela la racchetta di tutti gli avversari |
| 🔵 Acqua | **Capture** | Permette di catturare la palla (`SPACE`) e rilanciarla a piacere |
| ⚫ Viola scuro | **Black Hole** | Genera un **buco nero** al centro del campo per 7 secondi |
| 🔴 Rosso | **Sniper** | Attiva la modalità cecchino per i prossimi proiettili |
| 🟢 Verde | **WidePaddle** | Allarga temporaneamente la tua racchetta |
| 💛 Giallo chiaro | **ExtraLife** | Aggiunge 1 vita al tuo contatore |

Il **buco nero** è visibile come un disco pulsante (alone esterno → orizzonte degli eventi → nucleo oscuro → singolarità luminosa). Tutte le palle vengono attratte verso il centro con forza gravitazionale crescente.

---

## Classifica persistente

Ogni partita aggiorna automaticamente `~/.pong_arena_scores.json`. Per visualizzare la classifica:

```bash
./pong_arena leaderboard
```

oppure apri **Classifica** dal menu TUI per la versione con medaglie oro/argento/bronzo.

---

## Replay

L'host **registra automaticamente** ogni partita. I file `.par` vengono salvati in `~/.pong_arena_replays/`.

Per guardare un replay:
- Dal menu → **Replay partite** → seleziona → Invio
- Oppure da CLI: `./pong_arena replay <file.par>`

### Comandi durante il replay

| Tasto | Azione |
|-------|--------|
| `←` / `→` | dimezza / raddoppia la velocità (1× → 2× → 4× → 8× → 16×) |
| `SPACE` / `Invio` | pausa / riprendi |
| `R` | ricomincia dall'inizio |
| `ESC` / `Q` | esci dal replay |

La barra del titolo mostra `▶ ×4  1234/5678  21%` con velocità, frame corrente e percentuale.

---

## Come è fatto

Server **autoritativo**: l'host simula tutto e manda uno *snapshot* a ogni client a ~60 fps; i client inviano solo il proprio "intento" di movimento e i flag azione (sparo, granata). Gli spettatori ricevono gli stessi snapshot ma non inviano nulla.

```
            ┌──────────────────── HOST (autoritativo) ────────────────────┐
 input  →   │  raccoglie input (locali + rete) + IA bot                   │
 di rete    │  step fisico generico su N muri                             │   snapshot
            │  power-up, armi, buco nero, vite/eliminazione               │  ──────────►  client/spettatori
            │  broadcast snapshot a tutti (client + spettatori)           │  ──────────►  file replay
            │  beacon UDP LAN ogni 2 s sulla porta 5558                   │
            └─────────────────────────────────────────────────────────────┘
```

Punti chiave:

- **Geometria unificata** (`arena.rs`): rettangolo per 2, poligono regolare per 3+. Ogni lato ha una normale interna; un muro è *solido* o *di un giocatore*.
- **Fisica generica** (`game.rs`): la palla è sotto-campionata per evitare il tunneling; rimbalza speculare sui muri solidi, mentre sulle racchette l'angolo dipende dal punto d'impatto (stile arcade) ed è sempre verso l'interno. La gravità del buco nero è applicata ad ogni sotto-passo con `a = G / dist`, con cap a `2 × MAX_SPEED`.
- **Vista per-giocatore** (`render.rs`): l'arena viene ruotata così che il tuo lato sia sempre in basso; l'input è mappato di conseguenza.
- **Vite / eliminazione**: lato scoperto → punto subìto → −1 vita; a 0 il lato si chiude e l'arena "si stringe"; ultimo in piedi vince. Con 2 giocatori equivale al Pong classico contato alla rovescia.
- **Rete** (`net.rs`, `app.rs`): lobby con fino a **40 giocatori** (umani + bot), un thread reader per ciascuno (l'ultimo messaggio non blocca mai il game loop), gestione pulita di disconnessioni e rivincita.
- **Rendering senza flickering**: il frame è composto in un'unica stringa e inviato con un solo `flush()`. Le righe dell'interfaccia vengono sovrascritte con spazi a larghezza piena anziché usare sequenze di cancellazione, che su Windows ConHost causano un flash visibile.
- **Scoperta LAN**: beacon UDP broadcast ogni 2 s sulla porta 5558 (`DISCOVERY_PORT`); il menu ascolta e aggiorna la lista in tempo reale.
- **Replay**: ogni snapshot è codificato come riga `FRAME …` nel file `.par`. La funzione `replay::save()` persiste su disco; `run_replay()` decodifica con `Snapshot::decode()` e compone i frame con `compose()`.

### Protocollo snapshot

Ogni riga inviata dal server ha la forma:

```
S <phase> <countdown> <winner> <n_players> [<paddle> <lives> <alive>]... <n_balls> [<bx> <by>]... <n_weapons> [<ammo> <freeze> <freezetimer> <grenades> <cap>]... <n_items> [<x> <y> <kind>]... <black_hole_timer>
```

Il byte `cap` è un bitfield: bit 0 = capture_ready, bit 1 = palla trattenuta, bit 2 = congelato da granata.

### File

| File | Ruolo |
|------|-------|
| `geom.rs` | vettori 2D, rotazioni, proiezioni |
| `arena.rs` | costruzione campo (rettangolo / poligono), muri, racchette |
| `game.rs` | stato autoritativo, fisica, power-up, armi, buco nero, bot |
| `render.rs` | framebuffer mezzo-blocco, vista ruotata, HUD, overlay pausa/spettatore |
| `terminal.rs` | raw mode (RAII), lettura input/tasti (pause, confirm, quit separati) |
| `net.rs` | thread reader per host (input) e guest (snapshot) |
| `app.rs` | lobby, loop host (con pausa/spettatori/replay/LAN), loop guest, loop replay |
| `menu.rs` | menu TUI (Host/Join/Scopri/Classifica/Replay/Nickname/Avatar) |
| `replay.rs` | salvataggio e caricamento file `.par` |
| `scores.rs` | classifica persistente JSON |
| `config.rs` | configurazione persistente (nickname, avatar, ultimo server) |
| `main.rs` | CLI (`host` / `join` / `leaderboard`, opzioni) + loop menu |

---

## Note

- Massimo **40 giocatori** (umani + bot).
- Le regole "schiaccia N" (vite, eliminazione, ultimo vince) si configurano con `--lives N` da CLI o dalla voce *Vite* nel menu Host.
- La logica pura (geometria, fisica, protocollo) è coperta da test (`cargo test`). Rendering e rete in tempo reale, per loro natura, si provano solo con un terminale vero e una LAN.
- I replay vengono salvati solo se la partita dura almeno ~5 secondi (> 300 frame).
