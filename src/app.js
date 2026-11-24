// PioCare — interfața. Folosește doar comenzi de citire din motor.
const invoke = window.__TAURI__ ? window.__TAURI__.core.invoke : null;
const $ = (id) => document.getElementById(id);
const fmt = (n, d = 1) => Number(n).toLocaleString("ro-RO", { maximumFractionDigits: d, minimumFractionDigits: d });

function uptimeText(h) {
  const days = Math.floor(h / 24);
  const hours = Math.floor(h % 24);
  return days > 0 ? `${days} z ${hours} h` : `${hours} h ${Math.floor((h * 60) % 60)} min`;
}

async function load() {
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
  } catch (e) {
    $("win").textContent = "eroare";
    $("winver").textContent = String(e);
  }
}

$("refresh").addEventListener("click", load);
load();
