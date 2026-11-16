import DOMPurify from "dompurify";
import { marked } from "marked";

/** Image hosts that are Modrinth's own (shown even with other websites' images turned off). */
const MODRINTH_IMAGE_HOSTS = ["cdn.modrinth.com", "cdn-raw.modrinth.com"];

// Set for the duration of one renderMarkdown call (DOMPurify hooks are global).
let blockExternalImages = false;
let blockedImages = 0;

// Images in descriptions load only when scrolled near, and never send a referrer. With other
// websites' images turned off (Settings → Privacy), those lose their address so they never load.
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName !== "IMG") return;
  node.setAttribute("loading", "lazy");
  node.setAttribute("referrerpolicy", "no-referrer");
  if (!blockExternalImages) return;
  let host = "";
  try {
    host = new URL(node.getAttribute("src") ?? "").hostname;
  } catch {
    // No usable address: it wouldn't load anyway.
  }
  if (!MODRINTH_IMAGE_HOSTS.includes(host)) {
    node.removeAttribute("src");
    node.removeAttribute("srcset");
    node.setAttribute("data-blocked", "");
    blockedImages += 1;
  }
});

/**
 * Turns a project's Markdown (which may include HTML) into safe HTML. Descriptions come from
 * strangers, and Wisp's window can talk to the launcher, so anything that could run code, load
 * a frame, submit a form or restyle the page is stripped.
 */
export function renderMarkdown(source: string, { externalImages = true } = {}): { html: string; blockedImages: number } {
  const html = marked.parse(source, { async: false, gfm: true, breaks: false });
  blockExternalImages = !externalImages;
  blockedImages = 0;
  try {
    const clean = DOMPurify.sanitize(html, {
      USE_PROFILES: { html: true },
      FORBID_TAGS: ["style", "iframe", "frame", "object", "embed", "form", "input", "button", "textarea", "select", "video", "audio"],
      FORBID_ATTR: ["style", "class", "id"],
      ALLOWED_URI_REGEXP: /^(?:https?:|#)/i,
    });
    return { html: clean, blockedImages };
  } finally {
    blockExternalImages = false;
  }
}

/** Category slugs as words: "game-mechanics" → "Game mechanics". */
export function categoryLabel(slug: string): string {
  const special: Record<string, string> = {
    worldgen: "World generation",
    "kitchen-sink": "Kitchen sink",
    pbr: "PBR",
    potato: "Potato (very low-end)",
    "path-tracing": "Path tracing",
    "core-shaders": "Core shaders",
  };
  if (special[slug]) return special[slug];
  const words = slug.replace(/-/g, " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}
