// Loope Desktop — vanilla front-end over the Tauri global API (no bundler).
"use strict";

const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : async () => { throw new Error("Tauri unavailable (browser preview)"); };

const AGENT_ICONS = {
  claude:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M12 3v18M3 12h18M5.6 5.6l12.8 12.8M18.4 5.6 5.6 18.4"/></svg>',
  codex:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9"><path d="M12 2.5 20 7v10l-8 4.5L4 17V7z"/><circle cx="12" cy="12" r="3.1"/></svg>',
  opencode:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M9 7 4 12l5 5M15 7l5 5-5 5"/></svg>',
};
const AGENT_COLORS = { claude: "#5ba8ff", codex: "#ff9f45", opencode: "#c08cff" };
const GUT = { run: "#7fb4ff", diff: "#43e08f", reasoning: "#c08cff", action: "#5ba8ff", notice: "#ff9f45", markdown: "#7fb4ff" };

const $ = (id) => document.getElementById(id);
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text != null) e.textContent = text;
  return e;
};

let state = { projects: [], activeProject: null, activeSession: null };
let live = false;
const DEFAULT_OPTIONS = {
  max_iters: 3,
  implementer: "claude",
  reviewers: ["codex"],
  designer: "claude",
  include_design: false,
  verify_command: null,
  dry_run: false,
};
function loadStored(key, fallback) {
  try { const v = JSON.parse(localStorage.getItem(key)); return v == null ? fallback : v; }
  catch (e) { return fallback; }
}
const OPTIONS = Object.assign({}, DEFAULT_OPTIONS, loadStored("loope.options", {}));
function saveOptions() { localStorage.setItem("loope.options", JSON.stringify(OPTIONS)); }

// ---------------------------------------------------------------- agents
async function loadAgents() {
  const box = $("agents");
  box.textContent = "";
  let agents = [];
  try {
    agents = await invoke("list_agents");
  } catch (e) {
    return;
  }
  for (const a of agents) {
    const chip = el("span", "chip" + (a.available ? "" : " off"));
    chip.style.setProperty("--c", AGENT_COLORS[a.id] || "#9bb");
    if (AGENT_ICONS[a.id]) {
      const icon = el("span");
      icon.innerHTML = AGENT_ICONS[a.id];
      chip.appendChild(icon.firstChild);
    }
    chip.appendChild(document.createTextNode(a.name));
    if (a.available) {
      chip.appendChild(el("i", "ok", "✓"));
      if (a.version) chip.appendChild(el("span", "ver", a.version.replace(/[^0-9.].*$/, "")));
    } else {
      chip.appendChild(el("i", "miss", "✗"));
      chip.title = "missing — " + a.install_hint;
    }
    box.appendChild(chip);
  }
}

// ---------------------------------------------------------------- projects/sessions
async function loadProjects() {
  try {
    state.projects = await invoke("list_projects");
  } catch (e) {
    state.projects = [];
  }
  if (!state.activeProject && state.projects.length) state.activeProject = state.projects[0];
  renderSidebar();
  renderContext();
  if (state.activeProject && state.activeProject.sessions.length) {
    selectSession(state.activeProject.sessions[0]);
  }
}

function renderContext() {
  const ctx = $("ctx");
  ctx.textContent = "";
  if (state.activeProject) {
    ctx.appendChild(document.createTextNode("📁 " + state.activeProject.path));
  }
}

function renderSidebar() {
  const bar = $("sidebar");
  bar.textContent = "";
  const active = state.activeProject;
  bar.appendChild(el("div", "side-h", "RUNS"));
  if (active) {
    for (const s of active.sessions) {
      const row = el("div", "run" + (state.activeSession && state.activeSession.id === s.id ? " sel" : ""));
      row.appendChild(el("span", "dot " + (s.converged ? "ok" : "bad")));
      row.appendChild(document.createTextNode(s.name || s.id));
      row.appendChild(el("small", null, s.stop_reason + " · " + s.iterations));
      row.onclick = () => selectSession(s);
      row.ondblclick = () => renameSession(s);
      bar.appendChild(row);
    }
    if (!active.sessions.length) bar.appendChild(el("div", "proj", "no runs yet"));
  }
  const ph = el("div", "side-h sub");
  ph.appendChild(document.createTextNode("PROJECTS"));
  const add = el("span", "addbtn", "+");
  add.title = "register a project";
  add.onclick = addProject;
  ph.appendChild(add);
  bar.appendChild(ph);
  for (const p of state.projects) {
    const row = el("div", "proj");
    row.appendChild(document.createTextNode("▸ " + p.name));
    row.appendChild(el("small", null, String(p.run_count)));
    row.onclick = () => {
      state.activeProject = p;
      state.activeSession = null;
      renderSidebar();
      renderContext();
      if (p.sessions.length) selectSession(p.sessions[0]);
      else { $("stream").textContent = ""; $("stream").appendChild(el("div", "empty", "no runs yet")); $("pipe").textContent = ""; }
    };
    bar.appendChild(row);
  }
}

