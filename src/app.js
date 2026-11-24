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
/** Starea punctului de restaurare, tradusă la afișare (textul din jurnal rămâne în limba de atunci). */
function rpLabel(text) {
  if (!text) return "";
  if (/necreat|not created/.test(text)) return t("rp.failed");
  if (/recent/.test(text)) return t("rp.recent");
  if (/creat|created/.test(text)) return t("rp.ok");
  return text;
}
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

// ---------- „Ce este?” ----------
const VERDICT_CLS = { vital: "bad", keep: "ok", optional: "warn", safe_off: "ok", unknown: "muted" };
/** Buton ⓘ; kind = "service" | "program". */
const infoBtn = (kind, name, path = "") =>
  `<button class="info" data-ex-kind="${kind}" data-ex-name="${esc(name)}" data-ex-path="${esc(path)}" title="${esc(t("ex.btn"))}">${esc(t("ex.btn"))}</button>`;

async function openExplain(kind, name, path) {
  $("ex-name").textContent = name;
  $("ex-verdict").textContent = "";
  $("ex-verdict").className = "badge";
  $("ex-body").innerHTML = `<p class="sub">${esc(t("ex.loading"))}</p>`;
  $("explain").classList.remove("hidden");
  try {
    const e = await invoke("explain", { kind, name, path: path || null });
    $("ex-name").textContent = e.file_description && kind === "program" ? `${name} · ${e.file_description}` : name;
    $("ex-verdict").textContent = t(`ex.v.${e.verdict}`);
    $("ex-verdict").className = `badge ${VERDICT_CLS[e.verdict] || ""}`;
    const sec = (title, body) => (body ? `<div class="exsec"><b>${esc(title)}</b>${esc(body)}</div>` : "");
    let html = "";
    if (e.known) {
      html += sec(t("ex.what"), e.what) + sec(t("ex.off"), e.off);
    } else if (e.verdict === "vital") {
      html += `<p>${esc(t("ex.vitalGeneric"))}</p>`;
    } else {
      html += `<p class="sub">${esc(t("ex.unknown"))}</p>`;
    }
    html += sec(t("ex.windows"), e.windows_description);
    html += sec(t("ex.company"), e.company);
    html += sec(t("ex.file"), e.path);
    html += `<p class="sub" style="margin-top:14px">${esc(t("ex.base", { v: e.base_version }))}</p>`;
    const q = [name.replace(/\.exe$/i, ""), e.company || e.file_description || ""].filter(Boolean).join(" ");
    html += `<div class="op-actions"><button class="ghost" id="ex-search" data-q="${esc(q)}">${esc(t("ex.search"))}</button></div>
      <p class="sub">${esc(t("ex.searchHint"))}</p>`;
    $("ex-body").innerHTML = html;
    $("ex-search").addEventListener("click", (ev) => invoke("search_web", { query: ev.target.dataset.q }).catch(() => {}));
  } catch (err) {
    $("ex-body").textContent = String(err);
  }
}
document.addEventListener(
  "click",
  (e) => {
    const b = e.target.closest("button.info");
    if (!b) return;
    e.preventDefault(); // nu bifa/debifa căsuța din eticheta în care stă butonul
    e.stopPropagation();
    openExplain(b.dataset.exKind, b.dataset.exName, b.dataset.exPath);
  },
  true
);
$("ex-close").addEventListener("click", () => $("explain").classList.add("hidden"));
$("explain").addEventListener("click", (e) => {
  if (e.target.id === "explain") $("explain").classList.add("hidden");
});
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") $("explain").classList.add("hidden");
});

