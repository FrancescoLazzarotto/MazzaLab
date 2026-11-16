# Piano: MazzaLab oltre QLab

**Obiettivo:** un software di show control migliore di QLab che gira
ovunque: Windows, macOS, Linux, server senza schermo e qualsiasi browser o
tablet come telecomando.

Questo è il piano di tutto il lavoro che manca. È diviso in fasi; dentro
ogni fase le voci sono in ordine di priorità. Spunta le voci man mano
(`- [x]`). Le informazioni su QLab vengono solo dall'analisi clean room in
[qlab-analysis.md](architecture/qlab-analysis.md): niente disassemblaggio,
niente file di QLab nel repository.

## Cosa vuol dire "meglio di QLab"

Tre condizioni, tutte necessarie:

1. **Parità funzionale.** Ogni cosa che un tecnico fa con QLab 5 si fa
   anche con MazzaLab: audio, video, luci, MIDI, timecode, rete,
   collaborazione, script.
2. **Vantaggi che QLab non ha.** Multipiattaforma, interfaccia web su
   qualsiasi dispositivo, file di show in testo, motore deterministico
   (simulazione, test, replay, backup esatto), formati aperti (GDTF,
   CLAP/VST3, schema aperto dei dispositivi), render node remoti, API
   pubblica.
3. **Affidabilità misurata.** Numeri verificati in CI e su hardware vero,
   non impressioni:
   - latenza GO → suono sotto i 5 ms con buffer da 128 campioni;
   - partenze sincronizzate al campione tra cue dello stesso gruppo;
   - 8 ore di show continuo senza xrun né crescita di memoria;
   - 0 crash nella suite di test di carico;
   - failover sul backup in meno di 1 secondo.

## Stato attuale (novembre 2026)

Fatto: modello dei cue e motore deterministico (attese, continue, gruppi
in tutti i modi, fade, panic, undo/redo), file di show versionato, OSC
compatibile con QLab (sottoinsieme, UDP/TCP con SLIP, `/updates`), server
con API HTTP/WebSocket, app desktop con menu File e avviso per le
modifiche non salvate, CLI con simulazione, CI su tre sistemi.

Audio: scelta del dispositivo con frequenza e buffer (cambio a caldo),
matrice di routing per cue, fade integrati, meter di picco per uscita,
precaricamento dei cue al playhead, forma d'onda, modifica del livello
mentre il cue suona.

Rete e MIDI: i Network cue inviano OSC e testo su UDP e TCP verso le
patch di rete dello show; i cue MIDI inviano note, CC, program change,
SysEx e MSC verso le patch MIDI.

Luci: i cue Light impostano canali DMX, fixture e gruppi del patch per
nome, con parametri, colori e tempi di fade; gli universi escono in
Art-Net o sACN.

Affidabilità: autosave delle modifiche non salvate, backup delle ultime
dieci versioni a ogni salvataggio, recupero automatico dopo un crash.

Controllo: cue Load, Reset, Devamp e Target attivi, carico a un punto
qualsiasi con un clic sulla forma d'onda, cue cart, trigger MIDI con
apprendimento, preascolto in cuffia.

Interfaccia: barra dei tipi di cue, selezione multipla, copia/incolla,
duplica, raggruppa, rinumera, drag & drop (riordino e import di file
audio), modifica in linea, inspector a schede con forma d'onda e matrice,
pannello uscita audio, impostazioni (generale, audio, rete, scorciatoie),
modalità Show, segnalazione dei cue rotti, avvisi all'operatore, tasti
rapidi per cue.

Stima della copertura rispetto a QLab: 45–50% delle funzioni. Le
fondamenta (motore, protocollo, server) non vanno riscritte.

## Ordine delle fasi

