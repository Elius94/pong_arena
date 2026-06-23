# Pong Arena 🟢🟣🟡

Pong **LAN adattivo** scritto in Rust, da terminale. Stessa filosofia della
versione a 2 giocatori, ma ora il campo **cambia forma in base a quanti si
collegano**:

- **2 giocatori → Pong classico**: campo rettangolare, racchette a sinistra e a
  destra, lati corti come muri.
- **3+ giocatori → arena poligonale ("schiaccia 7")**: il campo diventa un
  poligono regolare a N lati e **ogni giocatore difende un lato**. Parti con
  **7 vite**; ogni volta che la palla passa dal tuo lato ne perdi una. A 0 vite
  sei **eliminato** e il tuo lato si **chiude** (diventa muro). **Vince l'ultimo
  rimasto.**

Un solo motore fisico gestisce entrambe le modalità: l'arena è una lista di
"muri" (segmenti con normale interna) e basta cambiare quanti sono e chi li
possiede.

Niente engine, niente dipendenze grafiche: solo `crossterm` per il terminale.
Rendering a **mezzo blocco** (`▀`) per raddoppiare la risoluzione verticale e
colori a 24 bit.

---

## Compilare

```bash
cd pong_arena
cargo build --release
```

L'eseguibile è `target/release/pong_arena`.

## Giocare in LAN

Sullo stesso PC che fa da **host**:

```bash
./pong_arena host                 # duello: aspetta 1 avversario
./pong_arena host --port 4000     # porta diversa
./pong_arena host --bots 3        # arena a 4 lati: tu + 3 bot
```

L'host apre una **lobby**: i giocatori si collegano, tu vedi il conteggio e
quando sei pronto premi **INVIO** per avviare. Comunica agli altri il tuo IP LAN
(`ip addr` su Linux/macOS, `ipconfig` su Windows).

Dagli altri PC, per **unirsi**:

```bash
./pong_arena join 192.168.1.20            # IP dell'host
./pong_arena join 192.168.1.20 --port 4000
```

> Stessa rete locale, stessa porta (default **7878**). Se non si collegano,
> controlla il firewall sull'host.

### Provare da soli

Non hai 3 macchine sottomano? Usa i **bot**:

```bash
./pong_arena host --bots 4    # arena a 5 lati, tu contro 4 IA
```

I bot inseguono la proiezione della palla sul proprio lato: bastano per vedere
subito la modalità poligonale e per riempire i posti in una partita vera.

## Comandi

| Tasto | Azione |
|------|--------|
| `←` / `→` · `A` / `D` · `W` / `S` | muovi la racchetta lungo il tuo lato |
| `R` | rivincita (a fine partita) |
| `Q` / `Esc` / `Ctrl-C` | esci |

Ogni giocatore vede **il proprio lato in basso**: i comandi sono coerenti dal
tuo punto di vista, qualunque lato ti tocchi.

> **Movimento fluido:** per il rilascio-tasto in tempo reale serve un terminale
> moderno (kitty, WezTerm, Ghostty, foot, Windows Terminal). Altrove si ricade
> su autorepeat + timeout: funziona, ma la racchetta è un filo meno reattiva.

---

## Come è fatto

Server **autoritativo**: l'host simula tutto e manda uno *snapshot* a ogni
client a ~60 fps; i client inviano solo il proprio "intento" di movimento.

```
            ┌──────────────── HOST (autoritativo) ────────────────┐
 input  →   │  raccoglie input (locali + rete) + IA bot           │
 di rete    │  step fisico generico su N muri                     │   snapshot
            │  vite / eliminazione / vittoria                     │  ───────────►  client
            │  broadcast snapshot a tutti                         │   (rendering dal
            └─────────────────────────────────────────────────────┘    proprio lato)
```

Punti chiave:

- **Geometria unificata** (`arena.rs`): rettangolo per 2, poligono regolare per
  3+. Ogni lato ha una normale interna; un muro è *solido* o *di un giocatore*.
- **Fisica generica** (`game.rs`): la palla è sotto-campionata per evitare il
  tunneling; rimbalza speculare sui muri solidi, mentre sulle racchette
  l'angolo dipende dal punto d'impatto (stile arcade) ed è sempre verso
  l'interno. Niente trigonometria speciale per il numero di lati.
- **Vista per-giocatore** (`render.rs`): l'arena viene ruotata così che il tuo
  lato sia sempre in basso; l'input è mappato di conseguenza.
- **Vite / eliminazione**: lato scoperto → punto subìto → −1 vita; a 0 il lato
  si chiude e l'arena "si stringe"; ultimo in piedi vince. Con 2 giocatori
  questo equivale al Pong classico contato alla rovescia da 7.
- **Rete** (`net.rs`, `app.rs`): lobby con più client, un thread reader per
  ciascuno (l'ultimo messaggio non blocca mai il game loop), gestione pulita di
  disconnessioni e rivincita.

### File

| File | Ruolo |
|------|-------|
| `geom.rs` | vettori 2D, rotazioni, proiezioni |
| `arena.rs` | costruzione campo (rettangolo / poligono), muri, racchette |
| `game.rs` | stato autoritativo, fisica, vite/eliminazione, protocollo, bot |
| `render.rs` | framebuffer mezzo-blocco, vista ruotata, HUD, overlay |
| `terminal.rs` | raw mode (RAII), lettura input/tasti |
| `net.rs` | thread reader per host (input) e guest (snapshot) |
| `app.rs` | lobby, loop host, loop guest |
| `main.rs` | CLI (`host` / `join`, `--port`, `--bots`) |

## Note

- Massimo **8 giocatori** (la palette colori ne ha 8).
- Le regole "schiaccia 7" (7 vite, eliminazione, ultimo vince) sono una
  interpretazione: si cambiano in un punto solo, `LIVES_START` in `game.rs`.
- La logica pura (geometria, fisica, protocollo) è coperta da test
  (`cargo test`). Rendering e rete in tempo reale, per loro natura, si provano
  solo con un terminale vero e una LAN.
