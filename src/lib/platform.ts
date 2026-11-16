export const isMac = typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.userAgent);

/** Label for the primary shortcut modifier on this platform. */
export const modKey = isMac ? "⌘" : "Ctrl ";
