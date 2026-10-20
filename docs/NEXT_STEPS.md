# Prossimi passi

Elenco di lavoro, in ordine di priorità dentro ogni sezione. La visione
d'insieme è nella [roadmap](architecture/overview.md#roadmap).
Spunta le voci man mano (`- [x]`).

## 0. Subito

- [ ] Fare il push di `main` e controllare che la CI passi su Linux, Windows
      e macOS (è la prima esecuzione: possibili aggiustamenti ai pacchetti
      di sistema su Linux).
- [ ] Rinominare la cartella locale `CuLab` in `MazzaLab`, con tutte le
      sessioni chiuse.
- [ ] Decidere l'hardware di riferimento per i test (scheda audio,
      interfaccia DMX) da usare come banco prova.

## 1. App desktop

- [ ] Finestre di apertura e salvataggio (`tauri-plugin-dialog`): Nuovo,
      Apri, Salva con nome.
- [ ] Avviso alla chiusura se ci sono modifiche non salvate.
- [ ] Menu dell'applicazione e show recenti.
- [ ] Autosave e copie di backup a rotazione.
- [ ] Installer firmati per Windows (MSI/NSIS), macOS (DMG, notarizzazione)
      e Linux (AppImage/deb).
- [ ] Aggiornamento automatico.

## 2. Interfaccia operatore

- [ ] Selettore di file e browser dei media: un endpoint del server per
      sfogliare la cartella dello show e caricare file da un tablet.
- [ ] Trascinare i cue per riordinarli e spostarli nei gruppi
      (`move_cue` è già supportato dal server).
- [ ] Copia, incolla e duplica dei cue.
- [ ] Rinumerazione dei cue.
- [ ] Gestione delle cue list: aggiungere, rinominare ed eliminare dalla UI.
- [ ] Pannello impostazioni dello show: protezione dal doppio GO, durata del
      panic, nome dello show.
- [ ] Forma d'onda con i punti di inizio e fine trascinabili.
- [ ] Cue cart: una griglia di pulsanti per gli effetti a chiamata.
- [ ] Vista "solo show" per tablet: GO, standby e stop, senza editing.
- [ ] Test end-to-end dell'interfaccia (Playwright).

## 3. Audio

- [ ] Elenco e scelta del dispositivo di uscita, anche multicanale.
- [ ] Patch di uscita e matrice di routing (canali del file → uscite).
- [ ] Precaricamento (autoload) per partenze a latenza zero, precise al
      campione.
- [ ] Orologio audio che guida il motore al posto dell'orologio di sistema.
- [ ] Probe della durata anche quando un file viene cambiato dalla UI.
- [ ] Indicatori di livello (meter) per cue e uscite.
- [ ] Fade and stop others e duck others.
- [ ] Cambio di velocità con mantenimento dell'intonazione.
- [ ] Ingressi microfonici (Mic cue).

## 4. Server e OSC

- [ ] `load` e `preview` sui cue.
- [ ] Password e permessi (view, edit, control) per client OSC e web.
- [ ] Valori `/live` separati dallo show salvato.
- [ ] Porta OSC in testo semplice (53535) e `/udpReplyPort`.
- [ ] Trigger dei cue: tasti rapidi, MIDI, timecode, orario.

## 5. Luci

- [ ] Uscita Art-Net e sACN.
- [ ] Patch delle fixture e libreria delle definizioni.
- [ ] Light cue con dissolvenze e comandi testuali.
- [ ] Dashboard luci dal vivo.
- [ ] Interfacce DMX USB.

## 6. MIDI, timecode e dispositivi di rete

- [ ] MIDI e MSC in ingresso e in uscita (`midir`).
- [ ] Timecode MTC/LTC in ingresso e in uscita.
- [ ] Network cue che inviano davvero OSC, UDP e TCP.
- [ ] Formato aperto per le descrizioni dei dispositivi, con una libreria
      iniziale (mixer, media server, PTZ).

## 7. Video

- [ ] Renderer `wgpu`: stage, uscite, layer, opacità.
- [ ] Text cue e Camera cue.
- [ ] Edge blending, warping e maschere.
- [ ] NDI, Syphon (macOS), Spout (Windows).

## 8. Collaborazione e ridondanza

- [ ] Più operatori sullo stesso show con permessi.
- [ ] Macchina di backup "calda" che segue il motore principale e subentra.

## 9. Script e plugin

- [ ] Script cue con un linguaggio integrato e isolato (Lua o WASM).
- [ ] API per i plugin: nuovi tipi di cue e nuovi dispositivi.

## Qualità e documentazione

- [ ] Benchmark del mixer audio e test di carico (molte voci).
- [ ] Guida utente in italiano e in inglese (`docs/user-guide`).
- [ ] Unificare i fine riga (LF) per evitare gli avvisi LF/CRLF su Windows.
