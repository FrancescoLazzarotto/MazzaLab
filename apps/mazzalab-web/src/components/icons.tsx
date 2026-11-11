import type { ReactNode } from "react";
import type { CueType } from "../api/types";

function Svg({ children, size = 16 }: { children: ReactNode; size?: number }) {
  return (
    <svg
      className="icon"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      {children}
    </svg>
  );
}

const CUE_ICONS: Record<CueType, ReactNode> = {
  audio: (
    <>
      <path d="M2.5 6v4h2.5l3.5 3V3L5 6z" fill="currentColor" stroke="none" />
      <path d="M10.5 5.5a3.5 3.5 0 0 1 0 5M12.3 3.6a6 6 0 0 1 0 8.8" />
    </>
  ),
  video: (
    <>
      <rect x="1.5" y="4" width="9" height="8" rx="1.5" />
      <path d="M10.5 7l4-2.5v7l-4-2.5z" />
    </>
  ),
  text: <path d="M3 4V3h10v1M8 3v10M6 13h4" />,
  mic: (
    <>
      <rect x="6" y="1.5" width="4" height="8" rx="2" />
      <path d="M3.5 7.5a4.5 4.5 0 0 0 9 0M8 12v2.5M5.5 14.5h5" />
    </>
  ),
  camera: (
    <>
      <path d="M1.5 5.5h3l1-1.5h5l1 1.5h3v7h-13z" />
      <circle cx="8" cy="8.7" r="2.2" />
    </>
  ),
  light: (
    <>
      <path d="M5.5 10.5a4 4 0 1 1 5 0V12h-5z" />
      <path d="M6 14.5h4" />
    </>
  ),
  fade: <path d="M1.5 12.5h3c3 0 4-9 7-9h3" />,
  network: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M2 8h12M8 2c-2.2 2-2.2 10 0 12M8 2c2.2 2 2.2 10 0 12" />
    </>
  ),
  midi: (
    <>
      <circle cx="8" cy="8" r="6" />
      <circle cx="5" cy="8" r=".6" fill="currentColor" />
      <circle cx="11" cy="8" r=".6" fill="currentColor" />
      <circle cx="8" cy="5" r=".6" fill="currentColor" />
      <circle cx="6" cy="6" r=".6" fill="currentColor" />
      <circle cx="10" cy="6" r=".6" fill="currentColor" />
    </>
  ),
  midi_file: (
    <>
      <path d="M4 1.5h5l3 3v10H4z" />
      <path d="M7 11V7l3-.7v4" />
    </>
  ),
  timecode: (
    <>
      <circle cx="8" cy="8.5" r="5.5" />
      <path d="M8 5.5v3l2 1.5M6.5 1.5h3" />
    </>
  ),
  group: <path d="M1.5 4.5v8h13v-7H8L6.5 3.5h-5z" />,
  start: <path d="M4.5 3v10l8-5z" fill="currentColor" stroke="none" />,
  stop: <rect x="3.5" y="3.5" width="9" height="9" rx="1" fill="currentColor" stroke="none" />,
  pause: (
    <>
      <rect x="3.5" y="3" width="3" height="10" rx=".8" fill="currentColor" stroke="none" />
      <rect x="9.5" y="3" width="3" height="10" rx=".8" fill="currentColor" stroke="none" />
    </>
  ),
  load: <path d="M8 2v8M4.5 6.5L8 10l3.5-3.5M2.5 13.5h11" />,
  reset: <path d="M3 8a5 5 0 1 0 1.6-3.7M3 2.5v3h3" />,
  devamp: <path d="M2 5.5h8a3 3 0 0 1 0 6H6M8 3L5.5 5.5 8 8M11 9.5l2.5 2-2.5 2" />,
  goto: <path d="M2 8h9M8 4.5L11.5 8 8 11.5M14 3v10" />,
  arm: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M5 8l2.2 2.2L11 6" />
    </>
  ),
  disarm: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M5.5 5.5l5 5M10.5 5.5l-5 5" />
    </>
  ),
  target: (
    <>
      <circle cx="8" cy="8" r="6" />
      <circle cx="8" cy="8" r="2" />
    </>
  ),
  wait: <path d="M4 2h8M4 14h8M5 2c0 3 6 4.5 6 6s-6 3-6 6M11 2c0 3-6 4.5-6 6s6 3 6 6" />,
  memo: (
    <>
      <path d="M3 2.5h10v8l-3 3H3z" />
      <path d="M5.5 5.5h5M5.5 8h3" />
    </>
  ),
  script: <path d="M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5M9 3l-2 10" />,
};

export function CueIcon({ type, size }: { type: CueType; size?: number }) {
  return <Svg size={size}>{CUE_ICONS[type]}</Svg>;
}

export type IconName =
  | "undo"
  | "redo"
  | "save"
  | "settings"
  | "warning"
  | "flag"
  | "close"
  | "copy"
  | "list"
  | "grid"
  | "lock"
  | "edit"
  | "plus"
  | "trash";

const ICONS: Record<IconName, ReactNode> = {
  undo: <path d="M3 6.5h6.5a3.5 3.5 0 0 1 0 7H6M3 6.5L6 3.5M3 6.5l3 3" />,
  redo: <path d="M13 6.5H6.5a3.5 3.5 0 0 0 0 7H10M13 6.5l-3-3M13 6.5l-3 3" />,
  save: (
    <>
      <path d="M2.5 2.5h9l2 2v9h-11z" />
      <path d="M5 2.5v4h5v-4M5 13.5v-4h6v4" />
    </>
  ),
  settings: (
    <>
      <circle cx="8" cy="8" r="2.2" />
      <path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2M3.4 3.4l1.4 1.4M11.2 11.2l1.4 1.4M3.4 12.6l1.4-1.4M11.2 4.8l1.4-1.4" />
    </>
  ),
  warning: (
    <>
      <path d="M8 2L1.5 13.5h13z" />
      <path d="M8 6.5v3.2M8 11.6v.2" />
    </>
  ),
  flag: <path d="M3.5 14V2.5h8l-1.5 3 1.5 3h-8" />,
  close: <path d="M3.5 3.5l9 9M12.5 3.5l-9 9" />,
  copy: (
    <>
      <rect x="5.5" y="5.5" width="8" height="8" rx="1" />
      <path d="M10.5 5.5v-3h-8v8h3" />
    </>
  ),
  list: <path d="M2.5 4h11M2.5 8h11M2.5 12h11" />,
  grid: (
    <>
      <rect x="2.5" y="2.5" width="4.5" height="4.5" rx=".8" />
      <rect x="9" y="2.5" width="4.5" height="4.5" rx=".8" />
      <rect x="2.5" y="9" width="4.5" height="4.5" rx=".8" />
      <rect x="9" y="9" width="4.5" height="4.5" rx=".8" />
    </>
  ),
  lock: (
    <>
      <rect x="3.5" y="7" width="9" height="6.5" rx="1" />
      <path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" />
    </>
  ),
  edit: <path d="M2.5 13.5l.7-3L10.5 3.2l2.3 2.3-7.3 7.3zM9 4.7l2.3 2.3" />,
  plus: <path d="M8 3v10M3 8h10" />,
  trash: <path d="M2.5 4.5h11M6 4.5v-2h4v2M4 4.5l.7 9h6.6l.7-9M6.7 7v4M9.3 7v4" />,
};

export function Icon({ name, size }: { name: IconName; size?: number }) {
  return <Svg size={size}>{ICONS[name]}</Svg>;
}
