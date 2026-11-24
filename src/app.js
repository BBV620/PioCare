// PioCare — interfața. Textele vin din i18n/<limbă>.js; aici nu se scrie text vizibil direct.
const invoke = window.__TAURI__ ? window.__TAURI__.core.invoke : null;
const $ = (id) => document.getElementById(id);
const LANGS = ["ro", "en"];
let LANG = "ro";

/** Text în limba curentă; {nume} se înlocuiește din `vars`. */
function t(key, vars = {}) {
  const dict = window.PIOCARE_LANGS[LANG] || window.PIOCARE_LANGS.ro;
  const s = dict[key] ?? window.PIOCARE_LANGS.ro[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (_, k) => (vars[k] ?? `{${k}}`));
}
const locale = () => t("locale");
const fmt = (n, d = 1) => Number(n).toLocaleString(locale(), { maximumFractionDigits: d, minimumFractionDigits: d });
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
const when = (secs) => new Date(secs * 1000).toLocaleString(locale(), { dateStyle: "medium", timeStyle: "short" });
const statusLabel = (s) => t(`status.${s}`);
const statusClass = { applied: "ok", backup_done: "ok", cleanup_done: "ok", undone: "muted", in_progress: "warn", rolled_back: "warn", failed: "bad", partially_undone: "bad" };
const risk = (r) => [t(`common.risk.${r}`), r === "low" ? "ok" : "warn"];
const startLabel = (s) => (["auto", "delayed", "manual", "disabled", "boot", "system"].includes(s) ? t(`start.${s}`) : s);

function applyTexts() {
  document.documentElement.lang = LANG;
  document.querySelectorAll("[data-i18n]").forEach((el) => (el.textContent = t(el.dataset.i18n)));
  document.querySelectorAll("[data-i18n-title]").forEach((el) => (el.title = t(el.dataset.i18nTitle)));
  document.querySelectorAll("[data-i18n-ph]").forEach((el) => (el.placeholder = t(el.dataset.i18nPh)));
}

const store = {
  get(k, d) { try { return JSON.parse(localStorage.getItem(k)) ?? d; } catch { return d; } },
  set(k, v) { try { localStorage.setItem(k, JSON.stringify(v)); } catch {} },
};

function statusBox(id) {
  return (text, kind = "") => {
    const s = $(id);
    s.textContent = text;
    s.className = `status ${kind}`;
    if (!text) s.classList.add("hidden");
  };
}

// ---------- limbă ----------
function fillLangSelect() {
  $("lang-select").innerHTML = LANGS.map((l) => `<option value="${l}" ${l === LANG ? "selected" : ""}>${esc(window.PIOCARE_LANGS[l]["lang.name"])}</option>`).join("");
}
async function chooseLanguage(lang) {
  if (invoke) await invoke("set_language", { lang });
  location.reload(); // reîncarcă toate textele, inclusiv cele venite din motor
}
$("lang-select").addEventListener("change", (e) => chooseLanguage(e.target.value));
document.querySelectorAll("#lang-pick button[data-lang]").forEach((b) => b.addEventListener("click", () => chooseLanguage(b.dataset.lang)));

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
  const d = Math.floor(h / 24);
  const hh = Math.floor(h % 24);
  return d > 0 ? t("panou.days", { d, h: hh }) : t("panou.hours", { h: hh, m: Math.floor((h * 60) % 60) });
}

