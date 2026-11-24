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
  cleanup_done: ["curățare făcută", "ok"],
};
const STEP = { pending: "în așteptare", applied: "aplicat", failed: "eșuat", undone: "anulat", undo_failed: "anulare eșuată" };

// ---------- navigare ----------
function show(page) {
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("hidden", p.id !== `page-${page}`));
  document.querySelectorAll("nav a[data-page]").forEach((a) => a.classList.toggle("active", a.dataset.page === page));
  if (page === "jurnal") loadJournal();
  if (page === "servicii") loadServicesPage();
  if (page === "firewall") loadFirewallPage();
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
    // Protecția sistemului: starea directă (cu administrator) sau dedusă din ultimul punct de restaurare încercat.
    const st = await invoke("protection_status");
    const last = ops.find((o) => o.restore_point);
    const lastFailed = last && !/^punct de restaurare (creat|recent)/.test(last.restore_point);
    const lastDisabled = lastFailed && /disabled|dezactivat|no enabled devices|dispozitive activate/i.test(last.restore_point);
    $("protect").classList.toggle("hidden", !(st === false || (st === null && lastDisabled)));
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
// ---------- registru curat ----------
function regStatus(text, kind = "") {
  const s = $("reg-status");
  s.textContent = text;
  s.className = `status ${kind}`;
  if (!text) s.classList.add("hidden");
}

function updateCleanButton() {
  const n = document.querySelectorAll("#reg-results input[type=checkbox]:checked").length;
  $("btn-clean").disabled = n === 0;
  $("btn-clean").textContent = n ? `Curăță selectate (${n})` : "Curăță selectate";
}

async function scanRegistry() {
  $("btn-scan").disabled = true;
  $("btn-clean").disabled = true;
  regStatus("Se scanează registrul…", "warn");
  try {
    const r = await invoke("registry_scan");
    regStatus(`Verificate ${r.scanned} intrări · găsite ${r.findings.length} · reguli v${r.rules_version}`, r.findings.length ? "warn" : "ok");
    if (!r.findings.length) {
      $("reg-results").innerHTML = `<p class="empty">Nu am găsit resturi. Registrul e curat.</p>`;
      return;
    }
    const groups = {};
    r.findings.forEach((f) => (groups[f.category] ||= { title: f.category_title, why: f.why, items: [] }).items.push(f));
    $("reg-results").innerHTML = Object.values(groups)
      .map(
        (g) => `
      <div class="op">
        <div class="op-head"><div><b>${esc(g.title)}</b><div class="sub">${esc(g.why)}</div></div><span class="badge warn">${g.items.length}</span></div>
        <ul class="finds">${g.items
          .map(
            (f) => `<li><label><input type="checkbox" checked data-id="${esc(f.id)}">
              <span><b>${esc(f.title)}</b>${f.needs_admin ? ' <span class="tag">administrator</span>' : ""}
              <div class="sub">${esc(f.location)}</div>
              <div class="sub">lipsește: ${esc(f.missing)}</div></span></label></li>`
          )
          .join("")}</ul>
      </div>`
      )
      .join("");
    updateCleanButton();
  } catch (e) {
    regStatus(String(e), "bad");
  } finally {
    $("btn-scan").disabled = false;
  }
}