```
Fase 0 (basi) ──► Fase 1 (audio) ──┬──► Fase 3 (trigger e OSC) ──► Fase 4 (MIDI, timecode, rete)
                  Fase 2 (UI) ─────┘                                     │
                                                                         ▼
                  Fase 5 (luci) ──► Fase 6 (video) ──► Fase 7 (collaborazione e backup) ──► Fase 8 (script e plugin)

Fase 9 (interoperabilità) e Fase 10 (prodotto e qualità) corrono in parallelo a tutto.
```

Dimensioni indicative: S = giorni, M = 1–3 settimane, L = 1–2 mesi,
XL = più mesi.

---

## 0. Subito

- [x] Push di `main`.
- [ ] Controllare che la CI passi su Linux, Windows e macOS (il plugin
      dei dialoghi può richiedere pacchetti di sistema in più su Linux).
- [ ] Rinominare la cartella locale `CuLab` in `MazzaLab`, con tutte le
      sessioni chiuse.
- [ ] Scegliere l'hardware di riferimento per i test: una scheda audio
      multicanale con ASIO, un'interfaccia DMX USB, un controller MIDI, un
      generatore di timecode.

## Fase 0 — Basi comuni (L)

Lavori che servono a tutte le fasi successive. Conviene farli prima.

- [ ] **Orologio audio come sorgente del tempo.** Il motore avanza con il
      contatore di campioni del dispositivo audio, non con l'orologio di
      sistema. Senza dispositivo audio si usa un orologio monotono.
- [ ] **Cambio di show senza riavviare il server.** Oggi Apri e Salva con
      nome riavviano il server: la riproduzione si ferma e i tablet si
      riconnettono. Va sostituito con uno scambio dello show dentro
      l'attore.
- [ ] **Impostazioni dello show estese**, con migrazione del formato alla
      versione 2 e test di migrazione: patch audio, stage video, patch
      luci, dispositivi MIDI e di rete, mappa dei tasti, template dei cue.
- [ ] **Valori `/live`** separati dallo show salvato (modifiche durante la
      recita che non sporcano il file).
- [ ] **Registro degli input dell'operatore e replay**: ogni GO, stop e
      modifica viene registrato con il suo istante; il replay riproduce una
      recita identica. Serve per il debug e per il backup (Fase 7).
- [ ] **Gestione dei media**: cartella dei media dello show, "Consolida"
      (copia nella cartella i media che stanno altrove) e ricollegamento dei
      file mancanti per nome, in una cartella scelta, sono fatti. Manca l'avviso
      se un media cambia su disco.
- [x] **Autosave, backup a rotazione e recupero dopo un crash**.
- [ ] **Registro dei backend di uscita**: audio, luci, video e dispositivi
      si configurano e si riavviano senza fermare il server.
- [x] **Pannello diagnostica** (dalla riga in basso, sulla connessione):
      carico del thread audio con l'andamento dell'ultimo minuto, callback in
      ritardo (i probabili xrun), uptime, client collegati, cue in corso e
      registro degli input. La memoria e la latenza non sono riportate.

## Fase 1 — Audio (XL)

Parità con QLab e poi oltre.

### Dispositivi e routing
- [x] Elenco e scelta del dispositivo, frequenza di campionamento e
      dimensione del buffer.
- [ ] Driver a bassa latenza: ASIO su Windows c'è come opzione di
      compilazione (`--features asio`, verificata in CI; manca la prova
      su una scheda vera e la licenza Steinberg per distribuirla).
      CoreAudio su macOS è già il default; mancano JACK e PipeWire su
      Linux.
- [ ] Più dispositivi insieme, con compensazione della deriva di clock.
- [ ] **Patch di uscita** con nome: dispositivo, canali, livello, mute,
      polarità e ritardo per canale.
- [x] **Matrice per cue**: canali del file → uscite della patch, con
      livello per incrocio.
- [x] Uscita di preascolto (cuffia) separata, per provare un cue senza
      mandarlo in sala.

### Riproduzione
- [x] Precaricamento (autoload): i cue audio al playhead sono decodificati
      prima del GO.
