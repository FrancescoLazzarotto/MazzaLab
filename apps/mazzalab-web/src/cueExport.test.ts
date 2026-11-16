import { describe, expect, it } from "vitest";
import type { Cue } from "./api/types";
import { buildCsv, csvField, parseCsv, parseCueCsv } from "./cueExport";

const cue = (extra: Record<string, unknown>): Cue => ({ id: "x", type: "memo", ...extra }) as Cue;

describe("csv building", () => {
  it("quotes what needs it", () => {
    expect(csvField("plain")).toBe("plain");
    expect(csvField("a,b")).toBe('"a,b"');
    expect(csvField('say "hi"')).toBe('"say ""hi"""');
    expect(csvField("two\nlines")).toBe('"two\nlines"');
    expect(csvField("a;b", ";")).toBe('"a;b"');
    expect(csvField("a;b")).toBe("a;b");
  });

  it("writes a header and one line per cue, groups opened", () => {
    const cues = [
      cue({ id: "a", number: "1", name: "Open, wide", notes: 'He said "go"', pre_wait: 1.5, color: "red", flagged: true }),
      cue({ id: "g", type: "group", number: "2", children: [cue({ id: "c", number: "2.1", type: "wait", duration: 3, continue_mode: "auto_follow" })] }),
    ];
    const lines = buildCsv(cues, cues).split("\r\n");
    expect(lines[0]).toBe("Number,Name,Type,Target,Pre-wait,Action,Post-wait,Continue,Notes,Colour,Flagged");
    expect(lines[1]).toBe('1,"Open, wide",Memo,,1.5,,,do_not_continue,"He said ""go""",red,yes');
    expect(lines[3]).toBe("2.1,Wait,Wait,,,3,,auto_follow,,,");
    expect(lines).toHaveLength(5);
  });
});

describe("csv parsing", () => {
  it("reads quotes, doubled quotes and line breaks inside fields", () => {
    expect(parseCsv('a,"b,1","say ""x""","l1\nl2"\r\nc,d,e,f\n')).toEqual([
      ["a", "b,1", 'say "x"', "l1\nl2"],
      ["c", "d", "e", "f"],
    ]);
  });

  it("finds a semicolon or tab delimiter and skips a byte order mark", () => {
    expect(parseCsv("﻿a;b\n1;2")).toEqual([["a", "b"], ["1", "2"]]);
    expect(parseCsv("a\tb\n1\t2")).toEqual([["a", "b"], ["1", "2"]]);
  });

  it("keeps empty fields", () => {
    expect(parseCsv("a,,c\n,,")).toEqual([["a", "", "c"], ["", "", ""]]);
  });
});

describe("cue import", () => {
  it("matches headers in English or Italian, in any case", () => {
    const { cues, read, ignored } = parseCueCsv("NUMERO;Nome;Tipo;Durata;Colore;Flagged;Extra\n5;Buio;wait;2,5;Rosso;sì;zzz");
    expect(read).toEqual(["NUMERO", "Nome", "Tipo", "Durata", "Colore", "Flagged"]);
    expect(ignored).toEqual(["Extra"]);
    expect(cues).toHaveLength(1);
    expect(cues[0]).toMatchObject({ type: "wait", number: "5", name: "Buio", duration: 2.5, color: "red", flagged: true });
  });

  it("defaults to a memo and ignores unknown columns and blank lines", () => {
    const { cues } = parseCueCsv("name,mystery,type\nHello,1,\n,,\nThere,2,Go To\nOdd,3,nonsense");
    expect(cues.map((c) => [c.type, c.name])).toEqual([
      ["memo", "Hello"],
      ["goto", "There"],
      ["memo", "Odd"],
    ]);
  });

  it("round-trips what it exports", () => {
    const source = [cue({ id: "a", number: "7", name: "Rain, heavy", type: "wait", duration: 4, post_wait: 2, notes: 'x\n"y"', continue_mode: "auto_continue", pre_wait: 0.5 })];
    const [back] = parseCueCsv(buildCsv(source, source)).cues;
    expect(back).toMatchObject({ number: "7", name: "Rain, heavy", type: "wait", duration: 4, post_wait: 2, notes: 'x\n"y"', continue_mode: "auto_continue", pre_wait: 0.5 });
  });

  it("has nothing to add from an empty file", () => {
    expect(parseCueCsv("").cues).toEqual([]);
  });
});
