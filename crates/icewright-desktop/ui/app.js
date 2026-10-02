"use strict";
// 与引擎 state.json 的 snake_case 线格式一致；状态枚举见 icewright-core/src/state.rs。
var invoke = window.__TAURI__.core.invoke;

// 界面文案中英双语，语种跟随 workspace.locale（经 ws_locale 命令），与引擎报错同一开关逻辑。
var DICT = {
  zh: {
    status: {
      pending: "待启动", running: "进行中", waiting_gate: "等闸门确认",
      approved: "已批准", failed: "失败", blocked_preflight: "预检受阻", skipped: "跳过",
    },
    gate: { approved: "批准", rejected: "驳回", partial_edit: "部分编辑驳回" },
    // 阶段中文名与引擎 StageId::title 对齐。
    stage: {
      S1: "需求摄入", S2: "环境预检", S3: "领域规则提取", S4: "设计文档与闸门A",
      S5: "代码生成", S6: "自动验证", S7: "验收交付与闸门B", S8: "变更/重生成",
    },
    ops: {
      init: "初始化", preflight: "环境预检", design_render: "渲染设计",
      design_approve: "批准闸门A", design_reject: "驳回闸门A", generate: "生成代码",
      verify: "自动验证", delivery_render: "渲染交付",
      delivery_approve: "批准闸门B", delivery_reject: "驳回闸门B",
    },
    ui: {
      workspaces: "工作区", hint_pre: "运行", hint_post: "创建新工作区",
      empty_pick: "选择左侧工作区查看进度与历史",
      th_stage: "阶段", th_name: "名称", th_status: "状态", th_hash: "产物哈希", th_gate: "闸门",
      history: "生成历史",
      note_placeholder: "驳回原因（驳回类操作必填）",
      not_init: "尚未初始化 pipeline：点下方「初始化」或运行 icewright pipeline init",
      stage_now: "当前阶段", updated: "更新于",
      arm: "再点一次确认",
      executing: "{0} 执行中…",
      refresh_fail: "刷新失败：{0}",
      menu_refreshed: "已刷新 · {0}",
      open_fail: "系统浏览器打开失败，请手动访问：{0}",
      no_history: "暂无历史记录",
      about_desc: "S1–S8 流水线构建器：把行业规则与流程交给 AI，产出可交付的客服系统。",
      about_github: "GitHub 主页",
      about_close: "关闭",
      settings: "设置",
      set_provider: "供应商",
      set_base: "接入点 Base URL",
      set_model: "模型名",
      set_discover: "在线发现",
      set_key: "密钥引用",
      set_key_hint: "只存引用不存密钥：env://变量名 / keyring://服务/账号 / plain:明文（不推荐）",
      set_save: "保存",
      set_close: "关闭",
      set_custom: "自定义 / 私有端点",
      set_found_n: "发现 {0} 个可用模型，点选填入",
      set_saved: "已保存到本工作区 icewright.toml",
      set_discovering: "发现中…",
      set_pick_ws: "先在左侧选择工作区",
      tab_model: "模型接入",
      tab_lang: "语言",
      tab_theme: "外观",
      lang_label: "界面语言",
      lang_auto: "自动跟随工作区",
      lang_hint: "未手动选择时界面语种跟随 workspace.locale；手动选择后跨重启记忆。",
      theme_label: "主题",
      theme_auto: "跟随系统",
      theme_light: "浅色",
      theme_dark: "深色",
      theme_hint: "主题即时生效并跨重启记忆；跟随系统时随操作系统深浅色自动切换。",
      ws_create: "新建",
      ws_id_ph: "新工作区 ID",
      ws_created: "已创建工作区 {0}：选中后点「初始化」启动管线",
      ws_empty_id: "请输入工作区 ID",
      aside_toggle: "收起 / 展开侧栏",
      err_base: "接入点必须以 http(s):// 开头",
      err_model: "模型名不能为空",
      err_key: "密钥引用格式不合法（掩码值需重新输入）",
      err_no_key: "未找到密钥环境变量：请填写密钥引用或先设置对应变量",
    },
  },
  en: {
    status: {
      pending: "Pending", running: "Running", waiting_gate: "Awaiting gate",
      approved: "Approved", failed: "Failed", blocked_preflight: "Blocked by preflight", skipped: "Skipped",
    },
    gate: { approved: "Approved", rejected: "Rejected", partial_edit: "Rejected with partial edits" },
    stage: {
      S1: "Requirement intake", S2: "Environment preflight", S3: "Domain rule extraction",
      S4: "Design doc & Gate A", S5: "Code generation", S6: "Automated verification",
      S7: "Delivery & Gate B", S8: "Change / regeneration",
    },
    ops: {
      init: "Init", preflight: "Preflight", design_render: "Render design",
      design_approve: "Approve Gate A", design_reject: "Reject Gate A", generate: "Generate code",
      verify: "Verify", delivery_render: "Render delivery",
      delivery_approve: "Approve Gate B", delivery_reject: "Reject Gate B",
    },
    ui: {
      workspaces: "Workspaces", hint_pre: "Run", hint_post: " to create a workspace",
      empty_pick: "Pick a workspace on the left to see progress and history",
      th_stage: "Stage", th_name: "Name", th_status: "Status", th_hash: "Artifact hash", th_gate: "Gate",
      history: "Build history",
      note_placeholder: "Rejection reason (required for reject actions)",
      not_init: "Pipeline not initialized: click Init below or run icewright pipeline init",
      stage_now: "current stage", updated: "updated",
      arm: "Click again to confirm",
      executing: "{0} running…",
      refresh_fail: "Refresh failed: {0}",
      menu_refreshed: "Refreshed · {0}",
      open_fail: "Could not open in system browser, visit manually: {0}",
      no_history: "No history yet",
      about_desc: "S1-S8 pipeline builder: hand industry rules and flows to the AI, get a deliverable customer-service system back.",
      about_github: "GitHub Homepage",
      about_close: "Close",
      settings: "Settings",
      set_provider: "Provider",
      set_base: "Base URL",
      set_model: "Model",
      set_discover: "Discover",
      set_key: "Key reference",
      set_key_hint: "Store a reference, never the key itself: env://VAR / keyring://service/account / plain:literal (not recommended)",
      set_save: "Save",
      set_close: "Close",
      set_custom: "Custom / private endpoint",
      set_found_n: "{0} models found, pick one to fill in",
      set_saved: "Saved to this workspace's icewright.toml",
      set_discovering: "Discovering…",
      set_pick_ws: "Pick a workspace on the left first",
      tab_model: "Model",
      tab_lang: "Language",
      tab_theme: "Appearance",
      lang_label: "UI language",
      lang_auto: "Follow workspace locale",
      lang_hint: "Without a manual choice the UI language follows workspace.locale; a manual choice persists across restarts.",
      theme_label: "Theme",
      theme_auto: "Follow system",
      theme_light: "Light",
      theme_dark: "Dark",
      theme_hint: "Theme applies instantly and persists across restarts; follow-system tracks the OS light/dark setting.",
      ws_create: "Create",
      ws_id_ph: "New workspace id",
      ws_created: "Workspace {0} created: select it and click Init to start the pipeline",
      ws_empty_id: "Enter a workspace id",
      aside_toggle: "Collapse / expand sidebar",
      err_base: "Base URL must start with http(s)://",
      err_model: "Model name is required",
      err_key: "Invalid key reference (re-enter if it shows a mask)",
      err_no_key: "No key environment variable found: set a key reference or export the variable first",
    },
  },
};