// ---------- navigare ----------
function show(page) {
  stopMonitor();
  if (page === "monitor") startMonitor();
  document.querySelectorAll(".page").forEach((p) => p.classList.toggle("hidden", p.id !== `page-${page}`));
  document.querySelectorAll("nav a[data-page]").forEach((a) => a.classList.toggle("active", a.dataset.page === page));
  if (page === "jurnal") loadJournal();
  if (page === "servicii") loadServicesPage();
  if (page === "firewall") loadFirewallPage();
  if (page === "programe") loadPrograms();
  if (page === "pornire") loadStartup();
  if (page === "ajutor") loadHelp();
  if (page === "sarcini") loadTasks();
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
    $("packs").innerHTML = `<div class="map">${r.packs
      .map((p) => {
        const cls = { valid: "ok", invalid: "bad", broken: "bad", schema_too_new: "warn", older: "muted", none: "muted" }[p.update_state] || "";
        return `<div class="svc"><div><b>${esc(t(`rp.name.${p.name}`))}</b> <span class="sub">v${esc(p.active_version)}</span></div>
          <div class="sub">${esc(p.source === "updated" ? t("rp.updated") : t("rp.builtin", { v: p.builtin_version }))}</div>
          ${p.update_state !== "none" ? `<div class="sub"><span class="badge ${cls}">${esc(t(`rp.state.${p.update_state}`, { v: p.update_version || "" }))}</span></div>` : ""}
        </div>`;
      })
      .join("")}</div>`;

    const ops = await invoke("journal_list");
    const b = ops.find((x) => x.kind === "backup" && x.status === "backup_done");
    $("lastbackup").textContent = b ? when(b.created) : t("panou.none");
    const rpFailed = b && /necreat|not created/.test(b.restore_point || "");
    $("lastbackupsub").textContent = b
      ? t("panou.backupSub", { n: b.files.length, rp: rpLabel(b.restore_point) }) + (rpFailed ? ` · ${t("panou.backupRedo")}` : "")
      : t("panou.backupHint");

    $("mode").textContent = (await invoke("is_admin")) ? t("mode.admin") : t("mode.user");
  } catch (e) {
    $("win").textContent = t("common.error");
    $("winver").textContent = String(e);
  }
}

// ---------- jurnal ----------
const setStatus = statusBox("status");

