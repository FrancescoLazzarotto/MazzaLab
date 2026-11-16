import { describe, expect, it } from "vitest";
import { PROTOCOLS, fadingArguments, isOsc, messageExample, patchPlace, usesUrl } from "./networkPatch";

describe("network patches", () => {
  it("knows which protocols take a URL", () => {
    expect(PROTOCOLS).toHaveLength(7);
    for (const protocol of ["http_get", "http_post", "websocket"] as const) expect(usesUrl(protocol)).toBe(true);
    for (const protocol of ["osc_udp", "osc_tcp", "plain_udp", "plain_tcp"] as const) expect(usesUrl(protocol)).toBe(false);
    expect(isOsc("osc_tcp")).toBe(true);
    expect(isOsc("http_get")).toBe(false);
    expect(isOsc(undefined)).toBe(true);
  });

  it("writes where a patch goes", () => {
    expect(patchPlace({ name: "a", host: "10.0.0.2", port: 9000, protocol: "osc_udp" })).toBe("10.0.0.2:9000");
    expect(patchPlace({ name: "a", host: "http://h/api", port: 0, protocol: "http_get" })).toBe("http://h/api");
  });

  it("gives an example message for each kind", () => {
    expect(messageExample("http_get")).toContain("/scene");
    expect(messageExample("websocket")).toContain("{");
    expect(messageExample("osc_udp")).toContain(">");
  });

  it("counts the arguments that fade", () => {
    expect(fadingArguments("/fader/1 0.0>1.0\n/x 5 0>100")).toBe(2);
    expect(fadingArguments("/a 1 2")).toBe(0);
    expect(fadingArguments('/a "1>2" x>y')).toBe(0);
  });
});
