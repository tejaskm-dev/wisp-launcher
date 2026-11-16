import { useCallback, useEffect, useRef, useState } from "react";

export type ToastTone = "info" | "success" | "resting";

export interface Toast {
  id: number;
  tone: ToastTone;
  title: string;
  body?: string;
}

const TOAST_LIFETIME_MS = 5200;
let nextId = 1;

export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const timers = useRef(new Map<number, number>());

  const dismiss = useCallback((id: number) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
    const timer = timers.current.get(id);
    if (timer !== undefined) {
      window.clearTimeout(timer);
      timers.current.delete(id);
    }
  }, []);

  const notify = useCallback(
    (toast: Omit<Toast, "id">) => {
      const id = nextId++;
      setToasts((current) => [...current.slice(-2), { ...toast, id }]);
      timers.current.set(id, window.setTimeout(() => dismiss(id), TOAST_LIFETIME_MS));
    },
    [dismiss],
  );

  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach((timer) => window.clearTimeout(timer));
  }, []);

  return { toasts, notify, dismiss };
}