function opCard(op) {
  const cls = statusClass[op.status] || "";
  // „În curs” de peste 5 minute = întreruptă (ex. repornire în timpul aplicării): se poate readuce.
  const interrupted = op.status === "in_progress" && Date.now() / 1000 - op.created > 300;
  const canUndo = op.kind === "change" && (op.status === "applied" || op.status === "partially_undone" || interrupted);
  const label = interrupted ? t("status.interrupted") : statusLabel(op.status);
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
    const featName = (k) => a.features[k] || k;
    $("svc-proposals").innerHTML = a.proposals.length
      ? `<div class="op"><p class="sub legend">${esc(t("svc.legend"))}</p><ul class="finds">${a.proposals
          .map((p) => {
            const [rl, rc] = risk(p.risk);
            return `<li><label><input type="checkbox" data-svc="${esc(p.service)}" ${p.risk === "low" ? "checked" : ""}>
              <span><b>${esc(p.display)}</b> <span class="badge ${rc}">${esc(rl)}</span>${p.running ? ` <span class="tag">${esc(t("svc.runningNow"))}</span>` : ""}${infoBtn("service", p.service)}
              <div class="sub">${esc(startLabel(p.current))} → <b>${esc(startLabel(p.proposed))}</b> · ${esc(p.service)}</div>
              <div class="sub">${esc(p.why)}</div>
              ${p.needed_for.length ? `<div class="sub needline">${esc(t("svc.neededFor", { l: p.needed_for.map(featName).join("; ") }))}</div>` : ""}
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
        <div><b>${esc(s.display)}</b> <span class="sub">${esc(s.name)}</span>${s.critical ? ` <span class="tag">${esc(t("svc.vital"))}</span>` : ""}${infoBtn("service", s.name)}</div>
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
/** ⓘ pentru regulile care blochează un program (ex. out-mshta → mshta.exe). */
function fwInfo(p) {
  const prog = p.action && p.action.program;
  if (!prog) return "";
  const file = prog.split("\\").pop();
  return infoBtn("program", file, prog);
}
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
              <span><b>${esc(p.title)}</b> <span class="badge ${rc}">${esc(rl)}</span>${fwInfo(p)}
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
          <div><b>${esc(p.name || "?")}</b>${p.new ? ` <span class="badge warn">${esc(t("fw.new"))}</span>` : ""}${p.blocked ? ` <span class="tag">${esc(t("fw.blockedBy"))}</span>` : ""}${p.name ? infoBtn("program", p.name, p.path || "") : ""}</div>
          <div class="sub">${esc(p.path || t("fw.hiddenPath"))}</div>
          <div class="sub">${esc(t("fw.connsN", { n: p.connections }))} ${p.remotes
            .map((r) => {
              const ip = r.slice(0, r.lastIndexOf(":"));
              return `<button class="ipchip" data-ip="${esc(ip)}" title="${esc(t("ip.click"))}">${esc(r)}</button>`;
            })
            .join(" ")}</div>
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
async function openIp(ip) {
  $("ex-name").textContent = ip;
  $("ex-verdict").textContent = "";
  $("ex-verdict").className = "badge";
  $("ex-body").innerHTML = `<p class="sub">${esc(t("ex.loading"))}</p>`;
  $("explain").classList.remove("hidden");
  try {
    const i = await invoke("ip_info", { ip });
    $("ex-verdict").textContent = t(`ip.scope.${i.scope}`);
    $("ex-verdict").className = `badge ${i.scope === "public" ? "muted" : "ok"}`;
    const sec = (title, body) => (body ? `<div class="exsec"><b>${esc(title)}</b>${esc(body)}</div>` : "");
    $("ex-body").innerHTML =
      sec(t("ip.owner"), i.owner || t("ip.ownerUnknown")) +
      sec(t("ip.reverse"), i.reverse_name || t("ip.noReverse")) +
      `<p class="sub">${esc(t("ip.hint"))}</p>` +
      (i.scope === "public" ? `<div class="op-actions"><button class="ghost" id="ip-web">${esc(t("ip.web"))}</button></div><p class="sub">${esc(t("ip.webHint"))}</p>` : "");
    $("ip-web")?.addEventListener("click", () => invoke("ip_web", { ip: i.ip }).catch(() => {}));
  } catch (err) {
    $("ex-body").textContent = String(err);
  }
}

$("fw-activity").addEventListener("click", async (e) => {
  const chip = e.target.closest("button.ipchip");
  if (chip) return openIp(chip.dataset.ip);
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

// ---------- ajutor ----------
async function loadHelp() {
  const help = window.PIOCARE_HELP || {};
  $("help-body").innerHTML = help[LANG] || help.ro || ""; // conținut static, scris de noi (fără date din afară)
  let v = "";
  try {
    v = await window.__TAURI__.app.getVersion();
  } catch {}
  $("about").textContent = t("help.about", { v });
}

// ---------- puncte de restaurare ----------
const gb = (b) => `${fmt(b / 1073741824, 1)} GB`;
async function showRestorePoints() {
  $("rp-info").innerHTML = `<p class="sub">${esc(t("rpx.loading"))}</p>`;
  try {
    const r = await invoke("restore_points");
    if (r.error) throw r.error;
    const pct = r.max_bytes && r.disk_bytes ? Math.round((r.max_bytes / r.disk_bytes) * 100) : null;
    const last = r.points.slice(-5).reverse();
    $("rp-info").innerHTML = `
      <div class="exsec"><b>${esc(t("rpx.count", { n: r.points.length }))}</b>${last.map((p) => `${esc(p.time)} · ${esc(p.description)}`).join("<br>") || esc(t("rpx.none"))}</div>
      <div class="exsec"><b>${esc(t("rpx.space"))}</b>${esc(t("rpx.used", { u: gb(r.used_bytes), m: r.max_bytes ? gb(r.max_bytes) + (pct != null ? ` (${pct}%)` : "") : t("rpx.unlimited") }))}</div>
      <div class="exsec"><b>${esc(t("rpx.limit"))}</b>${esc(t("rpx.limitHint"))}</div>
      <div class="op-actions">${[5, 8, 10].map((g) => `<button class="${g === 8 ? "" : "ghost"}" data-rpgb="${g}">${esc(t("rpx.set", { n: g }))}${g === 8 ? " ★" : ""}</button>`).join("")}</div>`;
  } catch (e) {
    $("rp-info").innerHTML = `<p class="sub">${esc(String(e))}</p>`;
  }
}
$("btn-rp-show").addEventListener("click", showRestorePoints);
$("rp-info").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-rpgb]");
  if (!b) return;
  await runAction("restore_limit", { gb: Number(b.dataset.rpgb) }, "rpx.working");
  showRestorePoints();
});