- [ ] Partenze allineate al campione per i cue che partono nello stesso
      istante (gruppi timeline).
- [x] Slice e loop per slice; il cue Devamp funziona davvero.
- [ ] Cambio di velocità con intonazione mantenuta (time-stretch; verificare
      la licenza della libreria scelta).
- [x] Fade integrati in apertura e chiusura di ogni cue.
- [ ] Formati: WAV, AIFF, FLAC, MP3, AAC, OGG, Opus, più l'audio dei file
      video.
- [x] Probe della durata anche quando un file viene cambiato dalla UI.

### Fade e dinamica
- [x] Fade cue: livello assoluto o relativo, livelli per incrocio della
      matrice, stop del cue di destinazione alla fine, curva lineare o a S;
      su un group sfuma tutti i suoni dentro. Mancano i fade di velocità e le
      curve personalizzate.
- [x] Fade and stop others (stessa lista, tutte le liste) e duck others con
      ripristino automatico, dall'inspector dei cue Audio e Video.
- [x] Normalizzazione del volume in LUFS, con analisi in background (obiettivo in
      Impostazioni, Audio; il volume misurato e il compenso nell'inspector e nel
      pannello Media).

### Misura ed effetti
- [ ] Meter peak, RMS e LUFS per cue, patch e uscita.
- [ ] Effetti integrati sulle uscite: EQ parametrico, compressore,
      limitatore, delay.
- [ ] **Plugin CLAP e VST3** su cue e uscite, su tutti i sistemi (QLab
      supporta solo Audio Unit, quindi solo macOS).
- [ ] **Spazializzazione a oggetti** (VBAP/DBAP) su configurazioni
      multi-altoparlante, con editor grafico della sala.

### Ingressi
- [ ] Mic cue: ingressi dal vivo con routing ed effetti, a bassa latenza.

### Prove
- [ ] Benchmark del mixer e test di carico (128 voci).
- [ ] Misura della latenza GO → suono su hardware vero.
- [ ] Test di durata di 8 ore senza xrun.

## Fase 2 — Interfaccia operatore (XL)

### Editing
- [x] Trascinare i cue per riordinarli e spostarli nei gruppi
      (`move_cue` è già nel server).
- [x] Trascinare file dal sistema operativo o dal browser per creare cue.
- [x] Copia, incolla, duplica; selezione multipla e modifica di gruppo
      nell'inspector.
- [x] Rinumerazione e auto-numerazione con passo configurabile.
- [x] Template dei cue: si salva la cue selezionata e si crea da un menu nella barra.
- [x] Ricerca e filtri (per tipo, colore, flag, testo in numero, nome e note).
- [x] Gestione delle cue list: aggiungere, rinominare, eliminare, anche
      liste cart, e due liste affiancate (per dispositivo).
- [ ] Inspector completo per ogni tipo di cue.
- [x] Browser dei media, con caricamento dei file anche da tablet.

### Viste
- [x] Forma d'onda con zoom, punti di inizio e fine, slice e fade
      trascinabili, preascolto. Manca lo scrub.
- [x] Vista timeline per i gruppi timeline, con i pre-wait modificabili
      graficamente (trascinando le barre).
- [x] Cue cart: griglia di pulsanti per gli effetti a chiamata.
- [ ] Pannello dei cue attivi: pausa, stop, avanzamento e tempo residuo
      ci sono. Mancano fade out e livelli dal vivo: servono comandi nel
      server che non sporchino lo show.
- [ ] Pannello impostazioni dello show: doppio GO, durata del panic, nome,
      cue di apertura e chiusura.

### Modalità di lavoro
- [ ] Modalità show e modalità edit, con blocco e password.
- [ ] Mappa dei tasti configurabile: i tasti del trasporto (GO, stop, pausa,
      standby, panic) si cambiano per dispositivo; mancano gli altri tasti.
- [x] Vista "solo show" per tablet (`/?view=show`): GO grande, standby, note,
      cue in corso, stop e panic.
