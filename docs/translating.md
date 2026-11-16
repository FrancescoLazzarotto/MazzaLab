# Translating the interface

The web interface (and so the desktop app) takes its texts from one JSON file
per language in `apps/mazzalab-web/src/locales/`. English, `en.json`, is the
reference: it defines every key. A language may lack keys; what it lacks is
shown in English.

## Adding a language

1. Copy `en.json` to `<code>.json`, with the language's two-letter code
   (`fr.json`, `de.json`...), and translate the values. Keep the keys.
   Keep any `{placeholder}` as it is.
2. Register it in `src/locales/index.ts`: import the file and add a line to
   `LOCALES` with the language's own name, as it should read in the picker
   (`Français`, `Deutsch`).
3. Run `npx vitest run src/locales`. The test lists, per language, the keys
   it lacks and the keys English does not have. Extra keys always fail. Missing
   keys fail too, so that gaps are seen; to ship a language that is not
   complete yet, add its code to `ALLOW_PARTIAL` in
   `src/locales/locales.test.ts`.

The language appears in Settings, under This device, next to Automatic, which
follows the browser's languages (`fr-CH` finds `fr`). The choice is kept per
device.

## Writing the texts

- Use the words technicians use in that language. Terms such as Standby, GO,
  Pre-wait, Target, Cart and Panic are often best left as they are.
- Keep texts as short as the English ones: they sit in buttons and narrow
  columns.
- Cue type names (Audio, Fade, Memo...) are not in the files; they are the
  same in every language.

## Adding or changing a text in the code

Add the key to `en.json` first; the type of every key in the code comes from
it, so a misspelt key does not compile. Then add it to the other languages, or
leave it for the translators: until then it shows in English.