async function loadOverview() {
  if (!invoke) {
    $("win").textContent = t("common.outside");
    return;
  }
  try {
    const o = await invoke("system_overview");
    $("win").textContent = o.windows_name || "Windows";
    $("winver").textContent = [o.windows_version, o.build && `build ${o.build}`].filter(Boolean).join(" · ");
    const used = o.disk_c_total_gb - o.disk_c_free_gb;
    const pct = o.disk_c_total_gb ? (used / o.disk_c_total_gb) * 100 : 0;
    $("disk").textContent = t("panou.diskFree", { gb: fmt(o.disk_c_free_gb) });
    $("disksub").textContent = t("panou.diskSub", { total: fmt(o.disk_c_total_gb, 0), pct: fmt(pct, 0) });
    const bar = $("diskbar");
    bar.style.width = `${pct}%`;
    bar.className = pct > 90 ? "bad" : pct > 75 ? "warn" : "";
    $("apps").textContent = o.installed_programs;
    $("uptime").textContent = uptimeText(o.uptime_hours);
    $("pc").textContent = o.computer_name;

    const r = await invoke("rules_info");
    $("rules").textContent = `v${r.version}`;
    $("rulessub").textContent = t("panou.rulesSub", { n: r.rule_count, src: r.source, s: r.schema_version, ss: r.supported_schema });

    const ops = await invoke("journal_list");
    const b = ops.find((x) => x.kind === "backup" && x.status === "backup_done");
    $("lastbackup").textContent = b ? when(b.created) : t("panou.none");
    $("lastbackupsub").textContent = b ? t("panou.backupSub", { n: b.files.length, rp: b.restore_point || "" }) : t("panou.backupHint");

    $("mode").textContent = (await invoke("is_admin")) ? t("mode.admin") : t("mode.user");
  } catch (e) {
    $("win").textContent = t("common.error");
    $("winver").textContent = String(e);
  }
}

// ---------- jurnal ----------
const setStatus = statusBox("status");

function opCard(op) {
  const label = statusLabel(op.status);
  const cls = statusClass[op.status] || "";
  const canUndo = op.kind === "change" && (op.status === "applied" || op.status === "partially_undone");
  const steps = op.step_texts
    .map((s) => `<li><span class="st ${s.state}">${esc(t(`step.${s.state}`))}</span> ${esc(s.text)}${s.error ? `<div class="err">${esc(s.error)}</div>` : ""}</li>`)
    .join("");
  const extra = [
    op.restore_point && `Windows: ${esc(op.restore_point)}`,
    op.files.length && esc(t("j.copies", { l: op.files.join(", ") })),
    ...op.notes.map(esc),
    op.undone && esc(t("j.undoneAt", { d: when(op.undone) })),
  ].filter(Boolean);
  return `
    <div class="op">
      <div class="op-head">
        <div>
          <b>${esc(op.title)}</b>
          <div class="sub">${when(op.created)}${op.needs_admin ? esc(t("j.withAdmin")) : ""}${op.rule_id ? esc(t("j.rule", { r: op.rule_id })) : ""}</div>
        </div>
        <span class="badge ${cls}">${esc(label)}</span>
      </div>
      ${steps ? `<ul class="steps">${steps}</ul>` : ""}
      ${extra.length ? `<div class="sub">${extra.join("<br>")}</div>` : ""}
      <div class="op-actions">
        ${canUndo ? `<button data-undo="${esc(op.id)}">${esc(t("j.undo"))}</button>` : ""}
        <button class="ghost" data-open="${esc(op.folder)}">${esc(t("j.openFolder"))}</button>
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
    const lastFailed = last && /necreat|not created/.test(last.restore_point);
    const lastDisabled = lastFailed && /disabled|dezactivat|no enabled devices|dispozitive activate/i.test(last.restore_point);
    $("protect").classList.toggle("hidden", !(st === false || (st === null && lastDisabled)));
    $("ops").innerHTML = ops.length ? ops.map(opCard).join("") : `<p class="empty">${esc(t("j.empty"))}</p>`;
  } catch (e) {
    setStatus(String(e), "bad");
  }
}

async function runAction(cmd, args, busyKey) {
  document.querySelectorAll("button").forEach((b) => (b.disabled = true));
  setStatus(t(busyKey), "warn");
  try {
    const op = await invoke(cmd, args);
    setStatus(t("j.result", { title: op.title, label: statusLabel(op.status) }), op.status === "failed" || op.status === "partially_undone" ? "bad" : "ok");
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
  if (b.dataset.undo) runAction("undo_operation", { id: b.dataset.undo }, "j.undoing");
  if (b.dataset.open) invoke("open_folder", { path: b.dataset.open }).catch((err) => setStatus(String(err), "bad"));
});
$("btn-test").addEventListener("click", () => runAction("safe_test_operation", {}, "j.testing"));
$("btn-svc").addEventListener("click", () => runAction("service_test_operation", {}, "j.svcTesting"));
$("btn-backup").addEventListener("click", () => runAction("full_backup", {}, "j.backingUp"));
$("btn-protect").addEventListener("click", () => runAction("enable_protection", {}, "j.protecting"));

// ---------- registru curat ----------
const regStatus = statusBox("reg-status");

function updateCleanButton() {
  const n = document.querySelectorAll("#reg-results input[type=checkbox]:checked").length;
  $("btn-clean").disabled = n === 0;
  $("btn-clean").textContent = n ? t("common.cleanN", { n }) : t("common.clean");
}

async function scanRegistry() {
  $("btn-scan").disabled = true;
  $("btn-clean").disabled = true;
  regStatus(t("reg.scanning"), "warn");
  try {
    const r = await invoke("registry_scan");
    regStatus(`${t("reg.summary", { n: r.scanned, f: r.findings.length })} · ${t("common.rules", { v: r.rules_version })}`, r.findings.length ? "warn" : "ok");
    if (!r.findings.length) {
      $("reg-results").innerHTML = `<p class="empty">${esc(t("reg.clean"))}</p>`;
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
              <span><b>${esc(f.title)}</b>${f.needs_admin ? ` <span class="tag">${esc(t("common.admin"))}</span>` : ""}
              <div class="sub">${esc(f.location)}</div>
              <div class="sub">${esc(t("reg.missing", { p: f.missing }))}</div></span></label></li>`
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
  regStatus(t("reg.cleaning"), "warn");
  try {
    const op = await invoke("registry_clean", { ids });
    await scanRegistry();
    regStatus(`${t("j.result", { title: op.title, label: statusLabel(op.status) })} ${t("common.undoHint")}`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    regStatus(String(e), "bad");
    $("btn-scan").disabled = false;
    updateCleanButton();
  }
});