- [x] App web installabile (PWA): manifest e service worker che tiene la
      shell, e la connessione riprova da sola.

### Qualità dell'interfaccia
- [ ] Accessibilità: uso completo da tastiera, screen reader, contrasto
      alto, ingrandimento (punto debole di molti software da palco). Fatti:
      etichette sui pulsanti, annunci di GO e standby, movimento ridotto,
      tema ad alto contrasto, anello di focus. Mancano la forma d'onda da
      tastiera e la prova con uno screen reader vero.
- [x] Temi chiaro, scuro, "buio di sala" a luminosità minima e alto contrasto.
- [x] Sistema di traduzioni aperto oltre a IT/EN: un JSON per lingua, inglese
      come riferimento, test sulle chiavi (`docs/translating.md`).
- [ ] Test end-to-end dell'interfaccia (Playwright) e test visivi.

## Fase 3 — Trigger, OSC e API (L)

- [x] Trigger dei cue: tasti rapidi, MIDI, timecode, orario (con i giorni
      della settimana), messaggi OSC in ingresso, in una scheda Trigger.
- [ ] "Second trigger" e calendario con date.
- [x] `load` e `preview` sui cue.
- [ ] **OSC completo**: coprire i circa 1.100 metodi documentati da QLab,
      in ordine di uso reale, con test di conformità basati sulla
      documentazione pubblica.
- [ ] Porta OSC in testo semplice (53535), `/udpReplyPort`, indirizzi con
      `/workspace/{id o nome}`, `/alwaysReply`.
- [x] Password e permessi (view, edit, control) per OSC e web, con la
      schermata del codice e i comandi disattivati nell'interfaccia.
- [ ] **API pubblica documentata** (OpenAPI per HTTP, schema dei messaggi
      WebSocket) e piccoli SDK per JavaScript e Python.
- [ ] Superfici di controllo: Stream Deck e controller MIDI con
      apprendimento ("learn") dei comandi.
- [x] Annuncio del server sulla rete (mDNS: `_mazzalab._tcp` e `_osc._udp`),
      così i tablet lo trovano da soli; `--no-announce` lo spegne.

## Fase 4 — MIDI, timecode e dispositivi di rete (L)

- [x] MIDI in ingresso e in uscita (`midir`): note, CC, program change e
      SysEx in uscita; note, CC e program change in ingresso, come trigger
      dei cue e dei comandi del trasporto, con apprendimento dal controller.
- [ ] MSC: in uscita c'è; in ingresso il messaggio si legge e si traduce in
      azioni dello show (crate MIDI), ma non è ancora collegato al server.
- [ ] MIDI File cue sincronizzato con lo show.
- [x] Timecode MTC e LTC in uscita, MTC e LTC in ingresso, tutti i
      formati di frame (drop frame compreso); l'LTC si legge da un canale
      di un ingresso audio, col frame rate riconosciuto dal segnale.
      Mancano l'inseguimento con freewheel e i comportamenti di partenza
      e arresto (skip, lookback, start all...).
- [x] Network cue che inviano davvero: OSC su UDP e TCP, UDP e TCP semplici,
      HTTP (GET e POST, senza TLS), WebSocket, e il fade dei valori OSC
      (`0.0>1.0` nella durata del cue).
- [ ] **Schema aperto e versionato** per le descrizioni dei dispositivi
      (JSON Schema), con editor nell'interfaccia.
- [ ] Libreria iniziale di dispositivi (mixer, media server, PTZ,
      processori audio) in un repository pubblico aperto ai contributi.

## Fase 5 — Luci (XL)

- [x] Uscita Art-Net e sACN (E1.31) con priorità, per universo e con indirizzo
      del nodo, con sincronizzazione (ArtSync; universo di sync 63999 per sACN).
- [ ] Interfacce DMX USB (Enttec Open e Pro, DMXking) via `serialport`.
- [ ] RDM: scoperta e indirizzamento delle fixture dalla rete.
- [ ] Patch delle fixture con import di **GDTF** e della **Open Fixture
      Library** (formati aperti, dove QLab usa un formato suo).
