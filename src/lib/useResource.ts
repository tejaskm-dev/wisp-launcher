import { useCallback, useEffect, useState } from "react";
import { useLatest } from "./hooks";

export type Resource<T> =
  | { status: "loading" }
  | { status: "ready"; data: T }
  | { status: "error"; message: string };

/** Loads data once on mount; `retry` loads it again. */
export function useResource<T>(load: () => Promise<T>) {
  const [state, setState] = useState<Resource<T>>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const loadRef = useLatest(load);

  useEffect(() => {
    let active = true;
    setState((current) => (current.status === "loading" ? current : { status: "loading" }));
    loadRef.current().then(
      (data) => active && setState({ status: "ready", data }),
      (error: unknown) =>
        active && setState({ status: "error", message: error instanceof Error ? error.message : String(error) }),
    );
    return () => {
      active = false;
    };
  }, [attempt, loadRef]);

  const retry = useCallback(() => setAttempt((count) => count + 1), []);
  return [state, retry] as const;
}
