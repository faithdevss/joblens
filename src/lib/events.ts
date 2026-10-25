import { listen } from "@tauri-apps/api/event";
import type { FeedUpdated } from "../bindings/FeedUpdated";

/** Calls `handler` after each background refresh. Returns a function that stops listening. */
export function onFeedUpdated(handler: (update: FeedUpdated) => void): () => void {
  let stop: (() => void) | null = null;
  let stopped = false;
  listen<FeedUpdated>("feed-updated", (e) => handler(e.payload))
    .then((unlisten) => {
      if (stopped) unlisten();
      else stop = unlisten;
    })
    .catch(() => {});
  return () => {
    stopped = true;
    stop?.();
  };
}