- [x] Light cue con comandi testuali in stile console (`1 thru 10 @ 50`),
      fixture e gruppi per nome, parametri e colori. Mancano palette e
      tempi per parametro.
- [ ] Effetti (chase, onde, random) e modalità tracking opzionale.
- [ ] Merge HTP/LTP tra più cue attivi.
- [ ] Dashboard luci dal vivo e programmer con "registra nel cue".
- [ ] Visualizzatore 2D del palco (3D in seguito).

## Fase 6 — Video (XL)

### Motore
- [ ] Renderer `wgpu` (Vulkan, Metal, DirectX 12) a schermo intero su più
      monitor.
- [ ] Stage composti da superfici, con layer, opacità, metodi di fusione e
      geometria (posizione, scala, rotazione 3D, ancoraggio).
- [ ] Decodifica con FFmpeg (licenza LGPL, linking dinamico), HAP
      decodificato su GPU, decodifica hardware (NVDEC, VideoToolbox,
      D3D11VA, VAAPI).
- [ ] Sincronia audio/video frame-accurata sull'orologio audio.
- [x] Fade di opacità e geometria, lineari o a S, da dove il cue si trova.
- [ ] Effetti in WGSL (sfocatura, colore, keystone e simili).

### Cue
- [x] Video cue e Text cue su stage, con layer, opacità, fade di opacità,
      trim, velocità, loop e fermo sull'ultimo fotogramma.
- [x] Geometria: adattamento allo stage, posizione, scala, rotazione e
      ritaglio, in pixel dello stage (ogni stage ha una risoluzione), con
      un'anteprima nell'inspector in cui trascinare l'immagine.
- [x] Stile del testo: carattere, dimensione, colore, allineamento,
      grassetto, corsivo, interlinea, larghezza a capo, riquadro dietro.
- [ ] Testo ricco (stili diversi nello stesso cue), rotazione 3D, Camera
      cue.

### Proiezione
- [ ] Edge blending con gamma, warping (prospettiva, lineare, Bézier,
      mesh), maschere, griglie di allineamento.

### Condivisione video
- [ ] NDI in ingresso e in uscita, Syphon (macOS), Spout (Windows),
      PipeWire (Linux).

### Oltre QLab
- [ ] **Render node remoti**: macchine senza interfaccia che fanno solo
      uscita video e ricevono ordini dal server principale, in sincronia.
- [x] Anteprima delle uscite video nell'interfaccia web e sui tablet: il
      monitor dello stage nel pannello laterale.
- [x] **Stage nel browser**: qualsiasi finestra a `/?stage=Nome` è
      un'uscita video, anche su un altro computer della rete, in sincronia
      con l'orologio del server e con la decodifica hardware del browser.
      Resta da fare il renderer nativo (`wgpu`) per edge blending,
      warping e HAP.
- [x] **Stage sui monitor**: l'app desktop mette ogni stage a schermo
      intero sullo schermo che gli è assegnato (menu Video, Ctrl+Maiusc+D,
      o il pulsante sul monitor dello stage), con l'audio senza clic e le
      finestre che seguono rinomine, cambi di schermo e cambi di show.

## Fase 7 — Collaborazione e ridondanza (L)

- [ ] Più operatori sullo stesso show: utenti, permessi, chi sta
      modificando cosa, blocco per cue.
- [ ] Modalità show che rende i collaboratori solo spettatori.
- [ ] **Backup caldo**: la macchina di backup riceve lo stesso registro di
      input (Fase 0), calcola lo stesso stato grazie al motore
      deterministico e subentra in meno di un secondo. Confronto continuo
      dello stato tramite hash.
- [ ] Sincronizzazione dei media tra macchine principale, backup e render
      node.
- [ ] `mazzalab diff` e merge degli show per lavorare con git.

