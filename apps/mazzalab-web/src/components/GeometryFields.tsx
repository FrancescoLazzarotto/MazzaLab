import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import type { ShowConnection } from "../api/connection";
import type { Cue, Fit, Geometry, TextAlign, TextStyle, WorkspaceSettings } from "../api/types";
import { useI18n } from "../i18n";
import { compactGeometry, fullGeometry, pictureBox, pictureStyle, stageNames, stageSize, textStyle, type Size } from "../stage";
import { CheckField, ColorField, NumberField, SelectField, TextField } from "./fields";
import { Icon } from "./icons";
import { StageCanvas } from "./StagePage";

type Shown = Extract<Cue, { type: "video" } | { type: "text" }>;

const FITS: Fit[] = ["fit", "fill", "stretch", "native"];
const ALIGNS: TextAlign[] = ["left", "center", "right"];
const FONTS = ["sans-serif", "serif", "monospace", "Arial", "Helvetica", "Verdana", "Georgia", "Times New Roman", "Courier New", "Impact"];
/** How close to the stage's centre lines a dragged picture snaps to them, in screen pixels. */
const SNAP = 6;

/** Rounds to `places` decimals, so that typed and dragged values stay readable. */
const round = (value: number, places = 1) => Math.round(value * 10 ** places) / 10 ** places;

/** The stage a cue shows on, and its size. */
function stageOf(cue: Shown, settings: WorkspaceSettings): Size {
  return stageSize(settings.stages, cue.stage ?? stageNames(settings)[0]);
}

/**
 * The cue as its stage will show it, small: drag the picture to move it,
 * with the stage's centre lines pulling it in.
 */
