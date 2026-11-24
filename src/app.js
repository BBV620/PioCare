// PioCare — interfața.
const invoke = window.__TAURI__ ? window.__TAURI__.core.invoke : null;
const $ = (id) => document.getElementById(id);
const fmt = (n, d = 1) => Number(n).toLocaleString("ro-RO", { maximumFractionDigits: d, minimumFractionDigits: d });
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const when = (secs) => new Date(secs * 1000).toLocaleString("ro-RO", { dateStyle: "medium", timeStyle: "short" });

const STATUS = {
  in_progress: ["în curs", "warn"],
  applied: ["aplicată", "ok"],
  rolled_back: ["eșuată, readusă automat", "warn"],
  undone: ["anulată", "muted"],
  partially_undone: ["anulată parțial", "bad"],
  failed: ["eșuată, nimic schimbat", "bad"],
  backup_done: ["copie făcută", "ok"],
};
const STEP = { pending: "în așteptare", applied: "aplicat", failed: "eșuat", undone: "anulat", undo_failed: "anulare eșuată" };

// ---------- navigare ----------
function show(page) {
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("hidden", p.id !== `page-${page}`));
  document.querySelectorAll("nav a[data-page]").forEach((a) => a.classList.toggle("active", a.dataset.page === page));
  if (page === "jurnal") loadJournal();
}
document.querySelectorAll("nav a").forEach((a) =>
  a.addEventListener("click", (e) => {
    e.preventDefault();
    if (a.dataset.page) show(a.dataset.page);
  })
);

// ---------- panou ----------
function uptimeText(h) {
  const days = Math.floor(h / 24);
  const hours = Math.floor(h % 24);
  return days > 0 ? `${days} z ${hours} h` : `${hours} h ${Math.floor((h * 60) % 60)} min`;
}

async function loadOverview() {
  if (!invoke) {
    $("win").textContent = "rulează în afara aplicației";
    return;
  }
  try {
    const o = await invoke("system_overview");
    $("win").textContent = o.windows_name || "Windows";
    $("winver").textContent = [o.windows_version, o.build && `build ${o.build}`].filter(Boolean).join(" · ");
    const used = o.disk_c_total_gb - o.disk_c_free_gb;
    const pct = o.disk_c_total_gb ? (used / o.disk_c_total_gb) * 100 : 0;
    $("disk").textContent = `${fmt(o.disk_c_free_gb)} GB liberi`;
    $("disksub").textContent = `din ${fmt(o.disk_c_total_gb, 0)} GB · ${fmt(pct, 0)}% ocupat`;
    const bar = $("diskbar");
    bar.style.width = `${pct}%`;
    bar.className = pct > 90 ? "bad" : pct > 75 ? "warn" : "";
    $("apps").textContent = o.installed_programs;
    $("uptime").textContent = uptimeText(o.uptime_hours);
    $("pc").textContent = o.computer_name;

    const r = await invoke("rules_info");
    $("rules").textContent = `v${r.version}`;
    $("rulessub").textContent = `${r.rule_count} reguli · ${r.source} · format ${r.schema_version}/${r.supported_schema}`;

    const ops = await invoke("journal_list");
    const b = ops.find((x) => x.kind === "backup" && x.status === "backup_done");
    $("lastbackup").textContent = b ? when(b.created) : "niciuna";
    $("lastbackupsub").textContent = b ? `${b.files.length} fișiere · ${b.restore_point || ""}` : "se face din „Jurnal și anulare”";

    $("mode").textContent = (await invoke("is_admin")) ? "Mod: administrator" : "Mod: utilizator (cere UAC la nevoie)";
  } catch (e) {
    $("win").textContent = "eroare";
    $("winver").textContent = String(e);
  }
}

// ---------- jurnal ----------
function setStatus(text, kind = "") {
  const s = $("status");
  s.textContent = text;
  s.className = `status ${kind}`;
  if (!text) s.classList.add("hidden");
}

function opCard(op) {
  const [label, cls] = STATUS[op.status] || [op.status, ""];
  const canUndo = op.kind === "change" && (op.status === "applied" || op.status === "partially_undone");
  const steps = op.step_texts
    .map((s) => `<li><span class="st ${s.state}">${STEP[s.state] || s.state}</span> ${esc(s.text)}${s.error ? `<div class="err">${esc(s.error)}</div>` : ""}</li>`)
    .join("");
  const extra = [
    op.restore_point && `Windows: ${esc(op.restore_point)}`,
    op.files.length && `Copii: ${op.files.map(esc).join(", ")}`,
    ...op.notes.map(esc),
    op.undone && `Anulată la ${when(op.undone)}`,
  ].filter(Boolean);
  return `
    <div class="op">
      <div class="op-head">
        <div>
          <b>${esc(op.title)}</b>
          <div class="sub">${when(op.created)}${op.needs_admin ? " · cu drepturi de administrator" : ""}${op.rule_id ? ` · regula ${esc(op.rule_id)}` : ""}</div>
        </div>
        <span class="badge ${cls}">${label}</span>
      </div>
      ${steps ? `<ul class="steps">${steps}</ul>` : ""}
      ${extra.length ? `<div class="sub">${extra.join("<br>")}</div>` : ""}
      <div class="op-actions">
        ${canUndo ? `<button data-undo="${esc(op.id)}">Anulează</button>` : ""}
        <button class="ghost" data-open="${esc(op.folder)}">Deschide folderul copiei</button>
      </div>
    </div>`;
}

async function loadJournal() {
  if (!invoke) return;
  try {
    const ops = await invoke("journal_list");
    $("ops").innerHTML = ops.length ? ops.map(opCard).join("") : `<p class="empty">Încă nu există nicio operație. Încearcă „Probă sigură”.</p>`;
  } catch (e) {
    setStatus(String(e), "bad");
  }
}

async function runAction(button, cmd, args, busyText) {
  document.querySelectorAll("button").forEach((b) => (b.disabled = true));
  setStatus(busyText, "warn");
  try {
    const op = await invoke(cmd, args);
    const [label] = STATUS[op.status] || [op.status];
    setStatus(`${op.title}: ${label}.`, op.status === "failed" || op.status === "partially_undone" ? "bad" : "ok");
  } catch (e) {
    setStatus(String(e), "bad");
  } finally {
    document.querySelectorAll("button").forEach((b) => (b.disabled = false));
    loadJournal();
  }
}

$("ops").addEventListener("click", (e) => {
  const b = e.target.closest("button");
  if (!b) return;
  if (b.dataset.undo) runAction(b, "undo_operation", { id: b.dataset.undo }, "Se anulează…");
  if (b.dataset.open) invoke("open_folder", { path: b.dataset.open }).catch((err) => setStatus(String(err), "bad"));
});
$("btn-test").addEventListener("click", (e) => runAction(e.target, "safe_test_operation", {}, "Se aplică proba…"));
$("btn-backup").addEventListener("click", (e) =>
  runAction(e.target, "full_backup", {}, "Se face copia completă (Windows va cere confirmarea de administrator)…")
);
$("refresh").addEventListener("click", loadOverview);

loadOverview();