## Fase 8 — Script e plugin (L)

- [ ] Script cue in un linguaggio integrato e isolato (Lua con `mlua`
      oppure WASM con `wasmtime`), con limiti di tempo e permessi.
- [ ] API per i plugin: nuovi tipi di cue, nuovi backend di uscita, nuovi
      dispositivi, pannelli dell'interfaccia.
- [ ] Versionamento dell'API dei plugin e firma dei pacchetti.
- [ ] Catalogo di plugin e dispositivi.

## Fase 9 — Interoperabilità e documenti per la compagnia (M)

- [x] **Controllo pre-show**: un'unica verifica (`GET /api/preflight`, e il
      pulsante nella barra) che segnala media mancanti, target mancanti o
      sbagliati, patch e stage inesistenti, comandi luci illeggibili, gruppi
      vuoti, numeri e trigger doppi. Non controlla i dispositivi audio e MIDI.
- [ ] Stampa ed esportazione della lista cue (PDF, CSV) per direttore di
      scena e tecnici.
- [ ] Import di liste cue da CSV e fogli di calcolo.
- [ ] Import di show QLab: solo dopo una verifica legale
      (direttiva UE sul software, art. 6) e con un workspace di esempio.

## Fase 10 — Prodotto e qualità (continuo)

### Distribuzione
- [ ] Installer firmati: Windows (MSI/NSIS), macOS (DMG notarizzato),
      Linux (AppImage, deb, rpm, Flatpak).
- [ ] Aggiornamento automatico con canali stable/beta e ritorno alla
      versione precedente; mai durante uno show.
- [ ] Server senza schermo per Linux ARM (Raspberry Pi e simili).
- [ ] Show recenti nel menu File.
- [ ] Licenza e attivazione (modello di prezzo da decidere).

### Affidabilità
- [ ] Suite di test su hardware vero, eseguita prima di ogni rilascio.
- [ ] Segnalazione dei crash, solo con il consenso dell'utente.
- [ ] Misura in CI degli obiettivi della sezione "Cosa vuol dire meglio di
      QLab".

### Documentazione
- [x] Guida utente in italiano e in inglese (`docs/user-guide`): descrive
      quello che c'è oggi e va tenuta aggiornata con le funzioni nuove.
- [x] Show di esempio e tutorial: `examples/shows/theatre.mazzalab` (si riscrive
      con `mazzalab new --example theatre`) e `docs/user-guide/tutorial.md`.
- [ ] Unificare i fine riga (LF) per evitare gli avvisi LF/CRLF su Windows.

---

## Fatto in parte

Voci ancora aperte qui sopra di cui esiste già un pezzo:

- **Gestione dei media**: i file importati dalla UI finiscono in `media/`
  accanto allo show (stesso file = nessuna copia doppia). "Consolida" e il ricollegamento dei file mancanti per nome stanno nel
  pannello Media (un solo annulla ciascuno). Manca l'avviso se un media
  cambia su disco.
- **Registro dei backend di uscita**: l'audio si riconfigura senza fermare
  il server; manca un registro comune per luci, video e dispositivi.
- **Precaricamento**: i cue al playhead (e quelli nei gruppi al playhead)
  sono decodificati prima del GO. Manca la misura della latenza GO → suono.
- **Meter**: picco per uscita. Mancano RMS, LUFS e i meter per cue.
- **Forma d'onda**: disegno, inizio e fine trascinabili, cursore di
  riproduzione, fade disegnati, marcatori di slice (Alt+clic per tagliare,
  trascinabili) con le ripetizioni di ogni slice. Mancano zoom e scrub.
- **Cue list**: aggiungere, rinominare, eliminare. Manca la vista di più
  liste insieme.
- **Impostazioni dello show**: nome, doppio GO, panic, audio, patch di
  rete. Mancano i cue di apertura e chiusura.
