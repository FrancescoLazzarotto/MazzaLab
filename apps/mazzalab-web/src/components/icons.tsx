import {
  ArrowBendDownRight,
  ArrowCounterClockwise,
  ArrowSquareOut,
  ArrowDown,
  ArrowUp,
  ArrowsOutLineHorizontal,
  ArrowLineDown,
  ArrowUUpLeft,
  ArrowUUpRight,
  ArrowsCounterClockwise,
  CaretDoubleRight,
  CaretRight,
  BookmarkSimple,
  CheckCircle,
  Columns,
  Code,
  Copy,
  Crosshair,
  DownloadSimple,
  FileAudio,
  FilmStrip,
  Flag,
  FilePlus,
  FileText,
  FloppyDisk,
  FloppyDiskBack,
  FolderOpen,
  FolderSimple,
  FolderSimplePlus,
  GearSix,
  Globe,
  Headphones,
  Hourglass,
  Lightbulb,
  MagnifyingGlass,
  MagnifyingGlassMinus,
  MagnifyingGlassPlus,
  ListChecks,
  ListNumbers,
  LockSimple,
  Microphone,
  Note,
  Pause,
  PencilSimple,
  PianoKeys,
  Play,
  Plus,
  Printer,
  ProjectorScreen,
  Repeat,
  SkipBack,
  SkipForward,
  SpeakerHigh,
  SpeakerLow,
  SpeakerSlash,
  SquaresFour,
  Stack,
  Stop,
  TextAlignCenter,
  TextAlignLeft,
  TextAlignRight,
  TextT,
  Timer,
  Trash,
  UploadSimple,
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
  external: [ArrowSquareOut, "regular"],
  screens: [ProjectorScreen, "regular"],
  settings: [GearSix, "regular"],
  warning: [Warning, "fill"],
  flag: [Flag, "fill"],
  close: [X, "regular"],
  search: [MagnifyingGlass, "regular"],
  up: [ArrowUp, "regular"],
  template: [Stack, "regular"],
  "template-save": [BookmarkSimple, "regular"],
  "others-stop": [SpeakerSlash, "regular"],
  "others-duck": [SpeakerLow, "regular"],
  check: [ListChecks, "regular"],
  split: [Columns, "regular"],
  "zoom-in": [MagnifyingGlassPlus, "regular"],
  "zoom-out": [MagnifyingGlassMinus, "regular"],
  "zoom-fit": [ArrowsOutLineHorizontal, "regular"],
  previous: [SkipBack, "fill"],
  next: [SkipForward, "fill"],
  folder: [FolderSimple, "regular"],
  file: [FileText, "regular"],
  "show-open": [FolderOpen, "regular"],
  "show-new": [FilePlus, "regular"],
  "show-save-as": [FloppyDiskBack, "regular"],
  export: [DownloadSimple, "regular"],
  import: [UploadSimple, "regular"],
  print: [Printer, "regular"],
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
  restore: [ArrowsCounterClockwise, "regular"],
  "align-left": [TextAlignLeft, "regular"],
  "align-center": [TextAlignCenter, "regular"],
  "align-right": [TextAlignRight, "regular"],
  playhead: [CaretRight, "fill"],
  loaded: [CaretDoubleRight, "bold"],
  cart: [SquaresFour, "regular"],
  audition: [Headphones, "regular"],
  auto_continue: [ArrowDown, "bold"],
  auto_follow: [ArrowLineDown, "bold"],
} satisfies Record<string, [PhosphorIcon, IconWeight]>;

export type IconName = keyof typeof ICONS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  const [Glyph, weight] = ICONS[name];
  return <Glyph className="icon" size={size} weight={weight} aria-hidden />;
}
