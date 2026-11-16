import { useEffect, useState } from "react";
import type { ShowConnection } from "./api/connection";
import type { Screens } from "./api/types";

/** How often the displays are looked at again: one may be plugged in, or stages shown from the app's menu. */
const POLL = 4000;

/**
 * The displays of the machine the server runs on, and whether stages are
 * up on them; `null` while unknown, and from a server without screens.
 */
export function useScreens(connection: ShowConnection): { screens: Screens | null; setShown: (shown: boolean) => void } {
  const [screens, setScreens] = useState<Screens | null>(null);
  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setInterval> | undefined;
    const load = () =>
      connection
        .screens()
        .then((next) => {
          if (!alive) return;
          setScreens(next);
          // A server without screens will not grow some.
          if (next === null) clearInterval(timer);
        })
        .catch(() => {});
    void load();
    timer = setInterval(load, POLL);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [connection]);
  const setShown = (shown: boolean) => {
    connection
      .showOnScreens(shown)
      .then((next) => setScreens((current) => current && { ...current, shown: next }))
      .catch((error: Error) => connection.toast("error", error.message));
  };
  return { screens, setShown };
}