// 手动语言选择优先于 workspace.locale，并跨重启记忆（开源用户可能不配置 locale）。
var langPref = null;
try { langPref = localStorage.getItem("iw-lang"); } catch (e) { langPref = null; }
if (langPref !== "en" && langPref !== "zh") { langPref = null; }
var lang = langPref || "zh";
// 原生菜单栏语种与界面语种保持同步（浏览器 mock 环境下静默降级）。
function pushMenuLang(l) {
  try { invoke("set_menu_lang", { lang: l }).catch(function () {}); } catch (e) { /* 非 Tauri 环境 */ }
}
function syncLang(l) {
  lang = l;
  langPref = l;
  try { localStorage.setItem("iw-lang", l); } catch (e) { /* 隐私模式下忽略 */ }
  applyStatic();
  renderLangToggle();
  pushMenuLang(l);
}
function renderLangToggle() {
  $("lang-zh").classList.toggle("active", lang === "zh");
  $("lang-en").classList.toggle("active", lang === "en");
}
function initLangToggle() {
  $("lang-zh").onclick = function () { syncLang("zh"); refresh(); };
  $("lang-en").onclick = function () { syncLang("en"); refresh(); };
  applyStatic();
  renderLangToggle();
}
function L() { return DICT[lang] || DICT.zh; }
function U(key) {
  var v = L().ui[key];
  return v === undefined ? DICT.zh.ui[key] : v;
}
function fmt(tpl, args) {
  return tpl.replace(/\{(\d+)\}/g, function (m, i) {
    return args[i] !== undefined ? String(args[i]) : m;
  });
}
function applyStatic() {
  document.querySelectorAll("[data-i18n]").forEach(function (el) {
    el.textContent = U(el.getAttribute("data-i18n"));
  });
  $("op-note").placeholder = U("note_placeholder");
  $("ws-new-id").placeholder = U("ws_id_ph");
  $("aside-toggle").title = U("aside_toggle");
}

