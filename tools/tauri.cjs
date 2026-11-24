// Rulează Tauri CLI cu folderul de build pe C: (%LOCALAPPDATA%\PioCare-build).
// Motiv: F: este exFAT, iar rustc se oprește cu STATUS_ACCESS_VIOLATION când compilează acolo.
// Se poate suprascrie cu variabila CARGO_TARGET_DIR.
const { spawnSync } = require("node:child_process");
const path = require("node:path");

const env = { ...process.env };
if (!env.CARGO_TARGET_DIR && env.LOCALAPPDATA) {
  env.CARGO_TARGET_DIR = path.join(env.LOCALAPPDATA, "PioCare-build");
}
const r = spawnSync("npx", ["tauri", ...process.argv.slice(2)], { stdio: "inherit", env, shell: true });
if (env.CARGO_TARGET_DIR) console.log(`\nFolder build: ${env.CARGO_TARGET_DIR}`);
process.exit(r.status ?? 1);
