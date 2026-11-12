import {
  ArrowBendDownRight,
  ArrowCounterClockwise,
  ArrowDown,
  ArrowLineDown,
  ArrowUUpLeft,
  ArrowUUpRight,
  CaretDoubleRight,
  CaretRight,
  CheckCircle,
  Code,
  Copy,
  Crosshair,
  DownloadSimple,
  FileAudio,
  FilmStrip,
  Flag,
  FloppyDisk,
  FolderSimple,
  FolderSimplePlus,
  GearSix,
  Globe,
  Hourglass,
  Lightbulb,
  ListNumbers,
  LockSimple,
  Microphone,
  Note,
  Pause,
  PencilSimple,
  PianoKeys,
  Play,
  Plus,
  Repeat,
  SpeakerHigh,
  Stop,
  TextT,
  Timer,
  Trash,
  VideoCamera,
  Warning,
  X,
  XCircle,
  type Icon as PhosphorIcon,
  type IconWeight,
} from "@phosphor-icons/react";
import type { CueType } from "../api/types";

/** A fade ramp, drawn on the icon set's grid and stroke so it sits with the rest. */
function FadeGlyph({ size }: { size: number }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 256 256"
      fill="none"
      stroke="currentColor"
      strokeWidth="16"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M32 200h40c64 0 48-144 112-144h40" />
    </svg>
  );
}

const CUE_ICONS: Record<Exclude<CueType, "fade">, PhosphorIcon> = {
  audio: SpeakerHigh,
  video: FilmStrip,
  text: TextT,
  mic: Microphone,
  camera: VideoCamera,
  light: Lightbulb,
  network: Globe,
  midi: PianoKeys,
  midi_file: FileAudio,
  timecode: Timer,
  group: FolderSimple,
  start: Play,
  stop: Stop,
  pause: Pause,
  load: DownloadSimple,
  reset: ArrowCounterClockwise,
  devamp: Repeat,
  goto: ArrowBendDownRight,
  arm: CheckCircle,
  disarm: XCircle,
  target: Crosshair,
  wait: Hourglass,
  memo: Note,
  script: Code,
};

export function CueIcon({ type, size = 16 }: { type: CueType; size?: number }) {
  if (type === "fade") return <FadeGlyph size={size} />;
  const Glyph = CUE_ICONS[type];
  return <Glyph className="icon" size={size} aria-hidden />;
}

const ICONS = {
  undo: [ArrowUUpLeft, "regular"],
  redo: [ArrowUUpRight, "regular"],
  save: [FloppyDisk, "regular"],
  settings: [GearSix, "regular"],
  warning: [Warning, "fill"],
  flag: [Flag, "fill"],
  close: [X, "regular"],
  copy: [Copy, "regular"],
  group: [FolderSimplePlus, "regular"],
  renumber: [ListNumbers, "regular"],
  lock: [LockSimple, "fill"],
  edit: [PencilSimple, "regular"],
  plus: [Plus, "regular"],
  trash: [Trash, "regular"],
  play: [Play, "fill"],
  pause: [Pause, "fill"],
  stop: [Stop, "fill"],
  reset: [ArrowCounterClockwise, "bold"],
  playhead: [CaretRight, "fill"],
  loaded: [CaretDoubleRight, "bold"],
  auto_continue: [ArrowDown, "bold"],
  auto_follow: [ArrowLineDown, "bold"],
} satisfies Record<string, [PhosphorIcon, IconWeight]>;

export type IconName = keyof typeof ICONS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  const [Glyph, weight] = ICONS[name];
  return <Glyph className="icon" size={size} weight={weight} aria-hidden />;
}
