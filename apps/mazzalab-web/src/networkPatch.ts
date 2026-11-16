import type { NetworkPatch, NetworkProtocol } from "./api/types";

export const PROTOCOLS: NetworkProtocol[] = ["osc_udp", "osc_tcp", "plain_udp", "plain_tcp", "http_get", "http_post", "websocket"];

/** Whether the patch's host is a URL, with no separate port. */
export function usesUrl(protocol: NetworkProtocol | undefined): boolean {
  return protocol === "http_get" || protocol === "http_post" || protocol === "websocket";
}

export function isOsc(protocol: NetworkProtocol | undefined): boolean {
  return protocol === undefined || protocol === "osc_udp" || protocol === "osc_tcp";
}

/** Where a patch sends to, as the cue's patch menu shows it. */
export function patchPlace(patch: NetworkPatch): string {
  return usesUrl(patch.protocol) ? patch.host : `${patch.host}:${patch.port}`;
}

/** An example of what a cue's message looks like for this protocol. */
export function messageExample(protocol: NetworkProtocol | undefined): string {
  switch (protocol) {
    case "http_get":
      return "/scene/2?fade=1";
    case "http_post":
      return '/scene {"id": 2}';
    case "websocket":
      return '{"cmd": "go"}';
    case "plain_udp":
    case "plain_tcp":
      return "PLAY 1\r\n";
    default:
      return "/cue/12/start\n/eos/sub/3 0.75\n/fader/1 0.0>1.0";
  }
}

/** The `from>to` arguments of an OSC message: how many lines move between two numbers. */
export function fadingArguments(message: string): number {
  return message.split(/\s+/).filter((word) => /^-?\d+(\.\d+)?>-?\d+(\.\d+)?$/.test(word)).length;
}
