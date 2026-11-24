// Semnează pachetele de reguli PioCare (ECDSA P-256, SHA-256), pentru actualizare fără reinstalare.
//
//   Generare cheie (o singură dată):
//     node tools/semneaza-reguli.cjs --genereaza <folder-secret>
//       → <folder-secret>/piocare-reguli-privat.pem  (NU se pune în git, se păstrează în siguranță)
//       → afișează cheia publică (hex) de pus în src-tauri/src/rules.rs (PUBLIC_KEY_HEX)
//
//   Semnare:
//     node tools/semneaza-reguli.cjs <cheie-privata.pem> [folder-iesire]
//       → copiază rules/*.json în folder-iesire (implicit dist-reguli/) și scrie lângă fiecare <nume>.json.sig
//
// Pachetele semnate se pun în %ProgramData%\PioCare\rules\ pe calculatorul utilizatorului.
"use strict";
const crypto = require("node:crypto");
const fs = require("node:fs");
const path = require("node:path");

const args = process.argv.slice(2);
if (args[0] === "--genereaza") {
  const dir = args[1];
  if (!dir) throw new Error("lipsește folderul pentru cheia privată");
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "piocare-reguli-privat.pem");
  if (fs.existsSync(file)) throw new Error(`există deja ${file}; nu o suprascriu`);
  const { privateKey, publicKey } = crypto.generateKeyPairSync("ec", { namedCurve: "prime256v1" });
  fs.writeFileSync(file, privateKey.export({ type: "pkcs8", format: "pem" }), { mode: 0o600 });
  const jwk = publicKey.export({ format: "jwk" });
  const pub = Buffer.concat([Buffer.from([4]), Buffer.from(jwk.x, "base64url"), Buffer.from(jwk.y, "base64url")]);
  console.log(`Cheie privată: ${file}`);
  console.log(`PUBLIC_KEY_HEX = "${pub.toString("hex")}"`);
  process.exit(0);
}

const keyFile = args[0];
if (!keyFile) throw new Error("folosire: node tools/semneaza-reguli.cjs <cheie-privata.pem> [folder-iesire]");
const out = args[1] || path.join(__dirname, "..", "dist-reguli");
const key = crypto.createPrivateKey(fs.readFileSync(keyFile));
const src = path.join(__dirname, "..", "rules");
fs.mkdirSync(out, { recursive: true });
for (const f of fs.readdirSync(src).filter((x) => x.endsWith(".json"))) {
  const data = fs.readFileSync(path.join(src, f));
  JSON.parse(data.toString("utf8")); // nu semnăm un JSON stricat
  const sig = crypto.sign("sha256", data, { key, dsaEncoding: "ieee-p1363" }); // r||s, 64 octeți
  fs.writeFileSync(path.join(out, f), data);
  fs.writeFileSync(path.join(out, `${f}.sig`), sig.toString("hex"));
  console.log(`semnat: ${f}`);
}
console.log(`Gata: ${out}`);