// ---------- servicii ----------
let svcMap = [];
const svcStatus = statusBox("svc-status");
const currentProfile = () => document.querySelector("input[name=profile]:checked").value;
const currentUses = () => [...document.querySelectorAll("#features input:checked")].map((c) => c.value);

function updateApply() {
  const n = document.querySelectorAll("#svc-proposals input:checked").length;
  $("btn-svc-apply").disabled = n === 0;
  $("btn-svc-apply").textContent = n ? t("common.applyN", { n }) : t("common.apply");
}

async function analyzeServices() {
  store.set("piocare.profile", currentProfile());
  if ($("features").children.length) store.set("piocare.uses", currentUses());
  svcStatus(t("svc.analyzing"), "warn");
  try {
    const a = await invoke("services_analyze", { profile: currentProfile(), uses: currentUses() });
    if (!$("features").children.length) {
      const saved = store.get("piocare.uses", []);
      $("features").innerHTML = Object.entries(a.features)
        .map(([k, v]) => `<label><input type="checkbox" value="${esc(k)}" ${saved.includes(k) ? "checked" : ""}> ${esc(v)}</label>`)
        .join("");
      if (saved.length) return analyzeServices();
    }
    svcStatus(`${t("svc.summary", { n: a.total_services, r: a.running_services, p: a.proposals.length })} · ${t("common.rules", { v: a.rules_version })}`, a.proposals.length ? "warn" : "ok");
    $("svc-proposals").innerHTML = a.proposals.length
      ? `<div class="op"><ul class="finds">${a.proposals
          .map((p) => {
            const [rl, rc] = risk(p.risk);
            return `<li><label><input type="checkbox" data-svc="${esc(p.service)}" ${p.risk === "low" ? "checked" : ""}>
              <span><b>${esc(p.display)}</b> <span class="badge ${rc}">${esc(rl)}</span>${p.running ? ` <span class="tag">${esc(t("svc.runningNow"))}</span>` : ""}
              <div class="sub">${esc(startLabel(p.current))} → <b>${esc(startLabel(p.proposed))}</b> · ${esc(p.service)}</div>
              <div class="sub">${esc(p.why)}</div>
              ${p.note ? `<div class="sub warnline">${esc(p.note)}</div>` : ""}</span></label></li>`;
          })
          .join("")}</ul></div>`
      : `<p class="empty">${esc(t("svc.none"))}</p>`;
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
        <div><b>${esc(s.display)}</b> <span class="sub">${esc(s.name)}</span>${s.critical ? ` <span class="tag">${esc(t("svc.vital"))}</span>` : ""}</div>
        <div class="sub">${esc(s.running ? t("svc.on") : t("svc.off"))} · ${esc(t("svc.startup", { s: startLabel(s.start) }))}</div>
        ${s.depends_on.length ? `<div class="sub">${esc(t("svc.dependsOn", { l: s.depends_on.join(", ") }))}</div>` : ""}
        ${s.required_by.length ? `<div class="sub">${esc(t("svc.requiredBy", { l: s.required_by.join(", ") }))}</div>` : ""}
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
  svcStatus(t("svc.applying"), "warn");
  try {
    const op = await invoke("services_apply", { profile: currentProfile(), uses: currentUses(), names });
    await analyzeServices();
    svcMap = await invoke("services_list");
    renderMap();
    svcStatus(t("svc.applied", { title: op.title, label: statusLabel(op.status) }), op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    svcStatus(String(e), "bad");
    updateApply();
  }
});

// ---------- firewall ----------
const fwUses = () => [...document.querySelectorAll("#fw-features input:checked")].map((c) => c.value);
const fwStatusMsg = statusBox("fw-status");

function updateFwApply() {
  const n = document.querySelectorAll("#fw-proposals input:checked").length;
  $("btn-fw-apply").disabled = n === 0;
  $("btn-fw-apply").textContent = n ? t("common.applyN", { n }) : t("common.apply");
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
        return `<div class="card"><h3>${esc(t(`fw.${p.profile}`))}</h3>
          <p class="big ${ok ? "" : "muted"}">${esc(p.enabled ? t("fw.on") : t("fw.off"))}</p>
          <p class="sub">${esc(t("fw.inbound", { s: p.inbound === "Allow" ? t("fw.allowed") : t("fw.blocked") }))}${p.policy_managed ? esc(t("fw.gpo")) : ""}</p></div>`;
      })
      .join("");
    fwStatusMsg(
      `${t("fw.summary", { n: s.total_rules, p: s.piocare_rules.length, q: s.proposals.length })} · ${t("common.rules", { v: s.rules_version })}`,
      s.proposals.length ? "warn" : "ok"
    );
    $("fw-proposals").innerHTML = s.proposals.length
      ? `<div class="op"><ul class="finds">${s.proposals
          .map((p) => {
            const [rl, rc] = risk(p.risk);
            return `<li><label><input type="checkbox" data-fw="${esc(p.id)}" ${p.risk === "low" ? "checked" : ""}>
              <span><b>${esc(p.title)}</b> <span class="badge ${rc}">${esc(rl)}</span>
              <div class="sub">${esc(p.why)}</div></span></label></li>`;
          })
          .join("")}</ul></div>`
      : `<p class="empty">${esc(t("fw.complete"))}</p>`;
    updateFwApply();
  } catch (e) {
    fwStatusMsg(String(e), "bad");
  }
}

