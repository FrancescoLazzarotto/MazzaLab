# MazzaLab

*[Read in English](README.md)*

Show control multipiattaforma per teatro, eventi dal vivo e installazioni:
riproduzione a cue di audio, video e luci, più il controllo dei dispositivi
intorno, tutto da un unico pulsante GO. MazzaLab gira su Windows, macOS e
Linux.

> **Stato:** sviluppo iniziale. Già funzionanti e testati: il modello dello
> show, il motore di riproduzione, i file `.mazzalab`, la riproduzione
> audio, il controllo remoto via OSC, il server dello show, l'interfaccia web
> (desktop, browser, tablet) e l'app desktop. Video, luci e dispositivi sono
> i prossimi passi. Vedi la [roadmap](docs/architecture/overview.md#roadmap).

## Punti di forza

- **Gira ovunque.** Un unico nucleo in Rust e un'app desktop nativa (Tauri)
  per Windows, macOS e Linux. La stessa interfaccia funziona in qualsiasi
  browser della rete, quindi tablet e telefoni diventano telecomandi senza
  installare nulla.
- **Motore deterministico.** La logica di riproduzione è pura e riceve il
  tempo dall'esterno. Gli show si possono simulare, testare in CI,
  rieseguire e replicare in modo identico su una macchina di backup.
- **Percorso audio professionale.** Decodifica in streaming (WAV, AIFF,
  FLAC, MP3, AAC, ALAC, Vorbis...), tagli e loop precisi al campione, un
  mixer in tempo reale senza lock con dissolvenze in dB (lineari o a S),
  stop senza click e dissolvenza di panic.
- **Controllo remoto compatibile.** L'OSC su UDP e TCP segue il dizionario
  OSC pubblico di QLab 5 (porta 53000), quindi le superfici di controllo
  esistenti continuano a funzionare.
- **File di show in testo.** I file `.mazzalab` sono JSON leggibile e
  confrontabile con git. Ogni modifica si può annullare.
- **Modello dei cue familiare.** Audio, Video, Testo, Mic, Camera, Luci,
  Fade, Network, MIDI, MIDI File, Timecode, Group (timeline, avvia il primo,
  avvia il primo ed entra, casuale, playlist), Start/Stop/Pause/Load/Reset/
  Devamp/Go To/Arm/Disarm/Target, Wait, Memo, Script, con pre-attesa,
  post-attesa, auto-continue e auto-follow.
- **Interfaccia in italiano e in inglese**, con la lingua selezionabile in
  qualsiasi momento.

## Avvio

Requisiti: Rust (versione fissata in `rust-toolchain.toml`) e Node.js 22+
per l'interfaccia web. Su Linux servono anche i pacchetti di sviluppo di ALSA
e WebKitGTK (vedi `.github/workflows/ci.yml`).

```sh
# Compila una volta l'interfaccia web
cd apps/mazzalab-web && npm ci && npm run build && cd ../..

# App desktop (server + finestra); il file viene creato al primo salvataggio
cargo run -p mazzalab-desktop -- mio-show.mazzalab

# ...oppure un server senza interfaccia, ad esempio su una macchina senza schermo
cargo run -p mazzalab-server -- mio-show.mazzalab --ui-dir apps/mazzalab-web/dist
```

Poi apri `http://<ip-della-macchina>:53080` da qualsiasi tablet o portatile
sulla stessa rete. L'OSC va inviato alla porta 53000 (per esempio `/go`,
`/cue/1/start`, `/cue/selected/name`).

Tastiera: **Spazio** GO · **Esc** panic · **↑/↓** selezione · **Invio**
sposta il playhead · **Ctrl+Z / Ctrl+Maiusc+Z** annulla/ripeti · **Ctrl+S**
salva.

### Sviluppo

```sh
cargo test --workspace                       # test Rust
cd apps/mazzalab-web && npm test             # test dell'interfaccia
npm run dev                                  # interfaccia con hot reload, collegata a un server su :53080
cargo run -p mazzalab-cli -- simulate examples/shows/demo.mazzalab --gos 4 --interval 15
```

Installer: dalla cartella `apps/mazzalab-desktop` esegui
`npx @tauri-apps/cli build --config tauri.bundle.conf.json`, dopo aver
compilato l'interfaccia web.

## Struttura del repository

| Percorso | Contenuto |
| --- | --- |
| `crates/mazzalab-core` | Modello dello show, validazione, modifiche all'albero dei cue |
| `crates/mazzalab-engine` | Motore di riproduzione deterministico |
| `crates/mazzalab-show` | Controller dello show: comandi, undo/redo, stato, gestione OSC |
| `crates/mazzalab-project` | Formato dei file `.mazzalab` |
| `crates/mazzalab-osc` | Parsing OSC, risposte, framing SLIP |
| `crates/mazzalab-audio` | Decodifica, mixer in tempo reale, uscita audio |
| `crates/mazzalab-{video,lighting,midi,network,timeline,ui,plugin}` | Sottosistemi previsti |
| `apps/mazzalab-server` | Server dello show in tempo reale: OSC, API HTTP/WebSocket, uscite |
| `apps/mazzalab-web` | Interfaccia operatore (React + TypeScript) |
| `apps/mazzalab-desktop` | App desktop (Tauri) con il server incorporato |
| `apps/mazzalab-cli` | Strumento da riga di comando `mazzalab` |
| `docs/` | Architettura, ADR, protocolli (in inglese) |

## Contesto

Il progetto di MazzaLab si basa su uno studio clean-room di quanto QLab 5
espone pubblicamente. Vedi l'[analisi di QLab](docs/architecture/qlab-analysis.md)
e l'[ADR 0001](docs/adr/0001-clean-room-qlab-analysis.md). MazzaLab non è
affiliato a Figure 53. QLab è un marchio di Figure 53, LLC.

## Licenza

Proprietaria: tutti i diritti riservati. Vedi [LICENSE](LICENSE).