// ---------- monitor ----------
let monTimer = null;
let monBusy = false;
const hist = { cpu: [], ram: [] };
const HIST = 40; // ~1 minut la 1,5 s
const monStatus = statusBox("mon-status");

function setRing(id, pct) {
  const el = $(id);
  const p = Math.max(0, Math.min(100, pct || 0));
  el.style.strokeDashoffset = String(314.16 * (1 - p / 100));
  el.classList.toggle("warn", p >= 75 && p < 90);
  el.classList.toggle("bad", p >= 90);
}

function drawChart() {
  const pts = (arr) => arr.map((v, i) => `${(i / (HIST - 1)) * 600},${140 - (v / 100) * 132 - 4}`).join(" ");
  $("line-cpu").setAttribute("points", pts(hist.cpu));
  $("line-ram").setAttribute("points", pts(hist.ram));
}

async function tickMonitor() {
  if (monBusy || !invoke) return;
  monBusy = true;
  try {
    const by = document.querySelector("input[name=monsort]:checked").value;
    const s = await invoke("monitor_sample", { by });
    const ramPct = s.ram_total_gb ? (s.ram_used_gb / s.ram_total_gb) * 100 : 0;
    $("mon-cpuname").textContent = `${s.cpu_name} · ${t("mon.cores", { n: s.cores })}`;
    setRing("ring-cpu", s.cpu_pct);
    $("val-cpu").textContent = `${Math.round(s.cpu_pct)}%`;
    $("sub-cpu").textContent = s.cpu_mhz ? `${fmt(s.cpu_mhz / 1000, 2)} GHz` : "";
    setRing("ring-ram", ramPct);
    $("val-ram").textContent = `${Math.round(ramPct)}%`;
    $("sub-ram").textContent = `${fmt(s.ram_used_gb)} / ${fmt(s.ram_total_gb)} GB`;
    if (s.gpu && s.gpu.temp_c != null) {
      setRing("ring-gpu", s.gpu.temp_c);
      $("val-gpu").textContent = `${Math.round(s.gpu.temp_c)}°C`;
      $("sub-gpu").textContent = s.gpu.load_pct != null ? t("mon.gpuLoad", { n: Math.round(s.gpu.load_pct) }) : "";
      $("gpu-note").textContent = s.gpu.name;
    } else {
      setRing("ring-gpu", 0);
      $("val-gpu").textContent = "–";
      $("sub-gpu").textContent = "";
      $("gpu-note").textContent = t("mon.gpuNone");
    }
    $("val-procs").textContent = s.process_count;
    $("sub-swap").textContent = t("mon.swap", { n: fmt(s.swap_used_gb) });
    hist.cpu.push(s.cpu_pct);
    hist.ram.push(ramPct);
    while (hist.cpu.length > HIST) hist.cpu.shift(), hist.ram.shift();
    drawChart();
    const maxRam = Math.max(...s.processes.map((p) => p.ram_mb), 1);
    $("mon-procs").innerHTML = s.processes
      .map(
        (p) => `<tr>
        <td><b>${esc(p.name)}</b> ${infoBtn("program", p.name, p.path || "")}<div class="bar"><div style="width:${(p.ram_mb / maxRam) * 100}%"></div></div></td>
        <td class="num">${esc(p.ram_mb >= 1024 ? fmt(p.ram_mb / 1024, 2) + " GB" : fmt(p.ram_mb, 0) + " MB")}</td>
        <td class="num">${esc(fmt(p.cpu_pct, 1))}%</td>
        <td>${p.protected ? `<span class="tag">${esc(t("mon.protected"))}</span>` : `<button class="ghost" data-kill="${p.pid}" data-name="${esc(p.name)}">${esc(t("mon.kill"))}</button>`}</td>
      </tr>`
      )
      .join("");
  } catch (e) {
    monStatus(String(e), "bad");
  } finally {
    monBusy = false;
  }
}