async function loadActivity() {
  $("fw-activity").innerHTML = `<p class="empty">${esc(t("fw.reading"))}</p>`;
  try {
    const a = await invoke("firewall_activity");
    const head = a.first_run ? `<p class="sub">${esc(t("fw.firstRun", { n: a.known_count }))}</p>` : "";
    $("fw-activity").innerHTML =
      head +
      (a.programs.length
        ? `<div class="map">${a.programs
            .map(
              (p) => `<div class="svc">
          <div><b>${esc(p.name || "?")}</b>${p.new ? ` <span class="badge warn">${esc(t("fw.new"))}</span>` : ""}${p.blocked ? ` <span class="tag">${esc(t("fw.blockedBy"))}</span>` : ""}</div>
          <div class="sub">${esc(p.path || t("fw.hiddenPath"))}</div>
          <div class="sub">${esc(t("fw.conns", { n: p.connections, l: p.remotes.join(", ") }))}</div>
          ${p.path && !p.blocked ? `<div class="op-actions"><button class="ghost" data-block="${esc(p.path)}">${esc(t("fw.block"))}</button></div>` : ""}
        </div>`
            )
            .join("")}</div>`
        : `<p class="empty">${esc(t("fw.noConns"))}</p>`);
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
  fwStatusMsg(t("fw.applying"), "warn");
  try {
    const op = await invoke("firewall_apply", { uses: fwUses(), ids });
    await loadFirewall();
    fwStatusMsg(`${t("j.result", { title: op.title, label: statusLabel(op.status) })} ${t("common.undoHint")}`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    fwStatusMsg(String(e), "bad");
    updateFwApply();
  }
});
$("fw-activity").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-block]");
  if (!b) return;
  b.disabled = true;
  fwStatusMsg(t("fw.blocking"), "warn");
  try {
    const op = await invoke("firewall_block_program", { path: b.dataset.block });
    fwStatusMsg(t("j.result", { title: op.title, label: statusLabel(op.status) }), op.status === "applied" ? "ok" : "bad");
    loadActivity();
    loadFirewall();
  } catch (err) {
    fwStatusMsg(String(err), "bad");
    b.disabled = false;
  }
});