- **Modalità show/edit**: c'è il blocco delle modifiche, senza password.
- **Network cue**: OSC su UDP e TCP, testo su UDP e TCP, HTTP, WebSocket e
  i fade dei valori OSC. L'HTTP è solo in chiaro (niente TLS).
- **Luci**: uscita Art-Net e sACN (unicast, broadcast o multicast), Light
  cue con comandi in stile console (`1 thru 10 @ 50`, `2/7 @ full`,
  `all @ out`) e tempo di fade, i look si sommano come su una console
  tracking, monitor dei canali accesi. Patch delle fixture con tipo e
  canali, gruppi anche annidati, chiamati per nome nei cue
  (`front wash @ 75`, `spot.pan @ 50`, `cyc @ #ff8000`, CMY compreso).
  Mancano priorità e sincronizzazione sACN, interfacce DMX USB, RDM,
  import GDTF/OFL, palette, effetti, merge HTP, dashboard e programmer.
- **MIDI**: in uscita note, CC, program change, SysEx e MSC verso le
  patch MIDI dello show; in ingresso note, CC e program change da tutti
  gli ingressi. Mancano SysEx e MSC in ingresso e il MIDI File cue.
- **Timecode**: il cue Timecode genera MTC verso una patch MIDI o LTC su
  un'uscita della scheda audio (verificato sulla scheda), in tutti i
  formati, drop frame compreso; l'MTC in ingresso si vede nella barra di
  stato e fa partire i cue col trigger timecode quando ci passa sopra in
  avanti (un salto non fa partire niente); lo stesso vale per l'LTC letto
  da un ingresso audio, con il frame rate riconosciuto dal segnale.
  Manca l'inseguimento (chase).
- **Trigger dei cue**: tasti rapidi e messaggi MIDI per cue, con
  apprendimento dal controller; GO, stop, panic e playhead comandabili via
  MIDI; trigger da timecode, da un indirizzo OSC qualsiasi (con `*` e
  `?`) e dall'orario, nei giorni scelti. Mancano il second trigger, il
  calendario con date e Stream Deck.
- **Slice e Devamp**: un cue audio si divide in slice, ognuna con le sue
  ripetizioni o in loop fino al devamp; il Devamp chiude il passaggio in
  corso e passa alla slice successiva senza buchi, al campione (verificato
  sulla scheda audio), o chiude il cue nell'ultima.
- **Video**: cue Video e Testo su stage nominati, a layer, con opacità,
  geometria (adattamento, posizione, scala, rotazione, ritaglio) e fade
  di entrambe, pausa, ripresa, carico a un punto e fine segnalata dallo
  stage; il testo ha carattere, colore, allineamento e riquadro. Ogni
  browser che apre `/?stage=Nome` è un proiettore, e il pannello laterale
  mostra lo stage dal vivo; l'app desktop apre gli stage a schermo intero
  sui monitor assegnati. L'audio dei video esce dal browser dello stage.
  Mancano il renderer nativo, edge blending e warping, NDI/Spout/Syphon e
  il Camera cue.
- **Avvisi**: file illeggibili, patch mancanti e dispositivi assenti
  arrivano all'operatore come notifiche; il controllo pre-show unico non guarda i dispositivi.

## Fatto

- [x] Scelta del dispositivo audio, frequenza e buffer, con ripiego
      sull'uscita predefinita se il dispositivo dello show manca.
- [x] Matrice di routing per cue, fade integrati, livello modificabile
      mentre il cue suona.
- [x] Upload dei media dalla UI e lettura automatica di durata e canali.
- [x] Comandi `batch` (più modifiche, un solo undo) e `set_numbers`.
- [x] Editing nella UI: selezione multipla, copia/incolla, duplica,
      raggruppa, rinumera, drag & drop, modifica in linea.
- [x] Finestre di apertura e salvataggio nel menu File (Nuovo, Apri,
      Salva, Salva con nome).
- [x] Avviso alla chiusura, e prima di Nuovo e Apri, se ci sono modifiche
      non salvate.