function startMonitor() {
  tickMonitor();
  monTimer = setInterval(tickMonitor, 1500);
}
function stopMonitor() {
  if (monTimer) clearInterval(monTimer);
  monTimer = null;
}

$("mon-sort").addEventListener("change", tickMonitor);
$("mon-procs").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-kill]");
  if (!b) return;
  // Confirmare în doi pași: primul clic întreabă, al doilea (în 4 s) închide.
  if (!b.dataset.armed) {
    b.dataset.armed = "1";
    b.textContent = t("mon.killSure");
    b.classList.remove("ghost");
    setTimeout(() => {
      if (b.isConnected) {
        delete b.dataset.armed;
        b.textContent = t("mon.kill");
        b.classList.add("ghost");
      }
    }, 4000);
    return;
  }
  try {
    await invoke("monitor_kill", { pid: Number(b.dataset.kill), name: b.dataset.name });
    monStatus(t("mon.killed", { n: b.dataset.name }), "ok");
  } catch (err) {
    monStatus(String(err), "bad");
  }
  tickMonitor();
});

// ---------- sarcini programate ----------
const tkStatus = statusBox("tk-status");

function taskCard(k) {
  const badges = [
    `<span class="badge ${k.enabled ? "ok" : "muted"}">${esc(k.enabled ? t("st.on") : t("st.off"))}</span>`,
    k.missing ? `<span class="badge bad">${esc(t("tk.missing"))}</span>` : "",
    k.looks_like_ad ? `<span class="badge warn">${esc(t("tk.ad"))}</span>` : "",
    k.user_created ? `<span class="tag">${esc(t("tk.yours"))}</span>` : "",
  ].join(" ");
  const when = [k.last_run && t("tk.last", { d: k.last_run }), k.next_run && t("tk.next", { d: k.next_run })].filter(Boolean).join(" · ");
  return `<div class="svc ${k.enabled ? "" : "dim"}">
    <div><b>${esc(k.name)}</b> ${badges}${k.file_name ? infoBtn("program", k.file_name, k.file || "") : ""}</div>
    <div class="sub">${esc([k.publisher || t("st.noPublisher"), k.path, k.triggers].filter(Boolean).join(" · "))}</div>
    ${when ? `<div class="sub">${esc(when)}</div>` : ""}
    <div class="sub">${esc(k.command)}</div>
    ${k.user_created ? `<div class="sub">${esc(t("tk.yoursHint"))}</div>` : ""}
    ${k.microsoft ? "" : `<div class="op-actions">
      <button class="ghost" data-tk="${esc(k.id)}" data-mode="${k.enabled ? "disable" : "enable"}">${esc(k.enabled ? t("tk.disable") : t("tk.enable"))}</button>
      ${k.can_delete ? `<button class="ghost" data-tk="${esc(k.id)}" data-mode="delete">${esc(t("tk.delete"))}</button>` : ""}
    </div>`}
  </div>`;
}

async function loadTasks() {
  $("tk-list").innerHTML = `<p class="empty">${esc(t("tk.loading"))}</p>`;
  try {
    const all = await invoke("tasks_list");
    const mine = all.filter((k) => !k.microsoft);
    const ms = all.filter((k) => k.microsoft);
    const flagged = mine.filter((k) => k.missing || k.looks_like_ad).length;
    tkStatus(t("tk.summary", { n: mine.length, f: flagged, m: ms.length }), flagged ? "warn" : "ok");
    $("tk-list").innerHTML =
      `<div class="map">${mine.map(taskCard).join("")}</div>` +
      `<details class="ms"><summary>${esc(t("tk.msTitle", { n: ms.length }))}</summary><p class="sub">${esc(t("tk.msLead"))}</p><div class="map">${ms.map(taskCard).join("")}</div></details>`;
  } catch (e) {
    tkStatus(String(e), "bad");
  }
}