/* ---------- 主题：跟随系统 / 强制浅色 / 强制深色 ---------- */
var THEMES = ["auto", "light", "dark"];
var themePref = "auto";
try { if (THEMES.indexOf(localStorage.getItem("iw_theme")) >= 0) { themePref = localStorage.getItem("iw_theme"); } } catch (e) { /* 隐私模式 */ }
function applyTheme() {
  if (themePref === "auto") { document.documentElement.removeAttribute("data-theme"); }
  else { document.documentElement.setAttribute("data-theme", themePref); }
}
function setTheme(t) {
  themePref = t;
  try {
    if (t === "auto") { localStorage.removeItem("iw_theme"); }
    else { localStorage.setItem("iw_theme", t); }
  } catch (e) { /* 隐私模式 */ }
  applyTheme();
}
applyTheme();

var selected = null;
var $ = function (id) { return document.getElementById(id); };

function esc(s) {
  var d = document.createElement("div");
  d.textContent = String(s);
  return d.innerHTML;
}

function shortHash(h) {
  return h ? h.replace(/^sha256:/, "").slice(0, 8) : "--------";
}

function fmtTs(ts) {
  if (!ts) { return "-"; }
  return ts.replace("T", " ").slice(0, 19) + "Z";
}

async function loadWorkspaces() {
  var ids = await invoke("ws_list");
  var ul = $("ws-list");
  ul.innerHTML = "";
  ids.forEach(function (id) {
    var li = document.createElement("li");
    li.textContent = id;
    li.className = id === selected ? "active" : "";
    li.onclick = function () { selected = id; refresh(); };
    ul.appendChild(li);
  });
  if (ids.length && !selected) { selected = ids[0]; }
}

/* ---------- 侧栏收起 ---------- */
function initAside() {
  var lay = $("layout");
  var collapsed = false;
  try { collapsed = localStorage.getItem("iw_aside") === "1"; } catch (e) { /* 隐私模式 */ }
  function paint() {
    lay.classList.toggle("collapsed", collapsed);
    $("aside-toggle").textContent = collapsed ? "▶" : "◀";
  }
  paint();
  $("aside-toggle").onclick = function () {
    collapsed = !collapsed;
    paint();
    try { localStorage.setItem("iw_aside", collapsed ? "1" : "0"); } catch (e) { /* 隐私模式 */ }
  };
}

/* ---------- 新建工作区 ---------- */
function wsNote(text) {
  var n = $("ws-note");
  n.classList.remove("hidden");
  n.textContent = text;
}

function createWorkspace() {
  var id = $("ws-new-id").value.trim();
  if (!id) { wsNote("✘ " + U("ws_empty_id")); return; }
  invoke("ws_create", { wsId: id }).then(function () {
    $("ws-new-id").value = "";
    selected = id;
    wsNote("✔ " + fmt(U("ws_created"), [id]));
    tick();
  }).catch(function (err) {
    wsNote("✘ " + (String(err) === "empty_ws_id" ? U("ws_empty_id") : err));
  });
}

function initWsCreate() {
  $("ws-create").onclick = createWorkspace;
  $("ws-new-id").addEventListener("keydown", function (e) {
    if (e.key === "Enter") { createWorkspace(); }
  });
}

