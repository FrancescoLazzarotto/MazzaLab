import type { AudioCue, Cue, Matrix } from "./api/types";
import { flatten, matrixFor } from "./cues";

/** How many channels the sound a fade works on has: its file's, or the widest sound inside a group; stereo when nothing says. */
export function targetChannels(target: Cue | undefined): number {
  if (!target) return 2;
  const sounds = (target.type === "group" ? flatten([target]).map((row) => row.cue) : [target]).filter((cue): cue is AudioCue & { id: string } => cue.type === "audio");
  const widths = sounds.map((sound) => sound.media_channels ?? sound.matrix?.length ?? 2);
  return widths.length > 0 ? Math.max(...widths) : 2;
}

/** The matrix a fade starts from when it is switched on: the target's own routing, so that nothing jumps. */
export function startingMatrix(target: Cue | undefined, inputs: number, outputs: number): Matrix {
  return matrixFor(target?.type === "audio" ? target.matrix : undefined, inputs, outputs);
}

/**
 * The fade's matrix dressed as an audio cue, which is what the matrix editor
 * edits: sized by the target's channels.
 */
export function asAudioCue(fade: Cue, target: Cue | undefined): AudioCue & { id: string; type: "audio" } {
  return { id: fade.id, type: "audio", matrix: fade.type === "fade" ? fade.matrix : undefined, media_channels: targetChannels(target) } as AudioCue & { id: string; type: "audio" };
}

/** The range of the level of a fade: a target level, or a move up or down when relative. */
export function levelRange(relative: boolean): { min: number; max: number } {
  return relative ? { min: -60, max: 24 } : { min: -60, max: 12 };
}