$("btn-tk-refresh").addEventListener("click", loadTasks);
$("tk-list").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-tk]");
  if (!b) return;
  b.disabled = true;
  tkStatus(t("tk.working"), "warn");
  try {
    const op = await invoke("tasks_apply", { ids: [b.dataset.tk], mode: b.dataset.mode });
    await loadTasks();
    tkStatus(`${t("j.result", { title: op.title, label: statusLabel(op.status) })} ${t("common.undoHint")}`, op.status === "applied" ? "ok" : "bad");
  } catch (err) {
    tkStatus(String(err), "bad");
    b.disabled = false;
  }
});

// ---------- programe la pornire ----------
const stStatus = statusBox("st-status");

async function loadStartup() {
  $("st-list").innerHTML = `<p class="empty">${esc(t("st.loading"))}</p>`;
  try {
    const items = await invoke("startup_list");
    const on = items.filter((i) => i.enabled).length;
    stStatus(t("st.summary", { n: items.length, on }), on > 8 ? "warn" : "ok");
    $("st-list").innerHTML = `<div class="map">${items
      .map(
        (i) => `<div class="svc ${i.enabled ? "" : "dim"}">
        <div><b>${esc(i.name)}</b> <span class="badge ${i.enabled ? "ok" : "muted"}">${esc(i.enabled ? t("st.on") : t("st.off"))}</span>${i.file_name ? infoBtn("program", i.file_name, i.path || "") : ""}</div>
        <div class="sub">${esc([i.publisher || t("st.noPublisher"), t(`st.src.${i.source}`)].join(" · "))}</div>
        <div class="sub">${esc(i.command)}</div>
        ${i.missing ? `<div class="sub warnline">${esc(t("st.missing"))}</div>` : ""}
        ${i.protected ? `<div class="sub">${esc(t("st.protected"))}</div>` : ""}
        <div class="op-actions">
          ${i.protected && i.enabled ? "" : `<button class="ghost" data-st="${esc(i.id)}" data-en="${i.enabled ? "0" : "1"}">${esc(i.enabled ? t("st.disable") : t("st.enable"))}</button>`}
          ${i.needs_admin ? `<span class="tag">${esc(t("common.admin"))}</span>` : ""}
        </div>
      </div>`
      )
      .join("")}</div>`;
  } catch (e) {
    stStatus(String(e), "bad");
  }
}

$("btn-st-refresh").addEventListener("click", loadStartup);
$("st-list").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-st]");
  if (!b) return;
  b.disabled = true;
  stStatus(t("st.working"), "warn");
  try {
    const op = await invoke("startup_set", { ids: [b.dataset.st], enable: b.dataset.en === "1" });
    await loadStartup();
    stStatus(`${t("j.result", { title: op.title, label: statusLabel(op.status) })} ${t("st.afterRestart")} ${t("common.undoHint")}`, op.status === "applied" ? "ok" : "bad");
  } catch (err) {
    stStatus(String(err), "bad");
    b.disabled = false;
  }
});

// ---------- foldere rămase ----------
const orStatus = statusBox("or-status");
function updateOrClean() {
  const boxes = [...document.querySelectorAll("#or-list input:checked")];
  const total = boxes.reduce((a, c) => a + Number(c.dataset.bytes || 0), 0);
  $("btn-or-clean").classList.toggle("hidden", boxes.length === 0);
  $("btn-or-clean").textContent = t("or.cleanN", { n: boxes.length, size: mb(total) });
}
async function scanOrphans() {
  $("btn-or-scan").disabled = true;
  orStatus(t("or.scanning"), "warn");
  try {
    const list = await invoke("orphans_scan");
    const total = list.reduce((a, o) => a + o.size_bytes, 0);
    orStatus(list.length ? t("or.found", { n: list.length, size: mb(total) }) : t("or.none"), list.length ? "warn" : "ok");
    $("or-list").innerHTML = list.length
      ? `<ul class="finds">${list
          .map(
            (o) => `<li><label><input type="checkbox" checked data-or="${esc(o.id)}" data-bytes="${o.size_bytes}">
          <span><b>${esc(o.name)}</b> <span class="sub">${esc(mb(o.size_bytes))}</span>${o.needs_admin ? ` <span class="tag">${esc(t("common.admin"))}</span>` : ""}
          <div class="sub">${esc(o.path)}</div>
          <div class="sub">${esc(t("or.age", { n: o.days_unused }))}</div></span></label></li>`
          )
          .join("")}</ul>`
      : "";
    updateOrClean();
  } catch (e) {
    orStatus(String(e), "bad");
  } finally {
    $("btn-or-scan").disabled = false;
  }
}
$("btn-or-scan").addEventListener("click", scanOrphans);
$("or-list").addEventListener("change", updateOrClean);
$("btn-or-clean").addEventListener("click", async () => {
  const ids = [...document.querySelectorAll("#or-list input:checked")].map((c) => c.dataset.or);
  if (!ids.length) return;
  $("btn-or-clean").disabled = true;
  orStatus(t("or.cleaning"), "warn");
  try {
    const op = await invoke("orphans_clean", { ids });
    await scanOrphans();
    orStatus(`${t("j.result", { title: op.title, label: statusLabel(op.status) })} ${t("common.undoHint")}`, op.status === "applied" ? "ok" : "bad");
  } catch (e) {
    orStatus(String(e), "bad");
  } finally {
    $("btn-or-clean").disabled = false;
  }
});