function renderStatus(st) {
  if (!st) {
    $("run-meta").textContent = U("not_init");
    $("stepper").innerHTML = "";
    $("stage-rows").innerHTML = "";
    return;
  }
  $("run-meta").textContent =
    "run " + st.run_id + " · pack " + (st.pack_ref || "-") +
    " · " + U("stage_now") + " " + st.current_stage +
    " · " + U("updated") + " " + fmtTs(st.updated_at);
  var step = $("stepper");
  step.innerHTML = "";
  st.stages.forEach(function (s) {
    var cell = document.createElement("div");
    cell.className = "step " + s.status;
    cell.title = L().status[s.status] || s.status;
    cell.innerHTML = "<b>" + esc(s.id) + "</b>" +
      "<span class='nm'>" + esc(titleOf(s.id)) + "</span>";
    step.appendChild(cell);
  });
  var rows = $("stage-rows");
  rows.innerHTML = "";
  st.stages.forEach(function (s) {
    var gate = s.gate
      ? (L().gate[s.gate.decision] || s.gate.decision) + " · " + s.gate.by
      : "";
    var tr = document.createElement("tr");
    tr.innerHTML =
      "<td>" + esc(s.id) + "</td>" +
      "<td>" + esc(titleOf(s.id)) + "</td>" +
      "<td class='st-" + s.status + "'>" + (L().status[s.status] || s.status) + "</td>" +
      "<td><code>" + shortHash(s.output_hash) + "</code></td>" +
      "<td>" + esc(gate) + "</td>";
    rows.appendChild(tr);
  });
}

/* ---------- 窗口内推进操作 ---------- */
// 按钮可用性只按状态机粗粒度门控；最终裁决在引擎（错误会显示在结果面板）。
var OPS = [
  { op: "init", need: function (st) { return !st; } },
  { op: "preflight", need: function (st) { return !!st; } },
  { op: "design_render", need: function (st) { return statusOf(st, "S3") === "approved"; } },
  { op: "design_approve", confirm: true, need: function (st) { return statusOf(st, "S4") === "waiting_gate"; } },
  { op: "design_reject", note: true, need: function (st) { return statusOf(st, "S4") === "waiting_gate"; } },
  { op: "generate", need: function (st) { return statusOf(st, "S4") === "approved"; } },
  { op: "verify", need: function (st) { return statusOf(st, "S5") === "approved"; } },
  { op: "delivery_render", need: function (st) { return statusOf(st, "S6") === "approved"; } },
  { op: "delivery_approve", confirm: true, need: function (st) { return statusOf(st, "S7") === "waiting_gate"; } },
  { op: "delivery_reject", note: true, need: function (st) { return statusOf(st, "S7") === "waiting_gate"; } },
];

var busy = false;
var armedOp = null;   // 双击确认：第一次点击挂起，3 秒内再点才执行
var armTimer = null;

function labelOf(spec) { return L().ops[spec.op] || spec.op; }

function statusOf(st, id) {
  if (!st) { return null; }
  var s = st.stages.find(function (x) { return x.id === id; });
  return s ? s.status : null;
}

function renderActions(st) {
  lastSt = st;
  OPS.forEach(function (spec) {
    var btn = $("btn-" + spec.op);
    if (!btn) { return; }
    var enabled = !busy && spec.need(st);
    if (spec.note) { enabled = enabled && $("op-note").value.trim().length > 0; }
    btn.disabled = !enabled;
    btn.textContent = armedOp === spec.op ? U("arm") : labelOf(spec);
    btn.classList.toggle("arm", armedOp === spec.op);
  });
  $("op-note").disabled = busy;
}

var lastSt = null;

function disarm() {
  armedOp = null;
  if (armTimer) { clearTimeout(armTimer); armTimer = null; }
  renderActions(lastSt);
}

async function currentStatus() {
  return await invoke("pipeline_status", { wsId: selected });
}

async function runOp(spec) {
  var st = await currentStatus();
  if (spec.confirm) {
    if (armedOp !== spec.op) {
      armedOp = spec.op;
      armTimer = setTimeout(disarm, 3000);
      renderActions(st);
      return;
    }
    disarm();
  }
  var label = labelOf(spec);
  var note = $("op-note").value.trim();
  busy = true;
  renderActions(st);
  var res = $("op-result");
  res.classList.remove("hidden");
  res.textContent = "⏳ " + fmt(U("executing"), [label]);
  try {
    var out = await invoke("run_op", {
      op: spec.op, wsId: selected,
      note: spec.note ? note : null,
    });
    res.textContent = "✔ " + label + "\n" + out;
    $("op-note").value = "";
  } catch (err) {
    res.textContent = "✘ " + label + "\n" + err;
  }
  busy = false;
  await refresh();
}

function initActions() {
  var box = $("op-buttons");
  OPS.forEach(function (spec) {
    var b = document.createElement("button");
    b.id = "btn-" + spec.op;
    b.textContent = labelOf(spec);
    b.className = spec.confirm ? "confirm" : (spec.note ? "danger" : "");
    b.onclick = function () { runOp(spec); };
    box.appendChild(b);
  });
  $("op-note").addEventListener("input", async function () {
    renderActions(await currentStatus());
  });
}

function titleOf(id) { return L().stage[id] || id; }

