# Tutorial: un primo atto con MazzaLab

*[Read in English](tutorial.md)*

Questo percorso passa per lo show di teatro che accompagna MazzaLab e poi ti fa
costruire da solo le parti principali. Richiede circa un'ora. La
[guida](README.it.md) spiega ogni parte per intero.

## 1. Apri l'esempio

L'esempio è `examples/shows/theatre.mazzalab`, con sei file sonori minuscoli in
`examples/shows/media/` (153 kB in tutto: toni e rumore, non musica). Avvia il
programma su di esso:

```
cargo run -p mazzalab-desktop -- examples/shows/theatre.mazzalab
```

(oppure il server senza schermo, poi apri `http://localhost:53080`). Per scriverne
una copia nuova altrove, coi suoi file sonori:

```
cargo run -p mazzalab-cli -- new il-mio-atto.mazzalab --example theatre
```

Premi il pulsante di controllo nella barra in alto (Controllo pre-show): deve dire
che non c'è niente da segnalare. Poi leggi la lista dall'alto. È un atto di 14 cue,
e una seconda lista, *Effects*, che è un cart.

## 2. Cosa c'è dentro

| N. | Cue | Cosa mostra |
| --- | --- | --- |
| 1 | House to half | un cue **Light**: `wash @ 50` e un blu sul ciclorama, in 5 s |
| 2 | Preshow music | un cue **Audio** in loop, con fade in |
| 3 | Welcome text | un cue **Text** sullo stage Main |
| 4 | House out | luci spente in 4 s |
| 5 | Scene 1 music | audio che **sfuma e ferma gli altri** quando parte |
| 6 | Scene 1 lights | un look fatto di fixture e un colore |
| 7 | Storm | un **Group** in modalità timeline: tuono, lampo, testo, con pre-wait |
| 8 | Waiting music | audio tagliato in **slice**, una delle quali in loop |
| 9 | Leave the loop | un cue **Devamp** che punta all'8 |
| 10 | Sound desk scene 2 | un cue **Network** (OSC), con auto-continue |
| 11 | Fade the scene music | un cue **Fade** che ferma il suo target |
| 12 | Beat | un **Wait** con auto-follow |
| 13 | House up | luci su |
| 14 | End of act 1 | un **Memo** |

Conducilo dalla tastiera: premi **Spazio** per il GO e guarda lo standby scendere
nella lista. Ferma tutto con **Esc** (panic). L'hotkey **T** fa partire il tuono
(cue 7.1) da qualsiasi schermo.

Le luci e il cue di rete mandano ad Art-Net (broadcast) e a `127.0.0.1:9000`; se
nessuno ascolta, non succede nulla. L'audio ha bisogno di un'uscita, che si sceglie
in Impostazioni, *Audio*.

## 3. Costruiscilo tu

Crea un nuovo show (barra in alto, Nuovo) e salvalo, così ha una cartella.

### L'impianto e lo stage

1. **Impostazioni, Fixture**: aggiungi *Front wash*, *Back wash* e *Spot* come
   dimmer sull'universo 1, indirizzi 1, 2 e 3, e *Cyc* al 10 coi parametri
   `red, green, blue`. Aggiungi un gruppo *Wash* con le prime due.
2. **Impostazioni, Luci**: aggiungi un'uscita per l'universo 1 (Art-Net).
3. **Impostazioni, Video**: aggiungi uno stage chiamato *Main*.
4. **Impostazioni, Rete**: aggiungi una patch *Sound desk*, host `127.0.0.1`, porta
   9000, OSC su UDP.

### Luci

1. Aggiungi un cue **Light** (toolbox). Nella sua scheda scrivi un comando per riga:
   `wash @ 50` e `cyc @ #101040`. Imposta la *durata* a 5 s: è il tempo del fade.
2. Chiamalo *House to half* e premi GO: il pannello DMX a destra mostra gli slot
   che si accendono.

### Suono, e un suono che parte sopra un altro

1. Trascina `preshow.wav` dal pannello Media sulla lista. Nella scheda **Tempi e
   loop** spunta *Loop infinito*; in **Livelli** imposta un fade in di 2 s.
2. Trascina `scene1.wav`. Nella scheda **Generale**, sotto *Gli altri, quando
   parte*, scegli *Sfuma e ferma*, *Tutte le liste*, tempo di sfumo 3 s. Ora il GO
   su questo cue fa sfumare da solo la musica del preshow. (*Abbassali (duck)*
   abbasserebbe invece gli altri, e li riporterebbe su quando questo cue finisce.)

### Una slice in loop, e il Devamp

1. Aggiungi `waiting.wav`. Nella scheda **Tempi e loop**, Alt-clic sulla forma
   d'onda a 1 s e a 2 s per ottenere tre slice. Dai alla seconda un numero di
   riproduzioni 0: gira finché non arriva un devamp.
2. Aggiungi un cue **Devamp** e scegli come target la musica d'attesa.
3. Fai partire la musica, lasciala entrare nel loop, poi premi GO sul Devamp: il
   giro in corso finisce e suona l'ultima slice.

### Un temporale: un group in timeline

1. Aggiungi un **Group**, modalità *Timeline (avvia tutti)*. Mettici dentro
   `thunder.wav` e due cue Light (trascinali sul group).
2. Dai al lampo un pre-wait di 0,5 s. Nella scheda del group i figli sono
   disegnati come barre: trascina una barra per cambiarne il pre-wait.
3. Aggiungi nel group un cue **Text**, `THE STORM`, sullo stage *Main*, con una
   dimensione più grande e un pre-wait di 1 s. Apri
   `http://localhost:53080/?stage=Main` in una finestra del browser per vedere lo
   stage.

### Fade, wait e auto-follow

1. Aggiungi un cue **Fade**, con target la musica della scena, livello −60 dB in
   2 s, e spunta *Ferma il target alla fine*.
2. Aggiungi un **Wait** di 2 s e imposta *Continua* su *Auto-follow*: il cue
   successivo parte quando questo finisce.

### Un cart per gli effetti

Premi il pulsante cart accanto alle schede delle liste, e trascina `bell.wav` e
`door.wav` sopra. Ognuno è un pulsante che puoi premere in qualsiasi ordine.

## 4. Fallo tuo

- Seleziona un cue che userai ancora e premi *Salva il cue selezionato come
  template*; ne crei di nuovi dal menu accanto alla toolbox.
- Dai un hotkey al tuono (scheda Trigger).
- Esegui il **Controllo pre-show** prima di ogni replica.
- Apri `/?view=show` su un tablet, e imposta dei codici in Impostazioni se la rete
  è condivisa.
- Premi Ctrl+S: una copia di sicurezza resta nella cartella `backups` accanto allo
  show.
