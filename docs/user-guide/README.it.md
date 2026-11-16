# Guida di MazzaLab

*[Read in English](README.md)*

Per il direttore di scena, l'operatore audio e luci e il tecnico che prepara
lo show. Descrive quello che il programma fa oggi; ciò che è ancora nel piano
di lavoro non è descritto. Per la parte tecnica: [API dei client](../protocols/client-api.md),
[dizionario OSC](../protocols/osc.md) e [piano](../PIANO.md).

Prima volta? Il [tutorial](tutorial.it.md) costruisce un primo atto passo per passo.

## Indice

1. [Installare e avviare](#1-installare-e-avviare)
2. [Il file dello show e la cartella media](#2-il-file-dello-show-e-la-cartella-media)
3. [Lo schermo](#3-lo-schermo)
4. [La cue list e i tipi di cue](#4-la-cue-list-e-i-tipi-di-cue)
5. [L'inspector](#5-linspector)
6. [GO, standby e trasporto](#6-go-standby-e-trasporto)
7. [Cart](#7-cart)
8. [Audio](#8-audio)
9. [Luci](#9-luci)
10. [Video e stage](#10-video-e-stage)
11. [Trigger: tasti, MIDI, OSC, timecode, orario](#11-trigger)
12. [Cue di rete e MIDI, timecode in uscita](#12-cue-di-rete-e-midi-timecode-in-uscita)
13. [Tablet: vista show, codici](#13-tablet)
14. [Salvare, aprire, copie e recupero](#14-salvare-aprire-copie-e-recupero)
15. [Scorciatoie da tastiera](#15-scorciatoie-da-tastiera)
16. [Aspetto e lingua](#16-aspetto-e-lingua)

## 1. Installare e avviare

MazzaLab funziona su Windows, macOS e Linux. Si avvia in due modi, e in
entrambi lo schermo dell'operatore è lo stesso.

- **L'app desktop** fa girare il server dello show dentro una finestra. Si usa
  sulla macchina che riproduce lo show.
- **Il server senza schermo** (headless) si apre da un browser di qualunque
  macchina della rete. Si usa per una macchina in rack o un media server.

Oggi entrambi si compilano dai sorgenti (il README ha i comandi: servono Rust e
Node.js, e lo schermo web si costruisce una volta con `npm run build`). Gli
installer si producono con Tauri; vedi il README.

```
cargo run -p mazzalab-desktop -- il-mio-show.mazzalab
cargo run -p mazzalab-server  -- il-mio-show.mazzalab --ui-dir apps/mazzalab-web/dist
```

Se il file dello show non esiste ancora, viene creato al primo salvataggio.

Il server ascolta sulla porta **53080** (browser, tablet, stage) e **53000**
(OSC). Apri `http://<indirizzo della macchina>:53080` da un browser della stessa
rete per avere lo schermo dell'operatore. Opzioni utili del server headless:

| Opzione | Fa |
| --- | --- |
| `--http-port`, `--osc-port` | cambia le porte |
| `--bind` | l'indirizzo su cui ascoltare (tutti, di default) |
| `--no-audio` | parte senza aprire un dispositivo audio |
| `--recover` | apre l'autosave se è più recente del file (vedi [sezione 14](#14-salvare-aprire-copie-e-recupero)) |
| `--passcode-local` | chiede i codici anche sulla macchina stessa |

Lo schermo si può anche installare come app dal browser (tiene i propri file, e
si ricarica anche se la rete cade per un attimo; lo show invece ha bisogno del
server).

## 2. Il file dello show e la cartella media

Uno show è un solo file con estensione `.mazzalab`. È testo (JSON), quindi si
può tenere sotto controllo di versione e confrontare. Contiene le cue list, le
cue e le impostazioni dello show: dispositivo audio, patch di rete e MIDI,
uscite DMX, fixture, stage, codici.

I media (file audio e video) stanno in una cartella chiamata `media` **accanto al
file dello show**. I cue nominano i file relativi allo show
(`media/Pioggia.wav`), quindi la cartella dello show si copia su un'altra
macchina così com'è.

Per portare dentro dei file:

- trascina file audio sulla cue list (ognuno diventa un cue Audio);
- nel pannello **Media** (a destra) premi il pulsante per aggiungere e scegli i
  file. Funziona anche da tablet, e si possono scegliere più file insieme;
- copiali tu nella cartella `media`.

Il pannello Media elenca il contenuto della cartella, con la dimensione e
quanti cue usano ogni file. **Non usati** nel filtro mostra i file che nessun
cue riproduce. Trascina un file dal pannello sulla cue list per creare un cue
Audio o Video.

Uno show non ancora salvato non ha una cartella, quindi non si possono
aggiungere media: salvalo prima.

## 3. Lo schermo

- **Barra in alto**: nome dello show, *Modificato* se ci sono modifiche non
  salvate, il selettore di modalità (**Modifica** / **Show**), annulla e
  ripeti, apri, nuovo, salva con nome, salva, impostazioni.
- **A sinistra**: il trasporto (GO, standby, pulsanti), la toolbox con i tipi
  di cue, le schede delle cue list, la cue list e sotto l'inspector.
- **A destra**: uscita audio con meter e livello master, luci, il monitor degli
  stage (se lo show ha cue Video o Text), i cue attivi e i media.
- **Riga in basso**: numero di cue, timecode in ingresso, il dispositivo audio,
  la connessione e, se lo show chiede un codice, cosa permette il tuo.

La **modalità Show** blocca le modifiche: nessun tasto premuto per sbaglio può
cambiare lo show. Usala per la recita. La scelta vale per ogni dispositivo.

Il pulsante a due colonne, a destra delle schede, mostra due cue list
affiancate. Clicca in un pannello per lavorarci; le schede cambiano allora la
lista di quel pannello. GO e inspector seguono l'ultimo pannello in cui hai
lavorato. L'affiancamento vale per dispositivo.

## 4. La cue list e i tipi di cue

Uno show ha una o più cue list. Ogni riga è un cue: numero, nome, target (il
file, il messaggio o il cue a cui punta), pre-wait, durata dell'azione,
post-wait, un segno per auto-continue o auto-follow, e un flag o un'avvertenza.
Un numero colorato indica un'etichetta di colore. Il segno di avvertenza dice
che il cue non può ancora fare nulla (manca il file, il target, il messaggio...).

Aggiungi i cue dalla toolbox sopra la lista; il nuovo va dopo quello
selezionato. Doppio clic su un numero o un nome per modificarlo. Trascina le
righe per spostarle; trascina un cue su un Group per metterlo dentro. Copia,
taglia, incolla e duplica funzionano come al solito (vedi la
[sezione 15](#15-scorciatoie-da-tastiera)). **Group** raggruppa i cue
selezionate. **Rinumera** numera la selezione da un primo numero con un passo.

**Ricerca**: il campo sopra la lista cerca nel numero, nel nome e nelle note;
i menu restringono per tipo di cue e colore e il pulsante flag mostra solo le
cue con il flag. **Ctrl+F** porta al campo, **Esc** lo svuota. Il filtro nasconde
solo le righe: GO e standby seguono la lista vera.

**Cue list**: le schede sopra la lista. Il più ne aggiunge una, il pulsante cart
aggiunge un cart ([sezione 7](#7-cart)), doppio clic su una scheda per
rinominarla, il cestino elimina quella aperta. I pulsanti a destra esportano la
lista in CSV, ne importano una, e stampano il foglio dei cue (la finestra di
stampa del browser può salvarlo in PDF). L'importazione legge un CSV dai nomi
delle colonne, in inglese o in italiano; le colonne che non conosce sono
ignorate e il tipo di default è Memo.

### Tipi di cue

| Tipo | Fa |
| --- | --- |
| **Audio** | riproduce un file sonoro: tagli, loop, slice, fade, livello, instradamento |
| **Video** | riproduce un file video su uno stage |
| **Text** | mostra un testo su uno stage |
| **Light** | imposta i livelli DMX con comandi in stile console, con un fade |
| **Fade** | porta al livello voluto (o opacità, o posizione) un altro cue, nel tempo |
| **Network** | manda messaggi OSC o semplici su UDP o TCP |
| **MIDI** | manda un messaggio MIDI: note, controller, program change, SysEx, MIDI Show Control |
| **Timecode** | manda MTC o LTC |
| **Group** | contiene altri cue e li fa partire in uno di cinque modi |
| **Start, Stop, Pause, Load, Reset, Devamp, Go To, Arm, Disarm** | agiscono sul cue indicato come target |
| **Target** | sta al posto di un altro cue, dal suo numero |
| **Wait** | non fa nulla per la sua durata (utile con l'auto-follow) |
| **Memo** | una nota nella lista; non fa nulla |

I cue Mic, Camera, MIDI File e Script si possono aggiungere ma per ora non
fanno nulla.

I **Group** fanno partire i figli in uno di cinque modi: *Timeline (avvia
tutti)* avvia tutti i figli insieme, ognuno ritardato dal proprio pre-wait;
*Avvia il primo*, *Avvia il primo ed entra*, *Avvia a caso* e *Playlist* fanno
quello che dicono.

**Tempi.** Ogni cue ha un *pre-wait* (un ritardo prima dell'azione) e un
*post-wait*. *Continua* sceglie cosa succede dopo: **Non continuare** aspetta il
GO successivo; **Auto-continue** avvia il cue seguente quando è passato il
post-wait, contato da quando parte l'azione di questo cue; **Auto-follow** avvia
il seguente appena finisce l'azione di questo. Un cue disarmato viene saltato;
*Armata* è nella scheda Generale.

## 5. L'inspector

Seleziona un cue e l'inspector mostra le sue schede. Più cue selezionati si
modificano insieme per i campi che hanno in comune.

- **Generale** (tutti i cue): numero, colore, nome, target (per i cue che
  puntano a un'altra), pre-wait, post-wait, modo di continuazione, armata, flag
  e note.
- **Tempi e loop** (Audio): il file e la sua forma d'onda; inizio e fine; loop e
  slice. Vedi la [sezione 8](#8-audio).
- **Livelli** (Audio): livello, fade in e out, matrice di instradamento, patch
  di uscita.
- **La scheda propria del cue** (col nome del tipo: Wait, Fade, Network, MIDI,
  Light, Timecode, Group...): cosa fa il cue.
- **Geometria** (Video, Text): dove sta l'immagine o il testo sul suo stage.
- **Trigger** (tutti i cue): cosa lo avvia dall'esterno. Vedi la
  [sezione 11](#11-trigger).

In un Group in modalità timeline la scheda disegna anche i figli come barre su
una scala dei tempi: trascina una barra per cambiare il pre-wait di quel figlio
(da tastiera, le frecce sinistra e destra spostano una barra selezionata di un
decimo di secondo, di un secondo con Shift).

Le modifiche valgono subito e si possono annullare.

## 6. GO, standby e trasporto

Il cue in **standby** è quello che il GO avvierà. Compare accanto al pulsante GO
con numero, nome e note. Dopo che un cue parte, lo standby passa al cue
successiva della lista.

- **GO** (Spazio) avvia il cue in standby. La **protezione doppio GO** nelle
  impostazioni ignora un secondo GO che arriva troppo presto dopo il primo.
- Selezionare un cue lo mette in standby; si disattiva in Impostazioni,
  *Questo dispositivo*, su un secondo schermo che non deve spostarlo. **Invio**
  mette in standby il cue selezionato; lo fa anche il doppio clic sul margine
  sinistro di una riga.
- **V** avvia il cue selezionato senza spostare lo standby.
- **Stop**, **Pausa** e **Riprendi** agiscono su tutti i cue in corso. **Reset**
  ferma tutto e torna all'inizio della lista.
- **Panic** (Esc) sfuma e ferma tutto, nel *tempo di sfumo del panic* delle
  impostazioni.
- **Cue attive** (pannello a destra) elenca quello che sta andando, con
  avanzamento e tempo residuo, e pausa, ripresa e stop per ognuna.
- **Load**: un clic sulla forma d'onda di un cue audio lo carica a quel punto;
  la partenza successiva riprende da lì.

## 7. Cart

Un **cart** è una cue list mostrata come griglia di pulsanti, per effetti
sonori e altri cue chiamati senza un ordine fisso. Se ne aggiunge uno col
pulsante cart accanto alle schede. Un clic su un pulsante avvia il suo cue (e lo
ferma se è in corso). I cue entrano in un cart come in una lista: trascinando o
rilasciando file, o aggiungendoli dalla toolbox.

## 8. Audio

**Dispositivo.** Impostazioni, *Audio*: scegli il dispositivo di uscita, la
frequenza di campionamento e la dimensione del buffer (buffer piccoli danno meno
latenza e chiedono una macchina più veloce), e il livello master. I driver ASIO
su Windows sono un'opzione della compilazione. Se il dispositivo scelto manca,
lo show suona sull'uscita predefinita e la riga in basso lo dice. Si può
scegliere un secondo dispositivo per il **preascolto**: nell'inspector, il
pulsante delle cuffie riproduce il cue lì, una volta, con tagli e fade, fuori
dallo show.

**File.** Si riproducono WAV, AIFF, FLAC, MP3, AAC, ALAC e Vorbis. I cue in
standby sul playhead vengono decodificate prima del GO.

**Tempi e loop.** La forma d'onda mostra il file con i fade integrati. Trascina
le due bandierine in alto per impostare inizio e fine. Sotto il righello, i
quadratini sulla linea del fade impostano le durate di fade in e fade out. Lo
zoom si fa con la rotella o con un pizzico, si scorre con Shift e la rotella (o
con la barra che compare quando è ingrandita), o coi pulsanti di zoom. Un clic
nella parte che suona carica il cue a quel punto. Loop: un numero di
riproduzioni, o loop infinito.

**Slice.** Alt-clic sulla forma d'onda per tagliare una slice. Ogni slice ha il
proprio numero di riproduzioni (0 la ripete finché non arriva un devamp); i
marcatori si trascinano. Un cue **Devamp** che punta al cue lascia finire il
giro in corso e passa alla slice successiva senza stacco; nell'ultima slice
chiude il cue.

**Livelli e instradamento.** Il livello è in dB. La *matrice di instradamento*
imposta, per ogni canale del file, il livello verso ogni uscita del
dispositivo, in dB, o silenzio. Senza matrice un file mono va alle uscite 1 e 2 e
ogni altro file va canale per canale. Un cue **Fade** che punta a un cue Audio
ne porta il livello a un valore, in un tempo, con curva lineare o a S, e può
fermare il target alla fine.

## 9. Luci

MazzaLab comanda le luci in rete con **Art-Net** o **sACN**.

**Uscite DMX** (Impostazioni, *Luci*): una riga per universo. Scegli il
protocollo, l'indirizzo del nodo (vuoto: Art-Net in broadcast e per sACN il
gruppo multicast) e il numero di universo sul cavo.

**Fixture** (Impostazioni, *Fixture*): dai un nome a ogni fixture, indica
universo e primo indirizzo, e di' cosa fanno i suoi canali in ordine
(intensity, red, green, blue...). I **gruppi** riuniscono fixture o altri gruppi
sotto un nome.

**Cue Light.** Un comando per riga, `selezione @ valore`:

```
1 thru 10 @ 50
front wash @ full
front wash + back wash @ #ff8000
2/7 @ out
spot.pan @ 25
all @ out
```

Una selezione unisce i termini con `+`: un indirizzo (da 1 a 512, `2/7` per
l'universo 2), un intervallo (`1 thru 10`), `all` per un universo, o il nome di
una fixture o di un gruppo. `nome.parametro` imposta un solo parametro. Il
valore è una percentuale, `full`, `out`, o un colore `#rrggbb`. La *durata* della
cue è il tempo del fade. Un cue cambia solo gli slot che nomina, quindi i look
si sommano; fermarla o premere Panic lascia le luci dove sono. Per un blackout
usa `all @ out`. Il pannello DMX a destra mostra gli slot accesi, coi nomi delle
fixture.

## 10. Video e stage

Uno **stage** è uno schermo che mostra i cue Video e Text. Qualunque browser
può essere uno stage: apri `http://<indirizzo della macchina>:53080/?stage=Main`
sul computer del proiettore (aggiungi `&monitor` per una copia muta).
Impostazioni, *Video* elenca gli stage per nome e dimensione (1920 per 1080 di
default); se non ce ne sono, ce n'è uno che si chiama Main.

Nell'app desktop uno stage può anche andare **a schermo intero su un display**:
scegli il display di ogni stage in Impostazioni, *Video*, poi usa *Video, Mostra
gli stage sugli schermi* (Ctrl+Maiusc+D). Il monitor degli stage nel pannello a
destra mostra cosa mostrano gli stage.

Un cue **Video** riproduce un file su uno stage, su un layer, con opacità,
inizio e fine, velocità e loop. Un cue **Text** mostra un testo con font,
dimensione, colore, allineamento e un riquadro di sfondo facoltativo. La scheda
**Geometria** colloca un cue sul suo stage: modo di adattamento, posizione,
scala, rotazione e ritaglio, con un'anteprima. Un cue **Fade** che punta a un
cue Video o Text può sfumarne l'opacità e portarlo a una nuova geometria.

## 11. Trigger

La scheda **Trigger** di un cue imposta cosa lo avvia dall'esterno, oltre al
GO. Un cue ne può avere più d'uno.

- **Hotkey**: un tasto o una combinazione (`Q`, `F5`, `Ctrl+Shift+1`) che avvia
  il cue da qualsiasi schermo dell'operatore. I tasti che lo schermo usa per sé
  non si possono assegnare.
- **MIDI**: una nota, un controller o un program change in arrivo su un
  qualsiasi ingresso MIDI. Premi *Impara* e muovi il controllo per riempirlo. Una
  nota on con velocity 0 vale come note off. In Impostazioni, *MIDI*, lo stesso
  apprendimento lega GO, Ferma tutto, Panic e i pulsanti del playhead ai
  controlli.
- **OSC**: un indirizzo OSC; un messaggio a quell'indirizzo sulla porta OSC dello
  show (UDP o TCP) avvia il cue. `*` e `?` sostituiscono qualsiasi carattere,
  quindi `/scena/*/go` risponde a `/scena/12/go`.
- **Timecode**: avvia il cue quando il timecode in ingresso (MTC su un qualsiasi
  ingresso MIDI, o LTC su un ingresso audio) passa da quel frame. Un salto o un
  locate non avvia nulla, così lo show si può riavvolgere sopra i suoi trigger.
  L'LTC si legge dall'ingresso e dal canale scelti in Impostazioni, *Audio*; il
  frame rate viene riconosciuto dal segnale.
- **Orario**: un orario (`19:30`, o con i secondi) e i giorni della settimana;
  nessuno vale ogni giorno. Scatta quando l'orologio della macchina passa
  dall'orario; una macchina in sospensione in quel momento non avvia nulla.

**Controllo remoto via OSC.** Il server risponde sulla porta 53000 agli stessi
indirizzi del dizionario OSC pubblico di QLab 5 (`/go`, `/cue/1/start`,
`/cue/selected/name`...), quindi le superfici di controllo pensate per QLab
funzionano. L'elenco di cosa è supportato è nel [documento OSC](../protocols/osc.md).

## 12. Cue di rete e MIDI, timecode in uscita

**Patch di rete** (Impostazioni, *Rete*): nome, host, porta e protocollo: OSC su
UDP, OSC su TCP (framing SLIP), UDP semplice o TCP semplice. Una **cue Network**
indica una patch (o usa la prima) e contiene un messaggio per riga:
`/indirizzo arg arg` per OSC (i numeri interi sono interi, quelli con la
virgola, cioè col punto, sono float, `true` e `false` booleani, il resto una
stringa), oppure il testo stesso per i protocolli semplici (con `\n`, `\r`, `\t`,
`\xHH`).

**Patch MIDI** (Impostazioni, *MIDI*): un nome e una delle porte di uscita della
macchina del server. Una **cue MIDI** manda una nota on o off, un controller, un
program change, byte SysEx, o MIDI Show Control (device, formato del comando,
comando, numero e lista del cue).

Le **cue Timecode** mandano timecode da un valore iniziale per una durata (o
finché non le fermi) a un frame rate (24, 25, 30 e varianti drop frame). L'**MTC**
va a una patch MIDI; l'**LTC** esce da un'uscita numerata del dispositivo audio.
La pausa lo ferma; riprendendo, riparte.

## 13. Tablet

Qualunque tablet o telefono in rete è un telecomando: apri
`http://<indirizzo della macchina>:53080`. Non serve installare nulla.

**La vista show.** `http://<indirizzo della macchina>:53080/?view=show` è uno
schermo solo per condurre lo show: il cue in standby con le sue note, un GO
molto grande, standby precedente e successivo, cosa sta andando con il suo
avanzamento, Stop e Panic. Non si modifica nulla.

**Codici.** Uno show senza codici è aperto a chiunque sia in rete. In
Impostazioni, *Codici*, aggiungi dei codici e spunta cosa permette ciascuno:

- **Vista**: vedere lo show e il suo stato;
- **Controllo**: condurlo (GO, stop, pausa, playhead, panic);
- **Modifica**: cambiarlo, salvare, aprire e creare show (Controllo e Modifica
  comprendono la Vista).

Appena esiste il primo codice, ogni altra macchina deve inserirne uno per
entrare; la macchina che fa girare il server non lo chiede mai. Se nessun codice
può modificare, solo quella macchina può cambiare lo show. Un dispositivo che ha
solo Controllo resta in modalità Show e non può cambiare lo show; uno che ha solo
Vista non può nemmeno condurlo. La riga in basso mostra cosa permette il
dispositivo e permette di cambiare codice. Un link può portare il codice:
`...?view=show&passcode=1234`.

I media si possono aggiungere da un tablet nel pannello Media (se il suo codice
può modificare).

## 14. Salvare, aprire, copie e recupero

- **Salva** (Ctrl+S) scrive il file dello show. Prima ne viene tenuta una copia
  com'era: accanto al file dello show, la cartella `backups` contiene
  `NOME.1.mazzalab` … `NOME.10.mazzalab`, le versioni sostituite dagli ultimi dieci
  salvataggi, la 1 è la più recente.
- **Autosave**: pochi secondi dopo ogni modifica una copia non salvata viene
  scritta in `backups/NOME.autosave.mazzalab` e tolta al salvataggio. Se il
  programma si ferma prima che tu salvi, riaprendo lo show viene proposta quella
  copia: l'app desktop la apre e segna lo show come *recuperato*; il server
  headless lo fa con `--recover`. Salva per farla diventare lo show.
- **Apri**, **Nuovo** e **Salva con nome** sono nella barra in alto (in modifica)
  e, nell'app desktop, nel menu File (Ctrl+O, Ctrl+N, Ctrl+Maiusc+S). Il selettore
  sfoglia le cartelle della macchina che fa girare il server. Se lo show aperto ha
  modifiche non salvate ti viene chiesto se salvarle, scartarle o annullare.
- **Annulla** e **ripeti** valgono per ogni modifica.

## 15. Scorciatoie da tastiera

| Tasti | Fa |
| --- | --- |
| Spazio | GO |
| Esc | Panic |
| ↑ ↓ | seleziona il cue precedente o successivo |
| Maiusc + ↑ ↓ | estende la selezione |
| Invio | mette in standby il cue selezionato |
| V | avvia il cue selezionato |
| Ctrl+A | seleziona tutto |
| Ctrl+C, X, V | copia, taglia, incolla |
| Ctrl+D | duplica |
| Ctrl+G | raggruppa |
| Canc | elimina |
| Ctrl+Z, Ctrl+Maiusc+Z | annulla, ripeti |
| Ctrl+S | salva |
| Ctrl+F | cerca nella cue list |

(Su macOS leggi Cmd al posto di Ctrl.) I tasti del trasporto si cambiano per ogni
dispositivo: Impostazioni, *Generale*, *Questo dispositivo*, *Tasti del
trasporto*. Oltre a GO e Panic, si possono dare tasti a Ferma tutto, Pausa,
Riprendi e standby successivo e precedente; un tasto già usato da un'altra
azione, da un cue o dallo schermo viene rifiutato, e *Ripristina i predefiniti*
rimette gli originali. L'hotkey di un cue (scheda Trigger) ha la precedenza.

## 16. Aspetto e lingua

In Impostazioni, *Generale*, *Questo dispositivo*: **Lingua** (Automatica segue il
browser, English, Italiano) e **Tema**: Scuro, Chiaro, *Buio di sala* (luminosità
minima, rossi cupi, per lavorare al buio dietro le quinte) e Alto contrasto. Si
ricordano per ogni dispositivo, quindi il tablet del direttore di scena e lo
schermo in regia possono essere diversi.