async function renderHistory() {
  var events = await invoke("history_tail", { wsId: selected, n: 50 });
  var ul = $("history");
  ul.innerHTML = "";
  if (!events.length) {
    ul.innerHTML = "<li class='hint'>" + esc(U("no_history")) + "</li>";
    return;
  }
  events.slice().reverse().forEach(function (e) {
    var li = document.createElement("li");
    li.innerHTML =
      "<span class='ts'>" + fmtTs(e.ts) + "</span>" +
      "<span class='stage'>" + esc(e.stage) + "</span>" +
      "<span class='detail'>" + esc(e.detail) + "</span>";
    ul.appendChild(li);
  });
}

async function refresh() {
  if (!selected) { return; }
  var loc = await invoke("ws_locale", { wsId: selected });
  var next = langPref || (String(loc).trim().toLowerCase() === "en" ? "en" : "zh");
  if (next !== lang) {
    lang = next;
    applyStatic();
    renderLangToggle();
    pushMenuLang(next);
  }
  var st = await invoke("pipeline_status", { wsId: selected });
  $("empty").classList.add("hidden");
  $("detail").classList.remove("hidden");
  $("ws-title").textContent = selected;
  renderStatus(st);
  renderActions(st);
  await renderHistory();
}

async function tick() {
  try {
    await loadWorkspaces();
    await refresh();
  } catch (err) {
    $("run-meta").textContent = fmt(U("refresh_fail"), [err]);
  }
}

/* ---------- 关于对话框与原生菜单栏事件 ---------- */
var APP_VER = "";

function showAbout() {
  $("about-version").textContent = APP_VER ? "v" + APP_VER : "";
  $("about-modal").classList.remove("hidden");
}
function hideAbout() { $("about-modal").classList.add("hidden"); }

function initAbout() {
  $("about-close").onclick = hideAbout;
  $("about-github").onclick = function () {
    try {
      invoke("open_github").catch(function () {
        menuNote("✘ " + fmt(U("open_fail"), [GITHUB_URL]));
      });
    } catch (e) { /* 非 Tauri 环境 */ }
  };
  $("about-modal").addEventListener("click", function (e) {
    if (e.target === this) { hideAbout(); }
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") { hideAbout(); }
  });
}

function setErr(err) {
  var map = {
    invalid_base_url: "err_base",
    invalid_model: "err_model",
    invalid_key_ref: "err_key",
    no_key_env: "err_no_key",
  };
  var key = map[String(err)];
  return key ? U(key) : String(err);
}

function setProviderOptions(list) {
  var sel = $("set-provider");
  sel.innerHTML = "";
  list.forEach(function (p) {
    var o = document.createElement("option");
    o.value = p.name;
    o.textContent = p.display + " · " + p.base_url;
    o.o = p;
    sel.appendChild(o);
  });
  var custom = document.createElement("option");
  custom.value = "";
  custom.textContent = U("set_custom");
  custom.o = null;
  sel.appendChild(custom);
}

function fillFromProvider() {
  var sel = $("set-provider");
  var opt = sel.options[sel.selectedIndex];
  if (opt && opt.o) {
    $("set-base").value = opt.o.base_url;
    if (!$("set-model").value) { $("set-model").value = opt.o.default_model; }
    var envs = opt.o.key_envs || [];
    if (envs.length) { $("set-key").placeholder = "env://" + envs[0]; }
  }
}

function openSettings() {
  showSettingsTab("model");
  syncSettingRadios();
  if (!selected) {
    $("set-status").textContent = U("set_pick_ws");
    $("settings-modal").classList.remove("hidden");
    return;
  }
  $("set-status").textContent = "";
  $("set-found").classList.add("hidden");
  $("settings-modal").classList.remove("hidden");
  Promise.all([invoke("model_providers"), invoke("model_get", { wsId: selected })])
    .then(function (rs) {
      var list = rs[0], info = rs[1];
      setProviderOptions(list);
      var match = list.find(function (p) { return p.base_url === info.base_url; });
      $("set-provider").value = match ? match.name : "";
      $("set-base").value = info.base_url || "";
      $("set-model").value = info.model || "";
      $("set-key").value = info.key_ref || "";
    })
    .catch(function (err) { $("set-status").textContent = setErr(err); });
}

function closeSettings() { $("settings-modal").classList.add("hidden"); }

