import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useShow, type ShowConnection } from "../api/connection";
import type { StageItem } from "../api/types";
import { useI18n } from "../i18n";
import { mediaTime, opacityAt, positionAt } from "../stage";

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
  const [soundBlocked, setSoundBlocked] = useState(false);
  const [muted, setMuted] = useState(monitor);

  useEffect(() => {
    document.title = `${name} · MazzaLab`;
  }, [name]);

  return (
    <div className={monitor ? "stage-page monitor" : "stage-page"} onClick={() => soundBlocked && (setMuted(false), setSoundBlocked(false))}>
      {items.map((item) =>
        item.kind === "video" ? (
          <StageVideo
            key={`${item.cue}-${item.started}`}
            connection={connection}
            item={item}
            clock={clock}
            muted={muted}
            onSoundBlocked={() => {
              setMuted(true);
              setSoundBlocked(true);
            }}
          />
        ) : (
          <div key={item.cue} className="stage-text" style={layerStyle(item, clock())}>
            {item.text}
          </div>
        ),
      )}
      {!monitor && state !== "open" && <div className="stage-label">{`${name} · ${t(`connection.${state}`)}`}</div>}
      {soundBlocked && !monitor && <div className="stage-label sound">{t("stage.clickForSound")}</div>}
    </div>
  );
}

/** Layer and opacity, the opacity gliding to its target over what is left of a fade. */
function layerStyle(item: StageItem, now: number): CSSProperties {
  const remaining = item.fade ? Math.max(0, item.fade.started + item.fade.duration - now) : 0;
  return {
    zIndex: item.layer,
    opacity: item.opacity,
    transition: remaining > 0 ? `opacity ${remaining}s linear` : undefined,
  };
}

function StageVideo({
  connection,
  item,
  clock,
  muted,
  onSoundBlocked,
}: {
  connection: ShowConnection;
  item: Extract<StageItem, { kind: "video" }>;
  clock: () => number;
  muted: boolean;
  onSoundBlocked: () => void;
}) {
  const video = useRef<HTMLVideoElement>(null);
  const ended = useRef(false);
  // The opacity a fade starts from, applied first so that the transition has somewhere to go from.
  const [style, setStyle] = useState<CSSProperties>(() => ({ zIndex: item.layer, opacity: opacityAt(item, clock()) }));
  useEffect(() => {
    const frame = requestAnimationFrame(() => setStyle(layerStyle(item, clock())));
    return () => cancelAnimationFrame(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [item.opacity, item.fade, item.layer]);

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