async function selectSession(s) {
  state.activeSession = s;
  renderSidebar();
  let run;
  try {
    run = await invoke("read_run", { runDir: s.dir });
  } catch (e) {
    return;
  }
  renderPipeline(run, s);
  renderTranscript(run, s);
}

// ---------------------------------------------------------------- pipeline
function renderPipeline(run, s) {
  const pipe = $("pipe");
  pipe.textContent = "";
  const roles = ["implement", "review", "verify"];
  roles.forEach((r, i) => {
    if (i) pipe.appendChild(el("span", "pe"));
    pipe.appendChild(el("span", "pn done", r));
  });
  pipe.appendChild(el("span", "pe"));
  pipe.appendChild(el("span", "pn goal", s.converged ? "converged" : "stopped"));
  pipe.appendChild(el("span", "iter", "iter " + run.iterations));
}

// ---------------------------------------------------------------- transcript
function renderTranscript(run, s) {
  const stream = $("stream");
  stream.textContent = "";

  if (s.has_highlight) {
    const hero = el("div", "hero-row");
    hero.appendChild(el("span", "hs", "✦"));
    hero.appendChild(el("span", null, "caught & fixed — a reviewer blocked a defect and it was fixed in a later iteration"));
    stream.appendChild(hero);
  }

  for (const step of run.steps) {
    const sep = el("div", "sep");
    sep.appendChild(document.createTextNode((step.role || "step") + " · " + (step.adapter || "")));
    stream.appendChild(sep);
    for (const c of step.cells) stream.appendChild(renderCell(c));
  }
  if (!run.steps.length) stream.appendChild(el("div", "empty", "this run has no recorded steps"));
}

function renderCell(c) {
  const row = el("div", "row");
  row.style.setProperty("--g", GUT[c.kind] || "rgba(255,255,255,.2)");
  row.appendChild(el("div", "gut"));
  const rc = el("div", "rc");
  const head = el("div", "rh");

  if (c.kind === "exec") {
    head.appendChild(el("span", "tag run", "run"));
    head.appendChild(codeEl(c.command));
    rc.appendChild(head);
    if (c.output && c.output.trim()) rc.appendChild(el("pre", "out", c.output));
  } else if (c.kind === "diff") {
    head.appendChild(el("span", "tag diff", "diff"));
    head.appendChild(codeEl(c.file));
    rc.appendChild(head);
    rc.appendChild(diffEl(c.hunks));
  } else if (c.kind === "markdown") {
    head.appendChild(el("span", "tag", "message"));
    rc.appendChild(head);
    rc.appendChild(el("div", "md", c.text));
  } else if (c.kind === "reasoning") {
    head.appendChild(el("span", "tag reasoning", "reasoning"));
    const fold = el("span", "fold", "folded — click to expand");
    head.appendChild(fold);
    rc.appendChild(head);
    const body = el("div", "md", c.text);
    body.style.display = "none";
    fold.onclick = () => {
      body.style.display = body.style.display === "none" ? "block" : "none";
      fold.textContent = body.style.display === "none" ? "folded — click to expand" : "reasoning";
    };
    rc.appendChild(body);
  } else if (c.kind === "action") {
    head.appendChild(el("span", "tag", c.action));
    head.appendChild(codeEl(c.target));
    rc.appendChild(head);
  } else if (c.kind === "notice") {
    head.appendChild(el("span", "tag notice", c.level));
    head.appendChild(document.createTextNode(c.text));
    rc.appendChild(head);
  }
  row.appendChild(rc);
  return row;
}

function codeEl(text) {
  const c = el("code");
  c.textContent = text || "";
  return c;
}