function discoverModels() {
  var base = $("set-base").value.trim();
  var status = $("set-status");
  status.textContent = U("set_discovering");
  invoke("model_discover", { baseUrl: base, keyRef: $("set-key").value.trim() || null })
    .then(function (ids) {
      var sel = $("set-found");
      sel.innerHTML = "";
      ids.forEach(function (id) {
        var o = document.createElement("option");
        o.value = id; o.textContent = id;
        sel.appendChild(o);
      });
      sel.classList.remove("hidden");
      status.textContent = fmt(U("set_found_n"), [ids.length]);
    })
    .catch(function (err) { status.textContent = setErr(err); });
}

function saveModel() {
  invoke("model_set", {
    wsId: selected,
    baseUrl: $("set-base").value,
    model: $("set-model").value,
    keyRef: $("set-key").value,
  }).then(function () {
    $("set-status").textContent = U("set_saved");
    tick();
  }).catch(function (err) { $("set-status").textContent = setErr(err); });
}

/* ---------- 设置标签页与语言/外观面板 ---------- */
function showSettingsTab(t) {
  ["model", "lang", "theme"].forEach(function (k) {
    $("tab-" + k).classList.toggle("active", k === t);
    $("pane-" + k).classList.toggle("hidden", k !== t);
  });
  // 保存只对模型接入有意义；语言/外观单选即时生效。
  $("set-save").classList.toggle("hidden", t !== "model");
  $("set-status").textContent = "";
}

function eachRadio(name, fn) {
  Array.prototype.forEach.call(document.getElementsByName(name), fn);
}

function syncSettingRadios() {
  var cur = langPref || "auto";
  eachRadio("pref-lang", function (r) { r.checked = r.value === cur; });
  eachRadio("pref-theme", function (r) { r.checked = r.value === themePref; });
}

function setLangPref(p) {
  if (p === "auto") {
    langPref = null;
    try { localStorage.removeItem("iw-lang"); } catch (e) { /* 隐私模式 */ }
    if (selected) {
      refresh();
    } else {
      lang = "zh";
      applyStatic();
      renderLangToggle();
      pushMenuLang(lang);
    }
  } else {
    syncLang(p);
    refresh();
  }
}

function initSettingsPanes() {
  ["model", "lang", "theme"].forEach(function (k) {
    $("tab-" + k).onclick = function () { showSettingsTab(k); };
  });
  eachRadio("pref-lang", function (r) {
    r.onchange = function () { if (r.checked) { setLangPref(r.value); } };
  });
  eachRadio("pref-theme", function (r) {
    r.onchange = function () { if (r.checked) { setTheme(r.value); } };
  });
}

function initSettings() {
  $("open-settings").onclick = openSettings;
  $("set-close").onclick = closeSettings;
  $("set-save").onclick = saveModel;
  $("set-discover").onclick = discoverModels;
  $("set-provider").onchange = fillFromProvider;
  $("set-found").onchange = function () { $("set-model").value = this.value; };
  initSettingsPanes();
  $("settings-modal").addEventListener("click", function (e) {
    if (e.target === this) { closeSettings(); }
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") { closeSettings(); }
  });
}

var GITHUB_URL = "https://github.com/Filthy-ice/Ice-Wright";

function menuNote(text) {
  var res = $("op-result");
  res.classList.remove("hidden");
  res.textContent = text;
}

function initMenuEvents() {
  var ev = window.__TAURI__ && window.__TAURI__.event;
  if (!ev || !ev.listen) { return; }
  ev.listen("menu-action", function (msg) {
    var a = String(msg.payload);
    if (a === "refresh") {
      tick().then(function () {
        menuNote("✔ " + fmt(U("menu_refreshed"), [new Date().toLocaleTimeString()]));
      });
    }
    else if (a === "reload") { location.reload(); }
    else if (a === "about") { showAbout(); }
    else if (a === "settings") { openSettings(); }
    else if (a === "github_failed") {
      menuNote("✘ " + fmt(U("open_fail"), [GITHUB_URL]));
    }
    else if (a === "lang:zh") { syncLang("zh"); refresh(); }
    else if (a === "lang:en") { syncLang("en"); refresh(); }
  }).catch(function () {});
}

initLangToggle();
initAside();
initWsCreate();
initActions();
initAbout();
initSettings();
initMenuEvents();
invoke("app_version").then(function (v) { APP_VER = v; }).catch(function () {});
// 窗口以 visible:false 启动：首帧渲染完成后亮相，启动动画随之播放。
tick().then(function () {
  try { invoke("show_window").catch(function () {}); } catch (e) { /* 非 Tauri 环境 */ }
});
setInterval(tick, 2000);