// ---------- întreținere ----------
const mb = (b) => (b >= 1073741824 ? `${fmt(b / 1073741824, 2)} GB` : `${fmt(b / 1048576, 1)} MB`);
const mtStatus = statusBox("mt-status");

function updateMtClean() {
  const boxes = [...document.querySelectorAll("#mt-results input:checked")];
  const total = boxes.reduce((a, c) => a + Number(c.dataset.bytes || 0), 0);
  $("btn-mt-clean").disabled = boxes.length === 0;
  $("btn-mt-clean").textContent = boxes.length
    ? t("mt.cleanSize", { size: mb(total) + (boxes.some((c) => c.dataset.unknown) ? "+" : "") })
    : t("common.clean");
}

async function scanMaintenance() {
  $("btn-mt-scan").disabled = true;
  mtStatus(t("mt.measuring"), "warn");
  try {
    const s = await invoke("maintenance_scan");
    mtStatus(`${t("mt.total", { size: mb(s.total_bytes) })} · ${t("common.rules", { v: s.rules_version })}`, "ok");
    $("mt-results").innerHTML = `<div class="op"><ul class="finds">${s.items
      .map((i) => {
        const [rl, rc] = risk(i.risk);
        const size =
          i.bytes == null
            ? i.admin ? t("mt.unknownAdmin") : t("mt.variable")
            : `${mb(i.bytes)}${i.files != null ? ` · ${t("mt.files", { n: i.files })}` : ""}`;
        const empty = i.bytes === 0;
        const checked = i.default && !i.blocked && !empty;
        return `<li><label><input type="checkbox" data-mt="${esc(i.id)}" data-bytes="${i.bytes || 0}" ${i.bytes == null ? 'data-unknown="1"' : ""} ${checked ? "checked" : ""} ${i.blocked || empty ? "disabled" : ""}>
          <span><b>${esc(i.title)}</b> <span class="badge ${rc}">${esc(rl)}</span>${i.admin ? ` <span class="tag">${esc(t("common.admin"))}</span>` : ""}
          <div class="sub">${esc(size)}</div>
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
  mtStatus(t("mt.cleaning"), "warn");
  try {
    const op = await invoke("maintenance_clean", { ids });
    await scanMaintenance();
    const skipped = op.notes.filter((n) => /sărit|eroare|refuzat|în uz|skipped|error|refused|in use/.test(n));
    mtStatus(t("mt.done", { title: op.title, warn: skipped.length ? t("mt.attention") + skipped.join(" | ") : "" }), skipped.length ? "warn" : "ok");
  } catch (e) {
    mtStatus(String(e), "bad");
    $("btn-mt-scan").disabled = false;
    updateMtClean();
  }
});

$("refresh").addEventListener("click", loadOverview);

// ---------- pornire ----------
(async function start() {
  let info = { lang: null, suggestion: "ro" };
  if (invoke) {
    try {
      info = await invoke("get_language");
    } catch {}
  }
  LANG = LANGS.includes(info.lang) ? info.lang : LANGS.includes(info.suggestion) ? info.suggestion : "ro";
  applyTexts();
  fillLangSelect();
  if (invoke && !info.lang) {
    // Prima pornire: întrebăm limba; butonul sugerat (limba Windows) e primul în focus.
    $("lang-pick").classList.remove("hidden");
    document.querySelector(`#lang-pick button[data-lang="${LANG}"]`)?.focus();
  }
  loadOverview();
})();