function diffEl(hunks) {
  const pre = el("pre", "out");
  for (const h of hunks || []) {
    pre.appendChild(spanLine("hdr", h.header));
    for (const line of h.lines) {
      const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : null;
      pre.appendChild(spanLine(cls, line));
    }
  }
  return pre;
}
function spanLine(cls, text) {
  const s = el("span", cls, text + "\n");
  return s;
}

// ---------------------------------------------------------------- search + command bar
async function runSearch(query) {
  const stream = $("stream");
  stream.textContent = "";
  stream.appendChild(el("div", "sep", "search: " + query));
  let hits = [];
  try {
    hits = await invoke("search_runs", { query });
  } catch (e) {}
  if (!hits.length) { stream.appendChild(el("div", "empty", "no matches")); return; }
  for (const h of hits) {
    const row = el("div", "row");
    row.style.setProperty("--g", "#7fb4ff");
    row.appendChild(el("div", "gut"));
    const rc = el("div", "rc");
    const head = el("div", "rh");
    head.appendChild(el("span", "tag", h.source));
    head.appendChild(codeEl(h.session_id));
    head.appendChild(el("span", "meta", "line " + h.line));
    rc.appendChild(head);
    rc.appendChild(el("div", "md", h.preview));
    row.appendChild(rc);
    stream.appendChild(row);
  }
}

// ---------------------------------------------------------------- rename / projects
async function renameSession(s) {
  const name = window.prompt("Name this run:", s.name || s.id);
  if (name == null) return;
  try { await invoke("set_session_name", { id: s.id, name }); await loadProjects(); }
  catch (e) { toast("rename failed"); }
}

async function addProject() {
  const path = window.prompt("Project path (a directory with .loope/runs):");
  if (!path) return;
  try { await invoke("add_project", { path }); await loadProjects(); toast("project registered"); }
  catch (e) { toast("could not register project"); }
}

// ---------------------------------------------------------------- settings + presets
function gearToggle() {
  const s = $("settings");
  if (s.classList.contains("hidden")) { renderSettings(); s.classList.remove("hidden"); }
  else { s.classList.add("hidden"); }
}
function presetsStore() { return loadStored("loope.presets", {}); }
function savePresets(p) { localStorage.setItem("loope.presets", JSON.stringify(p)); }

function fieldRow(label, input) { const f = el("div", "field"); f.appendChild(el("label", null, label)); f.appendChild(input); return f; }
function textField(label, val, on) { const i = el("input"); i.type = "text"; i.value = val; i.oninput = () => on(i.value); return fieldRow(label, i); }
function numField(label, val, on) { const i = el("input"); i.type = "number"; i.min = "1"; i.value = val; i.oninput = () => on(i.value); return fieldRow(label, i); }
function checkField(label, val, on) { const i = el("input"); i.type = "checkbox"; i.checked = val; i.onchange = () => on(i.checked); return fieldRow(label, i); }
function selectField(label, opts, val, on) {
  const sel = el("select");
  opts.forEach((o) => { const op = el("option", null, o); op.value = o; if (o === val) op.selected = true; sel.appendChild(op); });
  sel.onchange = () => on(sel.value);
  return fieldRow(label, sel);
}

function renderSettings() {
  const s = $("settings");
  s.textContent = "";
  s.appendChild(el("h3", null, "RUN SETTINGS"));
  s.appendChild(selectField("implementer", ["claude", "codex", "opencode"], OPTIONS.implementer, (v) => { OPTIONS.implementer = v; saveOptions(); }));
  s.appendChild(textField("reviewers", OPTIONS.reviewers.join(","), (v) => { OPTIONS.reviewers = v.split(",").map((x) => x.trim()).filter(Boolean); saveOptions(); }));
  s.appendChild(numField("max iters", OPTIONS.max_iters, (v) => { OPTIONS.max_iters = Math.max(1, parseInt(v) || 1); saveOptions(); }));
  s.appendChild(textField("verify cmd", OPTIONS.verify_command || "", (v) => { OPTIONS.verify_command = v.trim() || null; saveOptions(); }));
  s.appendChild(checkField("design step", OPTIONS.include_design, (v) => { OPTIONS.include_design = v; saveOptions(); }));
  s.appendChild(checkField("dry run (stub agents)", OPTIONS.dry_run, (v) => { OPTIONS.dry_run = v; saveOptions(); }));

  const box = el("div", "presets");
  box.appendChild(el("h3", null, "PRESETS"));
  const presets = presetsStore();
  for (const name of Object.keys(presets)) {
    const r = el("div", "preset");
    const nm = el("span", "nm", name);
    nm.onclick = () => { Object.assign(OPTIONS, presets[name]); saveOptions(); renderSettings(); toast("applied " + name); };
    const del = el("span", "del", "✕");
    del.onclick = () => { const p = presetsStore(); delete p[name]; savePresets(p); renderSettings(); };
    r.appendChild(nm); r.appendChild(del); box.appendChild(r);
  }
  const save = el("button", "btn", "save current as preset");
  save.onclick = () => { const name = window.prompt("Preset name:"); if (!name) return; const p = presetsStore(); p[name] = Object.assign({}, OPTIONS); savePresets(p); renderSettings(); };
  box.appendChild(save);
  s.appendChild(box);
}