// ---------- dezinstalare ----------
let programs = [];
let current = null; // programul în lucru (informațiile lui rămân după ce dispare din registru)
let countBefore = 0;
const unStatus = statusBox("un-status");
const fmtDate = (d) => (/^\d{8}$/.test(d) ? `${d.slice(6, 8)}.${d.slice(4, 6)}.${d.slice(0, 4)}` : "");
const KIND = (k) => t(`un.kind.${k}`);

function renderPrograms() {
  const q = $("un-filter").value.trim().toLowerCase();
  const rows = programs.filter((p) => !q || p.name.toLowerCase().includes(q) || p.publisher.toLowerCase().includes(q));
  $("un-list").innerHTML = `<p class="sub">${esc(t("un.count", { n: programs.length }))}</p><div class="map">${rows
    .map(
      (p) => `<div class="svc">
        <div><b>${esc(p.name)}</b>${p.shared ? ` <span class="badge warn">${esc(t("un.shared"))}</span>` : ""}${infoBtn("program", p.name, "")}</div>
        <div class="sub">${esc([p.publisher, p.version].filter(Boolean).join(" · "))}</div>
        <div class="sub">${esc([p.size_kb ? mb(p.size_kb * 1024) : "", fmtDate(p.install_date), p.machine ? t("un.allUsers") : t("un.thisUser")].filter(Boolean).join(" · "))}</div>
        <div class="op-actions"><button class="ghost" data-un="${esc(p.id)}">${esc(t("un.uninstall"))}</button></div>
      </div>`
    )
    .join("")}</div>`;
}

async function loadPrograms() {
  try {
    programs = await invoke("programs_list");
    renderPrograms();
  } catch (e) {
    $("un-list").textContent = String(e);
  }
}

async function scanLeftovers() {
  unStatus(t("un.scanning"), "warn");
  const r = await invoke("program_leftovers", { program: current });
  if (r.still_installed) {
    unStatus(t("un.stillInstalled"), "warn");
    $("un-badge").textContent = t("un.notRemoved");
    $("un-badge").className = "badge warn";
    return;
  }
  $("un-badge").textContent = t("un.removed");
  $("un-badge").className = "badge ok";
  // Lista de jos se actualizează imediat (programul nu mai e instalat).
  programs = await invoke("programs_list");
  renderPrograms();
  if (!r.leftovers.length) {
    unStatus(t("un.noLeftovers"), "ok");
    await showAfter(null);
    return;
  }
  const total = r.leftovers.reduce((a, l) => a + l.size_bytes, 0);
  unStatus(t("un.found", { n: r.leftovers.length, size: mb(total) }), "warn");
  $("un-left").innerHTML = `<p class="sub legend">${esc(t("un.legend"))}</p><ul class="finds">${r.leftovers
    .map(
      (l) => `<li><label><input type="checkbox" checked data-left="${esc(l.id)}">
        <span><b>${esc(KIND(l.kind))}</b>${l.needs_admin ? ` <span class="tag">${esc(t("common.admin"))}</span>` : ""}${l.size_bytes ? ` <span class="sub">${esc(mb(l.size_bytes))}</span>` : ""}
        <div class="sub">${esc(l.location)}</div></span></label></li>`
    )
    .join("")}</ul>`;
  $("btn-un-clean").classList.remove("hidden");
}

