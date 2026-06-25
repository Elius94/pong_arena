# Pong Arena

<img width="1145" height="853" alt="Animazione" src="https://github.com/user-attachments/assets/b624863d-e5f6-4ef3-94ff-1f1bf104b1f1" />

Pong **LAN adattivo** scritto in Rust, da terminale. Il campo **cambia forma in base a quanti si collegano**:

- **2 giocatori → Pong classico**: campo rettangolare, racchette a sinistra e a destra, lati corti come muri.
- **3+ giocatori → arena poligonale ("schiaccia 7")**: il campo diventa un poligono regolare a N lati e **ogni giocatore difende un lato**. Parti con **7 vite**; ogni volta che la palla passa dal tuo lato ne perdi una. A 0 vite sei **eliminato** e il tuo lato si **chiude** (diventa muro). **Vince l'ultimo rimasto.**

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

## Giocare in LAN

Sullo stesso PC che fa da **host**:

```bash
./pong_arena host                 # duello: aspetta 1 avversario
./pong_arena host --port 4000     # porta diversa
./pong_arena host --bots 3        # arena a 4 lati: tu + 3 bot
```

L'host apre una **lobby**: i giocatori si collegano, tu vedi il conteggio e quando sei pronto premi **INVIO** per avviare. Comunica agli altri il tuo IP LAN (`ip addr` su Linux/macOS, `ipconfig` su Windows).

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

---

## Comandi

| Tasto | Azione |
|-------|--------|
| `←` / `→` · `A` / `D` · `W` / `S` | muovi la racchetta lungo il tuo lato |
| `SPACE` | spara un proiettile (congela la racchetta avversaria) |
| `G` | lancia una granata (congela tutti gli avversari per qualche secondo) |
| `R` | rivincita (a fine partita) |
| `Q` / `Esc` / `Ctrl-C` | esci |

Ogni giocatore vede **il proprio lato in basso**: i comandi sono coerenti dal tuo punto di vista, qualunque lato ti tocchi.

> **Movimento fluido:** per il rilascio-tasto in tempo reale serve un terminale moderno (kitty, WezTerm, Ghostty, foot, Windows Terminal). Altrove si ricade su autorepeat + timeout: funziona, ma la racchetta è un filo meno reattiva.

---

## Power-up e armi

### Armi (sempre disponibili)

Ogni giocatore ha un caricatore di **proiettili** (barra `█░` in basso a destra) e un contatore di **granate** `◆x N`.

- **Proiettile** (`SPACE`): sparato dalla racchetta, congela la racchetta del giocatore che colpisce per qualche secondo. Il caricatore si ricarica automaticamente.
- **Granata** (`G`): esplode nell'arena e congela tutti gli avversari contemporaneamente. Chi viene colpito vede un **overlay lampeggiante** `⚡ GRANATA Ns ⚡` con il conto alla rovescia. Le granate si raccolgono con i power-up.

### Power-up (appaiono nell'arena)

Oggetti colorati compaiono nel campo a intervalli regolari (massimo 3 alla volta). La palla li attiva al contatto e assegna l'effetto al proprietario della racchetta che ha lanciato la palla.

| Colore | Nome | Effetto |
|--------|------|---------|
| 🟡 Oro | **Multiball** | Aggiunge una palla extra nell'arena |
| 🟣 Viola | **Paralysis** | Congela la racchetta di tutti gli avversari |
| 🔵 Acqua | **Capture** | Permette di catturare la palla (`SPACE`) e rilanciarla a piacere |
| ⚫ Viola scuro | **Black Hole** | Genera un **buco nero** al centro del campo per 7 secondi: tutte le palle vengono attratte verso il centro con forza gravitazionale crescente |

Il **buco nero** è visibile come un disco pulsante (alone esterno → orizzonte degli eventi → nucleo oscuro → singolarità luminosa) e persiste anche se il power-up viene sovrascritto da un altro prima che scada.

---

## Come è fatto

Server **autoritativo**: l'host simula tutto e manda uno *snapshot* a ogni client a ~60 fps; i client inviano solo il proprio "intento" di movimento e i flag azione (sparo, granata).

```
            ┌──────────────── HOST (autoritativo) ────────────────┐
 input  →   │  raccoglie input (locali + rete) + IA bot           │
 di rete    │  step fisico generico su N muri                     │   snapshot
            │  power-up, armi, buco nero, vite/eliminazione       │  ───────────►  client
            │  broadcast snapshot a tutti                         │   (rendering dal
            └─────────────────────────────────────────────────────┘    proprio lato)
```

Punti chiave:

- **Geometria unificata** (`arena.rs`): rettangolo per 2, poligono regolare per 3+. Ogni lato ha una normale interna; un muro è *solido* o *di un giocatore*.
- **Fisica generica** (`game.rs`): la palla è sotto-campionata per evitare il tunneling; rimbalza speculare sui muri solidi, mentre sulle racchette l'angolo dipende dal punto d'impatto (stile arcade) ed è sempre verso l'interno. La gravità del buco nero è applicata ad ogni sotto-passo con `a = G / dist`, con cap a `2 × MAX_SPEED`.
- **Vista per-giocatore** (`render.rs`): l'arena viene ruotata così che il tuo lato sia sempre in basso; l'input è mappato di conseguenza.
- **Vite / eliminazione**: lato scoperto → punto subìto → −1 vita; a 0 il lato si chiude e l'arena "si stringe"; ultimo in piedi vince. Con 2 giocatori questo equivale al Pong classico contato alla rovescia da 7.
- **Rete** (`net.rs`, `app.rs`): lobby con più client, un thread reader per ciascuno (l'ultimo messaggio non blocca mai il game loop), gestione pulita di disconnessioni e rivincita. Su Windows i socket accettati vengono esplicitamente rimessi in modalità bloccante dopo `accept()`.
- **Rendering senza flickering**: il frame è composto in un'unica stringa e inviato con un solo `flush()`. Le righe dell'interfaccia (HUD top/bottom) vengono sovrascritte con spazi a larghezza piena anziché usare sequenze di cancellazione (`\x1b[2K`), che su Windows ConHost causano un flash visibile.

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
| `render.rs` | framebuffer mezzo-blocco, vista ruotata, HUD, overlay, buco nero |
| `terminal.rs` | raw mode (RAII), lettura input/tasti |
| `net.rs` | thread reader per host (input) e guest (snapshot) |
| `app.rs` | lobby, loop host, loop guest, composizione frame |
| `main.rs` | CLI (`host` / `join`, `--port`, `--bots`) |

---

## Note

- Massimo **20 giocatori** (umani + bot).
- Le regole "schiaccia 7" (7 vite, eliminazione, ultimo vince) si cambiano in un punto solo: `LIVES_START` in `game.rs`.
- La logica pura (geometria, fisica, protocollo) è coperta da test (`cargo test`). Rendering e rete in tempo reale, per loro natura, si provano solo con un terminale vero e una LAN.
