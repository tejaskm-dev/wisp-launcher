// Tester builds update themselves from a channel of their own (see src-tauri/src/tester_updates.rs).
// They can play without a Microsoft account, so their installers are never published: each tester
// run locks its update files with a key only tester builds carry, and puts them beside the public
// update feed with a feed of their own (tester.json). Anyone else sees sealed files.
//
// The key comes from a tester code (WISP- and six groups of four), itself derived from the
// TAURI_SIGNING_PRIVATE_KEY secret, so there's no second secret to keep (a new signing key means
// new installs anyway). Tester builds have the code built in and show it, so their owner can invite
// people; pasted into any other copy of Wisp, it switches that copy to tester builds.
//
//   node scripts/tester-updates.mjs code
//       Prints the tester code. Never let it reach a public log: mask it first.
//   node scripts/tester-updates.mjs pack <bundle folder> <out folder> <system> <run number>
//       Locks each signed update file in the bundle folder with the key from WISP_TESTER_CODE, and
//       writes what the feed needs to know about them to <out folder>/<system>.json.
//   node scripts/tester-updates.mjs feed <folder> <version> <base url> <notes file>
//       Prints tester.json from the pieces pack wrote, with a check value (from WISP_TESTER_CODE)
//       that lets Wisp turn away a wrong code before downloading anything.
//
// A locked file is "WTU1", a 12-byte nonce, then AES-256-GCM ciphertext and its 16-byte tag, with
// "WTU1" as the authenticated data.

import { createCipheriv, hkdfSync, randomBytes } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const MAGIC = Buffer.from("WTU1");

/** Each system's name in the release workflow, as the updater names it. */
const SYSTEMS = {
  "macos-arm64": ["darwin", "aarch64"],
  "macos-x64": ["darwin", "x86_64"],
  "windows-x64": ["windows", "x86_64"],
  "windows-arm64": ["windows", "aarch64"],
  "linux-x64": ["linux", "x86_64"],
  "linux-arm64": ["linux", "aarch64"],
};

/** The update files Tauri signs: [bundle subfolder, file name, the updater's name for the kind]. */
const KINDS = [
  ["macos", /\.app\.tar\.gz$/, "app"],
  ["nsis", /-setup\.exe$/, "nsis"],
  ["msi", /\.msi$/, "msi"],
  ["appimage", /\.AppImage$/, "appimage"],
  ["deb", /\.deb$/, "deb"],
  ["rpm", /\.rpm$/, "rpm"],
];

/** Which kind answers for a system when the installed copy doesn't say (as tauri-action does it). */
const PLAIN = { darwin: ["app"], windows: ["msi", "nsis"], linux: ["appimage"] };

function fail(message) {
  console.error(message);
  process.exit(1);
}

/** Crockford's base 32: no I, L, O or U, so a code copied out by hand still works. */
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/** The tester code's 15 bytes, from the signing secret. */
function codeFromSecret() {
  const secret = (process.env.TAURI_SIGNING_PRIVATE_KEY ?? "").trim();
  if (!secret) fail("TAURI_SIGNING_PRIVATE_KEY isn't set");
  return Buffer.from(hkdfSync("sha256", secret, "wisp-launcher", "tester code v1", 15));
}

/**
 * The seed tester builds make their shared skin key from (hex), so every tester's game trusts every
 * tester's server: wisp_core::skin_server::shared_key. Not in the source, like the code.
 */
function skinSeed() {
  const secret = (process.env.TAURI_SIGNING_PRIVATE_KEY ?? "").trim();
  if (!secret) fail("TAURI_SIGNING_PRIVATE_KEY isn't set");
  return Buffer.from(hkdfSync("sha256", secret, "wisp-launcher", "skin key v1", 32)).toString("hex");
}

/** WISP- and six groups of four. */
function formatCode(code) {
  let bits = BigInt(`0x${code.toString("hex")}`);
  let symbols = "";
  for (let at = 0; at < 24; at += 1) {
    symbols = ALPHABET[Number(bits & 31n)] + symbols;
    bits >>= 5n;
  }
  return `WISP-${symbols.match(/.{4}/g).join("-")}`;
}