async function showAfter(op) {
  const before = countBefore;
  const now = await invoke("programs_list");
  let dead = "?";
  try {
    dead = (await invoke("registry_scan")).findings.length;
  } catch {}
  const lines = [
    t("un.after.programs", { a: before, b: now.length }),
    op ? t("un.after.cleaned", { n: op.step_texts.filter((s) => s.state === "applied").length }) : "",
    t("un.after.registry", { n: dead }),
    op ? t("common.undoHint") : "",
  ].filter(Boolean);
  $("un-after").innerHTML = `<div class="exsec"><b>${esc(t("un.after.title"))}</b>${lines.map(esc).join("<br>")}</div>`;
  programs = now;
  renderPrograms();
}

$("un-filter").addEventListener("input", renderPrograms);
$("btn-un-refresh").addEventListener("click", loadPrograms);
$("btn-un-close").addEventListener("click", () => {
  $("un-flow").classList.add("hidden");
  current = null;
});
$("un-list").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-un]");
  if (!b) return;
  current = programs.find((p) => p.id === b.dataset.un);
  if (!current) return;
  countBefore = programs.length;
  $("un-flow").classList.remove("hidden");
  $("un-name").textContent = current.name;
  $("un-sub").textContent = [current.publisher, current.version].filter(Boolean).join(" · ");
  $("un-badge").textContent = "";
  $("un-left").innerHTML = "";
  $("un-after").innerHTML = "";
  $("btn-un-clean").classList.add("hidden");
  window.scrollTo(0, 0);
  if (current.shared) unStatus(t("un.sharedWarn"), "warn");
  document.querySelectorAll("#un-list button").forEach((x) => (x.disabled = true));
  unStatus(t("un.running"), "warn");
  try {
    await invoke("program_uninstall", { id: current.id });
    // Unele dezinstalatoare (ex. Inno Setup) se copiază și continuă în alt proces: așteptăm să dispară din registru.
    unStatus(t("un.waiting"), "warn");
    for (let i = 0; i < 90 && (await invoke("program_exists", { id: current.id })); i++) {
      await new Promise((r) => setTimeout(r, 2000));
    }
    await scanLeftovers();
  } catch (err) {
    unStatus(String(err), "bad");
  } finally {
    document.querySelectorAll("#un-list button").forEach((x) => (x.disabled = false));
  }
});
$("btn-un-clean").addEventListener("click", async () => {
  const ids = [...document.querySelectorAll("#un-left input:checked")].map((c) => c.dataset.left);
  if (!ids.length || !current) return;
  $("btn-un-clean").disabled = true;
  unStatus(t("un.cleaning"), "warn");
  try {
    const op = await invoke("program_clean", { program: current, ids });
    unStatus(t("j.result", { title: op.title, label: statusLabel(op.status) }), op.status === "applied" ? "ok" : "bad");
    $("btn-un-clean").classList.add("hidden");
    $("un-left").innerHTML = "";
    await showAfter(op);
  } catch (err) {
    unStatus(String(err), "bad");
  } finally {
    $("btn-un-clean").disabled = false;
  }
});

$("btn-rules-folder").addEventListener("click", () => invoke("open_rules_folder").catch(() => {}));
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
  try {
    const v = await window.__TAURI__.app.getVersion();
    $("brand-ver").textContent = v;
    document.title = `PioCare ${v}`;
  } catch {}
  if (invoke && !info.lang) {
    // Prima pornire: întrebăm limba; butonul sugerat (limba Windows) e primul în focus.
    $("lang-pick").classList.remove("hidden");
    document.querySelector(`#lang-pick button[data-lang="${LANG}"]`)?.focus();
  }
  loadOverview();
})();
