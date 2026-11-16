import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useShow, type ShowConnection } from "../api/connection";
import type { Geometry, StageItem } from "../api/types";
import { useI18n } from "../i18n";
import { fadeEasing, fullGeometry, geometryAt, mediaTime, opacityAt, pictureBox, pictureStyle, positionAt, stageSize, textStyle, type Size } from "../stage";

/** How far a video may drift from the show's clock before it is put back. */
const DRIFT = 0.12;

/**
 * One stage, full window: the videos and texts the show puts on it, layer
 * over layer, in time with the server. Open it on a projector, a monitor or
 * any browser on the network as `/?stage=Name`.
 */
export function StagePage({ connection, name, monitor }: { connection: ShowConnection; name: string; monitor: boolean }) {
  const { t } = useI18n();
  const { stage, connection: state } = useShow(connection);
  // The server's clock, as it reads here.
  const clock = () => (stage ? stage.now + (performance.now() - stage.received) / 1000 : 0);
  const items = (stage?.items ?? []).filter((item) => item.stage === name);
  const size = stageSize(stage?.stages, name);
  const [soundBlocked, setSoundBlocked] = useState(false);
  const [muted, setMuted] = useState(monitor);

  useEffect(() => {
    document.title = `${name} · MazzaLab`;
  }, [name]);

  return (
    <div className={monitor ? "stage-page monitor" : "stage-page"} onClick={() => soundBlocked && (setMuted(false), setSoundBlocked(false))}>
      <StageCanvas size={size}>
        {items.map((item) =>
          item.kind === "video" ? (
            <StageVideo
              key={`${item.cue}-${item.started}`}
              connection={connection}
              item={item}
              stage={size}
              clock={clock}
              muted={muted}
              onSoundBlocked={() => {
                setMuted(true);
                setSoundBlocked(true);
              }}
            />
          ) : (
            <StageText key={item.cue} item={item} stage={size} clock={clock} />
          ),
        )}
      </StageCanvas>
      {!monitor && state !== "open" && <div className="stage-label">{`${name} · ${t(`connection.${state}`)}`}</div>}
      {soundBlocked && !monitor && <div className="stage-label sound">{t("stage.clickForSound")}</div>}
    </div>
  );
}

/**
 * A stage of `size` pixels, scaled to fit whatever holds it and centred:
 * cues are placed in stage pixels, so they look the same on any screen.
 */
export function StageCanvas({ size, children, className }: { size: Size; children: ReactNode; className?: string }) {
  const outer = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0);
  useLayoutEffect(() => {
    const element = outer.current;
    if (!element) return;
    const fit = () => setScale(Math.min(element.clientWidth / size.width, element.clientHeight / size.height));
    fit();
    const observer = new ResizeObserver(fit);
    observer.observe(element);
    return () => observer.disconnect();
  }, [size.width, size.height]);
  return (
    <div ref={outer} className={className ? `stage-outer ${className}` : "stage-outer"}>
      <div className="stage-canvas" style={{ width: size.width, height: size.height, transform: `translate(-50%, -50%) scale(${scale})` }}>
        {children}
      </div>
    </div>
  );
}

/**
 * The style of one layer: where its fade has got to when it first shows,
 * then gliding to where it is going over what is left of the fade.
 */
function useLayerStyle(item: StageItem, clock: () => number, place: (geometry: Required<Geometry>) => CSSProperties): CSSProperties {
  const [style, setStyle] = useState<CSSProperties>(() => ({ ...place(geometryAt(item, clock())), zIndex: item.layer, opacity: opacityAt(item, clock()) }));
  const target = JSON.stringify([item.opacity, item.geometry, item.fade, item.layer, place(fullGeometry(item.geometry))]);
  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      const fade = item.fade;
      const remaining = fade ? Math.max(0, fade.started + fade.duration - clock()) : 0;
      const easing = fadeEasing(fade?.curve);
      setStyle({
        ...place(fullGeometry(item.geometry)),
        zIndex: item.layer,
        opacity: item.opacity,
        transition: remaining > 0 ? ["opacity", "transform", "clip-path"].map((property) => `${property} ${remaining}s ${easing}`).join(", ") : undefined,
      });
    });
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [target]);
  return style;
}

function StageText({ item, stage, clock }: { item: Extract<StageItem, { kind: "text" }>; stage: Size; clock: () => number }) {
  const style = useLayerStyle(item, clock, (geometry) => textStyle(geometry, item.style, stage));
  return (
    <div className="stage-text" style={style}>
      {item.text}
    </div>
  );
}

function StageVideo({
  connection,
  item,
  stage,
  clock,
  muted,
  onSoundBlocked,
}: {
  connection: ShowConnection;
  item: Extract<StageItem, { kind: "video" }>;
  stage: Size;
  clock: () => number;
  muted: boolean;
  onSoundBlocked: () => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const ended = useRef(false);
  // The file's own size, once it is known.
  const [media, setMedia] = useState<Size | null>(null);
  const style = useLayerStyle(item, clock, (geometry) => pictureStyle(geometry, pictureBox(geometry.fit, stage, media), stage));

  const finish = () => {
    if (ended.current) return;
    ended.current = true;
    void connection.stageEnded(item.cue).catch(() => {});
  };

  // Follow the show: play, pause, and stay within a frame or so of its clock.
  useEffect(() => {
    const element = video.current;
    if (!element) return;
    element.playbackRate = item.rate;
    const follow = () => {
      const duration = Number.isFinite(element.duration) ? element.duration : undefined;
      const target = mediaTime(item, positionAt(item, clock()), duration);
      if (target === null) {
        if (!element.paused) element.pause();
        finish();
        return;
      }
      if (Math.abs(element.currentTime - target) > DRIFT) element.currentTime = target;
      if (item.paused && !element.paused) element.pause();
      if (!item.paused && element.paused) {
        element.play().catch((error: DOMException) => {
          // Browsers hold back sound until someone clicks: play muted meanwhile.
          if (error.name === "NotAllowedError" && !element.muted) {
            onSoundBlocked();
            element.muted = true;
            void element.play().catch(() => {});
          }
        });
      }
    };
    follow();
    const timer = setInterval(follow, 500);
    element.addEventListener("loadedmetadata", follow);
    return () => {
      clearInterval(timer);
      element.removeEventListener("loadedmetadata", follow);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item]);

  return (
    <video
      ref={video}
      className="stage-video"
      style={style}
      src={connection.cueMediaUrl(item.cue)}
      muted={muted}
      playsInline
      preload="auto"
      onLoadedMetadata={(event) => setMedia({ width: event.currentTarget.videoWidth, height: event.currentTarget.videoHeight })}
      onTimeUpdate={(event) => {
        const element = event.currentTarget;
        if (item.end_time === undefined || element.currentTime < item.end_time) return;
        if (item.looped) element.currentTime = item.start_time;
        else {
          element.pause();
          finish();
        }
      }}
      onEnded={(event) => {
        if (item.looped) {
          event.currentTarget.currentTime = item.start_time;
          void event.currentTarget.play().catch(() => {});
        } else finish();
      }}
    />
  );
}