/** A code as people pass it around: any case, WISP- or not, dashes anywhere, I, L, O for 1 and 0. */
function parseCode(text) {
  let symbols = text.toUpperCase().replace(/[^0-9A-Z]/g, "");
  if (symbols.length === 28 && symbols.startsWith("WISP")) symbols = symbols.slice(4);
  if (symbols.length !== 24) return null;
  let bits = 0n;
  for (const symbol of symbols) {
    const value = symbol === "I" || symbol === "L" ? 1 : symbol === "O" ? 0 : ALPHABET.indexOf(symbol);
    if (value < 0) return null;
    bits = (bits << 5n) | BigInt(value);
  }
  return Buffer.from(bits.toString(16).padStart(30, "0"), "hex");
}

/** HKDF-SHA256 of the code, salted "wisp-launcher", as the app does it. */
const derive = (code, info, length) => Buffer.from(hkdfSync("sha256", code, "wisp-launcher", info, length));
const keyFor = (code) => derive(code, "tester updates v1", 32);
const checkFor = (code) => derive(code, "tester code check v1", 8).toString("hex");

function codeFromEnv() {
  return parseCode(process.env.WISP_TESTER_CODE ?? "") ?? fail("WISP_TESTER_CODE isn't a tester code");
}

/** The values tester_updates.rs checks against, so both sides are known to agree. */
function codeTest() {
  const code = Buffer.from(Array.from({ length: 15 }, (_, at) => at));
  console.log(JSON.stringify({ code: formatCode(code), key: keyFor(code).toString("hex"), check: checkFor(code), roundTrip: parseCode(formatCode(code)).equals(code) }));
}

function lock(secretKey, data) {
  const nonce = randomBytes(12);
  const cipher = createCipheriv("aes-256-gcm", secretKey, nonce);
  cipher.setAAD(MAGIC);
  const body = Buffer.concat([cipher.update(data), cipher.final()]);
  return Buffer.concat([MAGIC, nonce, body, cipher.getAuthTag()]);
}

function pack(bundles, out, system, run) {
  const names = SYSTEMS[system] ?? fail(`No system called ${system}`);
  const [os, arch] = names;
  const secretKey = keyFor(codeFromEnv());
  if (!/^\d+$/.test(run)) fail(`The run number should be a number, not ${run}`);
  mkdirSync(out, { recursive: true });
  const entries = [];
  for (const [folder, pattern, kind] of KINDS) {
    const dir = join(bundles, folder);
    if (!existsSync(dir)) continue;
    for (const name of readdirSync(dir).filter((file) => pattern.test(file))) {
      const signature = join(dir, `${name}.sig`);
      if (!existsSync(signature)) continue;
      const file = `tester-${run}-${os}-${arch}-${kind}.bin`;
      writeFileSync(join(out, file), lock(secretKey, readFileSync(join(dir, name))));
      entries.push({ kind, file, signature: readFileSync(signature, "utf8").trim() });
    }
  }
  if (entries.length === 0) fail(`No signed update files in ${bundles}`);
  const platforms = {};
  for (const entry of entries) platforms[`${os}-${arch}-${entry.kind}`] = entry;
  const plain = PLAIN[os].map((kind) => entries.find((entry) => entry.kind === kind)).find(Boolean);
  if (plain) platforms[`${os}-${arch}`] = plain;
  writeFileSync(join(out, `${system}.json`), `${JSON.stringify(platforms, null, 2)}\n`);
  console.log(`Locked ${entries.map((entry) => entry.file).join(", ")}`);
}

function feed(folder, version, base, notesFile) {
  const check = checkFor(codeFromEnv());
  const platforms = {};
  for (const name of readdirSync(folder).filter((file) => file.endsWith(".json"))) {
    for (const [platform, entry] of Object.entries(JSON.parse(readFileSync(join(folder, name), "utf8")))) {
      platforms[platform] = { signature: entry.signature, url: `${base.replace(/\/+$/, "")}/${entry.file}` };
    }
  }
  if (Object.keys(platforms).length === 0) fail(`Nothing packed in ${folder}`);
  const notes = readFileSync(notesFile, "utf8").trim();
  console.log(JSON.stringify({ version, notes, pub_date: new Date().toISOString(), check, platforms }, null, 2));
}

const [command, ...rest] = process.argv.slice(2);
if (command === "code") console.log(formatCode(codeFromSecret()));
else if (command === "skin-seed") console.log(skinSeed());
else if (command === "code-test") codeTest();
else if (command === "pack" && rest.length === 4) pack(...rest);
else if (command === "feed" && rest.length === 4) feed(...rest);
else fail("Use: code | skin-seed | pack <bundles> <out> <system> <run> | feed <folder> <version> <base url> <notes file>");
