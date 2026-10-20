# ADR 0001: Clean-room approach to QLab

- **Status:** accepted
- **Date:** 2026-10-20

## Context

CuLab aims to be a cross-platform alternative to QLab. Studying QLab is
useful, but QLab is proprietary and its EULA (§4.2) prohibits reverse
engineering, decompiling and disassembling the software.

## Decision

1. We learn about QLab only from its public documentation, its observable
   behaviour, and the resources and metadata it openly ships (bundle
   layout, `Info.plist`, AppleScript dictionary, credits, changelog, JSON
   device descriptions and UI layouts).
2. We never disassemble, decompile or trace QLab's executable code, and we
   never extract class or method metadata from it.
3. No QLab files, text, images or code are committed to this repository.
   Analysis notes are written in our own words
   ([qlab-analysis.md](../architecture/qlab-analysis.md)).
4. Interoperability targets *documented* interfaces: the OSC API is
   implemented from the public OSC dictionary. Importing QLab workspace
   files is out of scope until it has been reviewed legally.
5. CuLab's data model, file format, device description schema and UI are
   our own designs. Generic show-control concepts (cue, GO, pre-wait,
   auto-follow...) are industry vocabulary and are used freely.

## Consequences

- Some details of QLab's internals stay unknown. Where they matter, we
  measure them as a black box (e.g. OSC round-trip latency on a Mac).
- Contributors must not paste QLab material into issues or pull requests.