$("reg-results").addEventListener("change", updateCleanButton);
$("btn-scan").addEventListener("click", scanRegistry);
$("btn-clean").addEventListener("click", async () => {
  const ids = [...document.querySelectorAll("#reg-results input[type=checkbox]:checked")].map((c) => c.dataset.id);
  if (!ids.length) return;
  $("btn-clean").disabled = $("btn-scan").disabled = true;
  regStatus("Se face copia și se curăță (pentru intrările de sistem Windows va cere confirmarea de administrator)…", "warn");
  try {
    const op = await invoke("registry_clean", { ids });
    const [label] = STATUS[op.status] || [op.status];
    await scanRegistry();
    regStatus(`${op.title}: ${label}. Se poate anula din „Jurnal și anulare”.`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    regStatus(String(e), "bad");
    $("btn-scan").disabled = false;
    updateCleanButton();
  }
});

// ---------- servicii ----------
const START = { auto: "automat", delayed: "automat (întârziat)", manual: "manual", disabled: "oprit", boot: "boot", system: "sistem", "?": "?" };
const RISK = { low: ["risc mic", "ok"], medium: ["risc mediu", "warn"] };
const store = {
  get(k, d) { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};
let svcMap = [];

function svcStatus(text, kind = "") {
  const s = $("svc-status");
  s.textContent = text;
  s.className = `status ${kind}`;
  if (!text) s.classList.add("hidden");
}
const currentProfile = () => document.querySelector("input[name=profile]:checked").value;
const currentUses = () => [...document.querySelectorAll("#features input:checked")].map((c) => c.value);

function updateApply() {
  const n = document.querySelectorAll("#svc-proposals input:checked").length;
  $("btn-svc-apply").disabled = n === 0;
  $("btn-svc-apply").textContent = n ? `Aplică selectate (${n})` : "Aplică selectate";
}

async function analyzeServices() {
  store.set("piocare.profile", currentProfile());
  if ($("features").children.length) store.set("piocare.uses", currentUses());
  svcStatus("Se analizează serviciile…", "warn");
  try {
    const a = await invoke("services_analyze", { profile: currentProfile(), uses: currentUses() });
    if (!$("features").children.length) {
      const saved = store.get("piocare.uses", []);
      $("features").innerHTML = Object.entries(a.features)
        .map(([k, v]) => `<label><input type="checkbox" value="${esc(k)}" ${saved.includes(k) ? "checked" : ""}> ${esc(v)}</label>`)
        .join("");
      if (saved.length) return analyzeServices();
    }
    svcStatus(`${a.total_services} servicii, ${a.running_services} pornite acum · ${a.proposals.length} propuneri · reguli v${a.rules_version}`, a.proposals.length ? "warn" : "ok");
    $("svc-proposals").innerHTML = a.proposals.length
      ? `<div class="op"><ul class="finds">${a.proposals
          .map((p) => {
            const [rl, rc] = RISK[p.risk] || [p.risk, ""];
            return `<li><label><input type="checkbox" data-svc="${esc(p.service)}" ${p.risk === "low" ? "checked" : ""}>
              <span><b>${esc(p.display)}</b> <span class="badge ${rc}">${rl}</span>${p.running ? ' <span class="tag">pornit acum</span>' : ""}
              <div class="sub">${esc(START[p.current] || p.current)} → <b>${esc(START[p.proposed] || p.proposed)}</b> · ${esc(p.service)}</div>
              <div class="sub">${esc(p.why)}</div>
              ${p.note ? `<div class="sub warnline">${esc(p.note)}</div>` : ""}</span></label></li>`;
          })
          .join("")}</ul></div>`
      : `<p class="empty">Nimic de propus pentru acest profil. Serviciile sunt deja reglate.</p>`;
    updateApply();
  } catch (e) {
    svcStatus(String(e), "bad");
  }
}

function renderMap() {
  const q = $("svc-filter").value.trim().toLowerCase();
  const rows = svcMap.filter((s) => !s.user_instance && (!q || s.name.toLowerCase().includes(q) || s.display.toLowerCase().includes(q)));
  $("svc-map").innerHTML = rows
    .slice(0, 400)
    .map(
      (s) => `<div class="svc">
        <div><b>${esc(s.display)}</b> <span class="sub">${esc(s.name)}</span>${s.critical ? ' <span class="tag">vital</span>' : ""}</div>
        <div class="sub">${s.running ? "pornit" : "oprit"} · pornire ${esc(START[s.start] || s.start)}</div>
        ${s.depends_on.length ? `<div class="sub">depinde de: ${s.depends_on.map(esc).join(", ")}</div>` : ""}
        ${s.required_by.length ? `<div class="sub">necesar pentru: ${s.required_by.map(esc).join(", ")}</div>` : ""}
      </div>`
    )
    .join("");
}

async function loadServicesPage() {
  const p = store.get("piocare.profile", "acasa");
  const r = document.querySelector(`input[name=profile][value="${p}"]`);
  if (r) r.checked = true;
  analyzeServices();
  try {
    svcMap = await invoke("services_list");
    renderMap();
  } catch (e) {
    $("svc-map").textContent = String(e);
  }
}

$("profiles").addEventListener("change", analyzeServices);
$("features").addEventListener("change", analyzeServices);
$("svc-proposals").addEventListener("change", updateApply);
$("svc-filter").addEventListener("input", renderMap);
$("btn-svc-apply").addEventListener("click", async () => {
  const names = [...document.querySelectorAll("#svc-proposals input:checked")].map((c) => c.dataset.svc);
  if (!names.length) return;
  $("btn-svc-apply").disabled = true;
  svcStatus("Se aplică (Windows va cere confirmarea de administrator)…", "warn");
  try {
    const op = await invoke("services_apply", { profile: currentProfile(), uses: currentUses(), names });
    const [label] = STATUS[op.status] || [op.status];
    await analyzeServices();
    svcMap = await invoke("services_list");
    renderMap();
    svcStatus(`${op.title}: ${label}. Efect după repornire. Se poate anula din „Jurnal și anulare”.`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    svcStatus(String(e), "bad");
    updateApply();
  }
});

// ---------- firewall ----------
const PROFILE_RO = { Public: "Rețele publice", Private: "Rețeaua de acasă", Domain: "Rețea de firmă" };
const fwUses = () => [...document.querySelectorAll("#fw-features input:checked")].map((c) => c.value);

function fwStatusMsg(text, kind = "") {
  const s = $("fw-status");
  s.textContent = text;
  s.className = `status ${kind}`;
  if (!text) s.classList.add("hidden");
}

function updateFwApply() {
  const n = document.querySelectorAll("#fw-proposals input:checked").length;
  $("btn-fw-apply").disabled = n === 0;
  $("btn-fw-apply").textContent = n ? `Aplică selectate (${n})` : "Aplică selectate";
}

async function loadFirewall() {
  if ($("fw-features").children.length) store.set("piocare.fwuses", fwUses());
  try {
    const s = await invoke("firewall_status", { uses: fwUses() });
    if (!$("fw-features").children.length) {
      const saved = store.get("piocare.fwuses", []);
      $("fw-features").innerHTML = Object.entries(s.features)
        .map(([k, v]) => `<label><input type="checkbox" value="${esc(k)}" ${saved.includes(k) ? "checked" : ""}> ${esc(v)}</label>`)
        .join("");
      if (saved.length) return loadFirewall();
    }
    $("fw-profiles").innerHTML = s.profiles
      .map((p) => {
        const ok = p.enabled && p.inbound !== "Allow";
        return `<div class="card"><h3>${esc(PROFILE_RO[p.profile] || p.profile)}</h3>
          <p class="big ${ok ? "" : "muted"}">${p.enabled ? "pornit" : "OPRIT"}</p>
          <p class="sub">intrări nesolicitate: ${p.inbound === "Allow" ? "PERMISE" : "blocate"}${p.policy_managed ? " · gestionat de politică de grup" : ""}</p></div>`;
      })
      .join("");
    fwStatusMsg(
      `${s.total_rules} reguli în Windows Firewall · ${s.piocare_rules.length} puse de PioCare · ${s.proposals.length} propuneri · reguli v${s.rules_version}`,
      s.proposals.length ? "warn" : "ok"
    );
    $("fw-proposals").innerHTML = s.proposals.length
      ? `<div class="op"><ul class="finds">${s.proposals
          .map((p) => {
            const [rl, rc] = RISK[p.risk] || [p.risk, ""];
            return `<li><label><input type="checkbox" data-fw="${esc(p.id)}" ${p.risk === "low" ? "checked" : ""}>
              <span><b>${esc(p.title)}</b> <span class="badge ${rc}">${rl}</span>
              <div class="sub">${esc(p.why)}</div></span></label></li>`;
          })
          .join("")}</ul></div>`
      : `<p class="empty">Politica de siguranță PioCare este aplicată complet.</p>`;
    updateFwApply();
  } catch (e) {
    fwStatusMsg(String(e), "bad");
  }
}

async function loadActivity() {
  $("fw-activity").innerHTML = `<p class="empty">Se citesc conexiunile…</p>`;
  try {
    const a = await invoke("firewall_activity");
    const head = a.first_run
      ? `<p class="sub">Prima rulare: am memorat ${a.known_count} programe ca fiind cunoscute. De acum, orice program nou va fi marcat.</p>`
      : "";
    $("fw-activity").innerHTML =
      head +
      (a.programs.length
        ? `<div class="map">${a.programs
            .map(
              (p) => `<div class="svc">
          <div><b>${esc(p.name || "?")}</b>${p.new ? ' <span class="badge warn">nou</span>' : ""}${p.blocked ? ' <span class="tag">blocat de PioCare</span>' : ""}</div>
          <div class="sub">${esc(p.path || "cale ascunsă (proces de sistem sau al altui utilizator)")}</div>
          <div class="sub">${p.connections} conexiuni · ${p.remotes.map(esc).join(", ")}</div>
          ${p.path && !p.blocked ? `<div class="op-actions"><button class="ghost" data-block="${esc(p.path)}">Blochează ieșirea pe internet</button></div>` : ""}
        </div>`
            )
            .join("")}</div>`
        : `<p class="empty">Niciun program nu are acum conexiuni spre internet.</p>`);
  } catch (e) {
    $("fw-activity").textContent = String(e);
  }
}

function loadFirewallPage() {
  loadFirewall();
  loadActivity();
}

$("fw-features").addEventListener("change", loadFirewall);
$("fw-proposals").addEventListener("change", updateFwApply);
$("btn-fw-activity").addEventListener("click", loadActivity);
$("btn-fw-apply").addEventListener("click", async () => {
  const ids = [...document.querySelectorAll("#fw-proposals input:checked")].map((c) => c.dataset.fw);
  if (!ids.length) return;
  $("btn-fw-apply").disabled = true;
  fwStatusMsg("Se face copia firewall-ului și se aplică (Windows va cere confirmarea de administrator)…", "warn");
  try {
    const op = await invoke("firewall_apply", { uses: fwUses(), ids });
    const [label] = STATUS[op.status] || [op.status];
    await loadFirewall();
    fwStatusMsg(`${op.title}: ${label}. Se poate anula din „Jurnal și anulare”.`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    fwStatusMsg(String(e), "bad");
    updateFwApply();
  }
});
$("fw-activity").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-block]");
  if (!b) return;
  b.disabled = true;
  fwStatusMsg("Se blochează (Windows va cere confirmarea de administrator)…", "warn");
  try {
    const op = await invoke("firewall_block_program", { path: b.dataset.block });
    const [label] = STATUS[op.status] || [op.status];
    fwStatusMsg(`${op.title}: ${label}.`, op.status === "applied" ? "ok" : "bad");
    loadActivity();
    loadFirewall();
  } catch (err) {
    fwStatusMsg(String(err), "bad");
    b.disabled = false;
  }
});

// ---------- întreținere ----------
const mb = (b) => (b >= 1073741824 ? `${fmt(b / 1073741824, 2)} GB` : `${fmt(b / 1048576, 1)} MB`);
function mtStatus(text, kind = "") {
  const s = $("mt-status");
  s.textContent = text;
  s.className = `status ${kind}`;
  if (!text) s.classList.add("hidden");
}
function updateMtClean() {
  const boxes = [...document.querySelectorAll("#mt-results input:checked")];
  const total = boxes.reduce((a, c) => a + Number(c.dataset.bytes || 0), 0);
  $("btn-mt-clean").disabled = boxes.length === 0;
  $("btn-mt-clean").textContent = boxes.length ? `Curăță selectate (${mb(total)}${boxes.some((c) => c.dataset.unknown) ? "+" : ""})` : "Curăță selectate";
}
async function scanMaintenance() {
  $("btn-mt-scan").disabled = true;
  mtStatus("Se măsoară…", "warn");
  try {
    const s = await invoke("maintenance_scan");
    mtStatus(`Se pot elibera cel puțin ${mb(s.total_bytes)} · reguli v${s.rules_version}`, "ok");
    $("mt-results").innerHTML = `<div class="op"><ul class="finds">${s.items
      .map((i) => {
        const [rl, rc] = RISK[i.risk] || [i.risk, ""];
        const size = i.bytes == null ? (i.admin ? "mărime necunoscută (cere administrator)" : "mărime variabilă") : `${mb(i.bytes)}${i.files != null ? ` · ${i.files} fișiere` : ""}`;
        const empty = i.bytes === 0;
        const checked = i.default && !i.blocked && !empty;
        return `<li><label><input type="checkbox" data-mt="${esc(i.id)}" data-bytes="${i.bytes || 0}" ${i.bytes == null ? 'data-unknown="1"' : ""} ${checked ? "checked" : ""} ${i.blocked || empty ? "disabled" : ""}>
          <span><b>${esc(i.title)}</b> <span class="badge ${rc}">${rl}</span>${i.admin ? ' <span class="tag">administrator</span>' : ""}
          <div class="sub">${size}</div>
          <div class="sub">${esc(i.why)}</div>
          ${i.blocked ? `<div class="sub warnline">${esc(i.blocked)}</div>` : ""}</span></label></li>`;
      })
      .join("")}</ul></div>`;
    updateMtClean();
  } catch (e) {
    mtStatus(String(e), "bad");
  } finally {
    $("btn-mt-scan").disabled = false;
  }
}
$("mt-results").addEventListener("change", updateMtClean);
$("btn-mt-scan").addEventListener("click", scanMaintenance);
$("btn-mt-clean").addEventListener("click", async () => {
  const ids = [...document.querySelectorAll("#mt-results input:checked")].map((c) => c.dataset.mt);
  if (!ids.length) return;
  $("btn-mt-clean").disabled = $("btn-mt-scan").disabled = true;
  mtStatus("Se curăță… (pentru categoriile de sistem Windows va cere confirmarea de administrator; DISM poate dura câteva minute)", "warn");
  try {
    const op = await invoke("maintenance_clean", { ids });
    await scanMaintenance();
    mtStatus(`${op.title}. Detalii în „Jurnal și anulare”.`, "ok");
  } catch (e) {
    mtStatus(String(e), "bad");
    $("btn-mt-scan").disabled = false;
    updateMtClean();
  }
});

$("btn-svc").addEventListener("click", (e) =>
  runAction(e.target, "service_test_operation", {}, "Se aplică proba pe serviciu (Windows va cere confirmarea de administrator)…")
);
$("btn-protect").addEventListener("click", (e) =>
  runAction(e.target, "enable_protection", {}, "Se pornește Protecția sistemului (Windows va cere confirmarea de administrator)…")
);
$("refresh").addEventListener("click", loadOverview);

loadOverview();