function toast(msg) {
  let t = $("toast");
  if (!t) { t = el("div", "toast"); t.id = "toast"; document.body.appendChild(t); }
  t.textContent = msg;
  t.classList.add("show");
  clearTimeout(toast._t);
  toast._t = setTimeout(() => t.classList.remove("show"), 2600);
}

function wireCommandBar() {
  const gear = $("gear");
  if (gear) gear.onclick = gearToggle;
  const input = $("prompt");
  input.addEventListener("keydown", (e) => {
    if (e.key !== "Enter") return;
    const value = input.value.trim();
    if (!value) return;
    if (e.shiftKey) {
      runSearch(value);
    } else {
      startRun(value);
    }
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && live) {
      invoke("stop_run").catch(() => {});
      toast("stopping… (finishes the current step)");
    }
  });
}

// ---------------------------------------------------------------- live run
async function startRun(requirement) {
  if (!state.activeProject) { toast("Open a project first."); return; }
  $("prompt").value = "";
  $("stream").textContent = "";
  live = true;
  $("hint").textContent = "running… · Esc to stop";
  renderLivePipeline();
  try {
    await invoke("start_run", {
      projectPath: state.activeProject.path,
      requirement,
      options: OPTIONS,
    });
  } catch (e) {
    live = false;
    $("hint").textContent = "⏎ run · ⇧⏎ search";
    toast("Could not start: " + e);
  }
}

function renderLivePipeline() {
  const pipe = $("pipe");
  pipe.textContent = "";
  [["implement", "run"], ["review", ""], ["verify", ""]].forEach(([r, c], i) => {
    if (i) pipe.appendChild(el("span", "pe"));
    pipe.appendChild(el("span", "pn " + c, r));
  });
  pipe.appendChild(el("span", "pe"));
  pipe.appendChild(el("span", "pn goal", "converge"));
  pipe.appendChild(el("span", "iter", "iter 1"));
}

function streamAppend(node) {
  const s = $("stream");
  s.appendChild(node);
  s.scrollTop = s.scrollHeight;
}

function setupListeners() {
  if (!TAURI) return;
  const { listen } = TAURI.event;
  listen("loope://iteration", (e) => {
    const it = document.querySelector("#pipe .iter");
    if (it) it.textContent = "iter " + e.payload.n + " / " + e.payload.total;
  });
  listen("loope://step-start", (e) => {
    streamAppend(el("div", "sep", (e.payload.role || "step") + " · " + (e.payload.adapter || "")));
  });
  listen("loope://cell", (e) => streamAppend(renderCell(e.payload)));
  listen("loope://run-finished", (e) => onRunFinished(e.payload));
}

async function onRunFinished(p) {
  live = false;
  $("hint").textContent = "⏎ run · ⇧⏎ search";
  if (!p.ok) { toast("run failed: " + (p.error || "")); return; }
  toast(p.converged ? "converged ✓" : (p.stop_reason || "finished"));
  await loadProjects();
  try {
    const run = await invoke("read_run", { runDir: p.run_dir });
    const sess = { converged: p.converged, has_highlight: false, id: run.id };
    renderPipeline(run, sess);
    renderTranscript(run, sess);
  } catch (e) {}
}

// ---------------------------------------------------------------- boot
async function boot() {
  wireCommandBar();
  if (!TAURI) {
    $("empty").textContent = "Browser preview — agent/run data loads inside the desktop app.";
    return;
  }
  setupListeners();
  await loadAgents();
  await loadProjects();
}
boot();
