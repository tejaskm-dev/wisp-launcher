/** The same server however it's written: "Example.net" and "example.net:25565" match. */
export function addressKey(address: string): string {
  const text = address.trim().toLowerCase();
  if (text.startsWith("[")) {
    const end = text.indexOf("]");
    return `${text.slice(1, end)}:${text.slice(end + 2) || "25565"}`;
  }
  const parts = text.split(":");
  if (parts.length === 2) return `${parts[0].replace(/\.$/, "")}:${parts[1]}`;
  return `${text.replace(/\.$/, "")}:25565`;
}

export const sameServer = (a: string | null | undefined, b: string) => !!a && addressKey(a) === addressKey(b);

const MODE_KEY = "wisp.discoverMode";

/** Whether Discover shows content or servers (remembered between visits). */
export function readDiscoverMode(): "content" | "servers" {
  try {
    return localStorage.getItem(MODE_KEY) === "servers" ? "servers" : "content";
  } catch {
    return "content";
  }
}

export function saveDiscoverMode(mode: "content" | "servers") {
  try {
    localStorage.setItem(MODE_KEY, mode);
  } catch {
    // Only a convenience.
  }
}