export function StagePreview({
  connection,
  cue,
  settings,
  geometry,
  onMove,
}: {
  connection: ShowConnection;
  cue: Shown;
  settings: WorkspaceSettings;
  geometry: Geometry | undefined;
  onMove?: (x: number, y: number) => void;
}) {
  const { t } = useI18n();
  const size = stageOf(cue, settings);
  const frame = useRef<HTMLDivElement>(null);
  const [media, setMedia] = useState<Size | null>(null);
  const [drag, setDrag] = useState<{ pointer: number; fromX: number; fromY: number; x: number; y: number; startX: number; startY: number } | null>(null);
  const full = fullGeometry(geometry);
  const shown = drag ? { ...full, x: drag.x, y: drag.y } : full;
  // Screen pixels per stage pixel.
  const scale = () => (frame.current ? frame.current.clientWidth / size.width : 1);

  const down = (event: ReactPointerEvent) => {
    if (!onMove || event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrag({ pointer: event.pointerId, fromX: event.clientX, fromY: event.clientY, x: full.x, y: full.y, startX: full.x, startY: full.y });
  };
  const move = (event: ReactPointerEvent) => {
    if (!drag || event.pointerId !== drag.pointer) return;
    const k = scale();
    const snap = (value: number) => (Math.abs(value * k) < SNAP ? 0 : Math.round(value));
    setDrag({ ...drag, x: snap(drag.startX + (event.clientX - drag.fromX) / k), y: snap(drag.startY + (event.clientY - drag.fromY) / k) });
  };
  const up = (event: ReactPointerEvent) => {
    if (!drag || event.pointerId !== drag.pointer) return;
    setDrag(null);
    if (drag.x !== drag.startX || drag.y !== drag.startY) onMove?.(drag.x, drag.y);
  };

  return (
    <div
      ref={frame}
      className={onMove ? "stage-preview movable" : "stage-preview"}
      style={{ aspectRatio: `${size.width} / ${size.height}` }}
      title={onMove ? t("geometry.dragHint") : undefined}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={() => setDrag(null)}
    >
      <StageCanvas size={size}>
        {cue.type === "video" ? (
          cue.file ? (
            <video
              key={`${cue.file}-${cue.start_time ?? 0}`}
              className="stage-video"
              src={`${connection.cueMediaUrl(cue.id)}#t=${cue.start_time ?? 0}`}
              muted
              preload="metadata"
              style={{ ...pictureStyle(shown, pictureBox(shown.fit, size, media), size), opacity: cue.opacity ?? 1 }}
              onLoadedMetadata={(event) => setMedia({ width: event.currentTarget.videoWidth, height: event.currentTarget.videoHeight })}
            />
          ) : null
        ) : (
          <div className="stage-text" style={{ ...textStyle(shown, cue.style, size), opacity: cue.opacity ?? 1 }}>
            {cue.text || "…"}
          </div>
        )}
      </StageCanvas>
      {drag && drag.x === 0 && <span className="snap-line vertical" />}
      {drag && drag.y === 0 && <span className="snap-line horizontal" />}
      <span className="stage-preview-size">
        {size.width}×{size.height}
      </span>
    </div>
  );
}

/**
 * Position, scale, rotation and crop, as numbers. Changes come out with
 * only what differs from the default, `{}` when nothing does.
 */
export function GeometryFields({ geometry, onChange, withFit = true }: { geometry: Geometry | undefined; onChange: (geometry: Geometry) => void; withFit?: boolean }) {
  const { t } = useI18n();
  const full = fullGeometry(geometry);
  const [linked, setLinked] = useState(full.scale_x === full.scale_y);
  const set = (change: Partial<Geometry>) => onChange(compactGeometry({ ...full, ...change }) ?? {});
  const percent = (value: number) => round(value * 100);
  const crop = (side: "crop_top" | "crop_right" | "crop_bottom" | "crop_left", label: string) => (
    <NumberField label={label} value={percent(full[side])} min={0} onCommit={(value) => set({ [side]: Math.min(49, value) / 100 })} />
  );
  const scale = (axis: "scale_x" | "scale_y") => (value: number) => {
    // A scale of zero would make the picture vanish for good: keep a sliver.
    const factor = Math.abs(value) < 0.1 ? 0.001 : value / 100;
    set(linked ? { scale_x: factor, scale_y: factor } : { [axis]: factor });
  };
  return (
    <div className="geometry-fields">
      <div className="row">
        {withFit && (
          <SelectField label={t("geometry.fit")} value={full.fit} options={FITS.map((fit) => ({ value: fit, label: t(`fit.${fit}`) }))} onChange={(fit) => set({ fit })} />
        )}
        <NumberField label={t("geometry.x")} value={round(full.x)} onCommit={(x) => set({ x })} />
        <NumberField label={t("geometry.y")} value={round(full.y)} onCommit={(y) => set({ y })} />
        <NumberField label={t("geometry.rotation")} value={round(full.rotation)} step={15} onCommit={(rotation) => set({ rotation })} />
      </div>
      <div className="row">
        <NumberField label={t("geometry.scaleX")} value={percent(full.scale_x)} step={5} onCommit={scale("scale_x")} />
        <NumberField label={t("geometry.scaleY")} value={percent(full.scale_y)} step={5} onCommit={scale("scale_y")} />
        <div className="geometry-link">
          <CheckField label={t("geometry.proportional")} checked={linked} onChange={setLinked} />
        </div>
      </div>
      <div className="row">
        {crop("crop_top", t("geometry.cropTop"))}
        {crop("crop_right", t("geometry.cropRight"))}
        {crop("crop_bottom", t("geometry.cropBottom"))}
        {crop("crop_left", t("geometry.cropLeft"))}
      </div>
      <div>
        <button disabled={compactGeometry(withFit ? full : { ...full, fit: "fit" }) === undefined} onClick={() => onChange(withFit ? {} : (compactGeometry({ fit: full.fit }) ?? {}))}>
          <Icon name="restore" size={12} />
          {t("geometry.reset")}
        </button>
      </div>
    </div>
  );
}

/** The Geometry tab of a Video or Text cue: the preview beside the numbers. */
export function GeometryTab({
  connection,
  cue,
  settings,
  set,
}: {
  connection: ShowConnection;
  cue: Shown;
  settings: WorkspaceSettings;
  set: (patch: { geometry: Geometry | undefined }) => void;
}) {
  return (
    <div className="form-columns">
      <div className="form-column preview-column">
        <StagePreview connection={connection} cue={cue} settings={settings} geometry={cue.geometry} onMove={(x, y) => set({ geometry: compactGeometry({ ...cue.geometry, x, y }) })} />
      </div>
      <div className="form-column grow">
        <GeometryFields geometry={cue.geometry} withFit={cue.type === "video"} onChange={(geometry) => set({ geometry: compactGeometry(geometry) })} />
      </div>
    </div>
  );
}

/** Splits `#rrggbbaa` into a colour and an opacity. */
function splitColor(value: string | undefined): { color: string; alpha: number } {
  const match = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec(value ?? "");
  if (!match) return { color: "#000000", alpha: 0.6 };
  return { color: `#${match[1]}`, alpha: match[2] ? Number.parseInt(match[2], 16) / 255 : 1 };
}

function joinColor(color: string, alpha: number): string {
  const byte = Math.round(Math.min(1, Math.max(0, alpha)) * 255);
  return byte === 255 ? color : `${color}${byte.toString(16).padStart(2, "0")}`;
}

/** Font, size, colour and layout of a Text cue. */
export function TextStyleFields({ style, onChange }: { style: TextStyle | undefined; onChange: (style: TextStyle | undefined) => void }) {
  const { t } = useI18n();
  const set = (change: Partial<TextStyle>) => {
    const next: TextStyle = { ...style, ...change };
    // Keep only what differs from how text looks by default.
    const defaults: TextStyle = { font: "sans-serif", size: 72, color: "#ffffff", align: "center", bold: false, italic: false, line_height: 1.2 };
    const kept = Object.fromEntries(
      Object.entries(next).filter(([key, value]) => value !== undefined && value !== defaults[key as keyof TextStyle]),
    ) as TextStyle;
    onChange(Object.keys(kept).length > 0 ? kept : undefined);
  };
  const box = splitColor(style?.background);
  return (
    <div className="text-style-fields">
      <div className="row">
        <TextField label={t("text.font")} value={style?.font ?? "sans-serif"} list="mazzalab-fonts" onCommit={(font) => set({ font: font.trim() || undefined })} />
        <datalist id="mazzalab-fonts">
          {FONTS.map((font) => (
            <option key={font} value={font} />
          ))}
        </datalist>
        <NumberField label={t("text.size")} value={style?.size ?? 72} min={1} onCommit={(size) => set({ size })} />
        <ColorField label={t("text.color")} value={style?.color ?? "#ffffff"} onCommit={(color) => set({ color })} />
      </div>
      <div className="row">
        <div className="field">
          <span className="field-label">{t("text.align")}</span>
          <div className="segmented">
            {ALIGNS.map((align) => (
              <button key={align} className={(style?.align ?? "center") === align ? "active" : undefined} title={t(`align.${align}`)} onClick={() => set({ align })}>
                <Icon name={`align-${align}`} size={13} />
              </button>
            ))}
          </div>
        </div>
        <NumberField label={t("text.lineHeight")} value={style?.line_height ?? 1.2} step={0.05} min={0.5} onCommit={(line_height) => set({ line_height })} />
        <NumberField label={t("text.width")} value={style?.width ?? 0} min={0} step={10} onCommit={(width) => set({ width: width > 0 ? width : undefined })} />
      </div>
      <div className="row checks">
        <CheckField label={t("text.bold")} checked={style?.bold ?? false} onChange={(bold) => set({ bold })} />
        <CheckField label={t("text.italic")} checked={style?.italic ?? false} onChange={(italic) => set({ italic })} />
        <CheckField
          label={t("text.box")}
          checked={style?.background !== undefined}
          onChange={(on) => set({ background: on ? joinColor(box.color, box.alpha) : undefined })}
        />
      </div>
      {style?.background !== undefined && (
        <div className="row">
          <ColorField label={t("text.boxColor")} value={box.color} onCommit={(color) => set({ background: joinColor(color, box.alpha) })} />
          <NumberField
            label={t("text.boxOpacity")}
            value={Math.round(box.alpha * 100)}
            min={0}
            onCommit={(percent) => set({ background: joinColor(box.color, Math.min(100, percent) / 100) })}
          />
        </div>
      )}
    </div>
  );
}
