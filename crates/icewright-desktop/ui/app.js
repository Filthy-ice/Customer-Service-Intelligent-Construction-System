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
      init: "初始化", preflight: "环境预检", extract: "提取语料", design_render: "渲染设计",
      design_approve: "批准闸门A", design_reject: "驳回闸门A", generate: "生成代码",
      verify: "自动验证", delivery_render: "渲染交付",
      delivery_approve: "批准闸门B", delivery_reject: "驳回闸门B",
    },
    ui: {
      workspaces: "工作区",
      empty_pick: "选择左侧工作区查看进度与历史",
      th_stage: "阶段", th_name: "名称", th_status: "状态", th_hash: "产物哈希", th_gate: "闸门",
      history: "生成历史",
      note_placeholder: "驳回原因（驳回类操作必填）",
      not_init: "尚未初始化 pipeline：点下方「初始化」启动",
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
      model_settings: "模型设置",
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
      opened_corpus: "已在文件管理器中打开语料目录",
      opened_output: "已在文件管理器中打开生成目录",
      delivery_hint: "在「生成项目」页填写客户规定的去向目录，点「确认交付目录」；代码将直接生成到该目录，工作区不留副本",
      ws_create: "新建",
      ws_id_ph: "新工作区 ID",
      ws_created: "已创建工作区 {0}：选中后点「初始化」启动管线",
      ws_empty_id: "请输入工作区 ID",
      ws_none: "暂时没有项目",
      aside_toggle: "收起 / 展开侧栏",
      tab_flow: "流程",
      tab_corpus: "需求语料",
      tab_output: "生成项目",
      open_in_folder: "在文件夹中打开",
      corpus_files: "语料文件",
      corpus_name_ph: "文件名（如 refund.md）",
      corpus_add: "添加",
      corpus_path_ph: "客户文件/目录路径（绝对或 ~/ 开头），导入到所选分类",
      corpus_import: "从路径导入",
      corpus_import_update: "覆盖同名（刷新）",
      corpus_import_busy: "导入中…",
      corpus_import_ok: "已导入 {0} 个文件到 corpus/{1}",
      corpus_import_upd: "（{0} 个已覆盖更新）",
      corpus_import_img: "（{0} 张图片已留档，暂不参与文本提取，待视觉解析接入；流程图建议另附文字版步骤）",
      corpus_import_same: "（{0} 个内容相同未重复入库）",
      corpus_import_bin: "（{0} 个非文本已跳过）",
      corpus_edit_ph: "在此编写或粘贴需求文本（markdown / 纯文本）",
      corpus_save: "保存语料",
      model_lang_label: "模型操作语言",
      model_lang_none: "未选择",
      model_lang_hint: "决定发给模型的提示词语种与生成物默认回复语种；与界面语言、供应商国籍无关。初始化管线前必须选定。",
      model_lang_saved: "已设为 {0}（与界面语言无关）",
      output_files: "生成文件",
      output_edit_ph: "点左侧文件查看，可直接编辑后保存",
      output_save: "保存修改",
      delivery_ph: "客户规定的交付目录（如 ~/桌面/客服交付 或 D:\\交付，可含 ~）",
      delivery_go: "确认交付目录",
      delivery_ok: "交付目录已确认：{0}——代码将直接生成到该目录，工作区不留副本",
      delivery_busy: "确认交付目录中…",
      delivery_state_ok: "已确认交付目录：{0}（S5 直接生成到此处）",
      delivery_state_pending: "交付目录已设定：{0}（尚未确认，生成会被拒绝）",
      delivery_state_none: "尚未设定交付目录：生成代码前须由客户规定去向",
      pick_file: "点左侧文件查看与编辑",
      no_corpus: "暂无语料：下方选分类新建，或用「从路径导入」收客户材料",
      no_output: "尚未生成项目：确认交付目录后，在「流程」页点「生成代码」",
      saved: "已保存 {0}",
      corpus_added: "已创建 {0}，编辑后记得保存",
      corpus_exists: "该语料文件已存在，直接在左侧点开编辑",
      corpus_empty_name: "请输入文件名",
      unsaved_first: "有未保存的修改：请先保存",
      output_saved: "已保存 {0}（注意：S8 重生成可能覆盖手工修改）",
      open_dir_fail: "打开文件夹失败：{0}",
      err_base: "接入点必须以 http(s):// 开头",
      err_model: "模型名不能为空",
      err_key: "密钥引用格式不合法（掩码值需重新输入）",
      err_no_key: "未找到密钥环境变量：请填写密钥引用或先设置对应变量",
      err_need_init: "pipeline 尚未初始化：先在「流程」页点「初始化」",
      err_s2_not_approved: "S2 环境预检未批准：先在「流程」页执行预检",
      err_rel: "文件路径不合法（语料须为「分类/文件名」，禁止 .. 和绝对路径）",
      err_big: "文件超过 4 MB 上限",
      err_empty_dest: "交付目录不能为空",
      err_empty_path: "请输入要导入的文件或目录路径",
      err_need_abs: "交付目录必须是绝对路径（或 ~ 开头的家目录路径）",
      err_empty_output: "output/ 尚无生成物可导出：先在「流程」页完成生成",
      sec_flow: "整体流程",
      sec_ops: "操作与结果",
      sec_stages: "阶段明细",
      pf_pass: "预检通过：全部检查项合格",
      pf_fail: "预检未通过：修复 FAIL 项后重试",
      env_stale_banner: "S2 环境预检未通过：其后的绿色状态均为历史记录；修复 FAIL 项并重跑预检之前，后续所有推进操作都会被拒绝。",
      stale_tag: "历史记录",
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
      init: "Init", preflight: "Preflight", extract: "Extract corpus", design_render: "Render design",
      design_approve: "Approve Gate A", design_reject: "Reject Gate A", generate: "Generate code",
      verify: "Verify", delivery_render: "Render delivery",
      delivery_approve: "Approve Gate B", delivery_reject: "Reject Gate B",
    },
    ui: {
      workspaces: "Workspaces",
      empty_pick: "Pick a workspace on the left to see progress and history",
      th_stage: "Stage", th_name: "Name", th_status: "Status", th_hash: "Artifact hash", th_gate: "Gate",
      history: "Build history",
      note_placeholder: "Rejection reason (required for reject actions)",
      not_init: "Pipeline not initialized: click Init below to start",
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
      model_settings: "Model settings",
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
      opened_corpus: "Corpus folder opened in the file manager",
      opened_output: "Output folder opened in the file manager",
      delivery_hint: "In the Generated project tab, type the delivery folder the customer chose and click Confirm delivery folder; code is generated straight there with no workspace copy",
      ws_create: "Create",
      ws_id_ph: "New workspace id",
      ws_created: "Workspace {0} created: select it and click Init to start the pipeline",
      ws_empty_id: "Enter a workspace id",
      ws_none: "No projects yet",
      aside_toggle: "Collapse / expand sidebar",
      tab_flow: "Flow",
      tab_corpus: "Requirement corpus",
      tab_output: "Generated project",
      open_in_folder: "Open in folder",
      corpus_files: "Corpus files",
      corpus_name_ph: "File name (e.g. refund.md)",
      corpus_add: "Add",
      corpus_path_ph: "Customer file/folder path (absolute or ~/...), imported into the selected category",
      corpus_import: "Import from path",
      corpus_import_update: "Overwrite same-name (refresh)",
      corpus_import_busy: "Importing…",
      corpus_import_ok: "Imported {0} file(s) into corpus/{1}",
      corpus_import_upd: "({0} overwritten with fresh content)",
      corpus_import_img: "({0} image(s) archived — excluded from text extraction until a vision-parsing channel is added; ask for a text/mermaid flowchart too)",
      corpus_import_same: "({0} already present with identical content)",
      corpus_import_bin: "({0} non-text file(s) skipped)",
      corpus_edit_ph: "Write or paste requirement text here (markdown / plain text)",
      corpus_save: "Save corpus",
      model_lang_label: "Model language",
      model_lang_none: "Not chosen",
      model_lang_hint: "Drives the language of prompts sent to the model and the generated project's default reply language; independent of UI locale and vendor country. Must be picked before pipeline init.",
      model_lang_saved: "Set to {0} (independent of UI locale)",
      output_files: "Generated files",
      output_edit_ph: "Pick a file on the left to view; edit directly and save",
      output_save: "Save edits",
      delivery_ph: "Delivery folder chosen by the customer (e.g. ~/Desktop/cs-delivery or D:\\delivery; ~ allowed)",
      delivery_go: "Confirm delivery folder",
      delivery_ok: "Delivery folder confirmed: {0} — code will be generated straight there, no workspace copy",
      delivery_busy: "Confirming delivery folder…",
      delivery_state_ok: "Delivery folder confirmed: {0} (S5 generates straight here)",
      delivery_state_pending: "Delivery folder set: {0} (not confirmed yet — generation will be refused)",
      delivery_state_none: "No delivery folder yet: the customer must choose where artifacts go before generation",
      pick_file: "Pick a file on the left to view and edit",
      no_corpus: "No corpus yet: add a file below, or import the customer's files by path",
      no_output: "No generated project yet: confirm the delivery folder, then run \"Generate code\" in the Flow tab",
      saved: "Saved {0}",
      corpus_added: "Created {0}; edit it and remember to save",
      corpus_exists: "That corpus file already exists - open it from the list",
      corpus_empty_name: "Enter a file name",
      unsaved_first: "Unsaved changes: save first",
      output_saved: "Saved {0} (note: S8 regeneration may overwrite manual edits)",
      open_dir_fail: "Could not open folder: {0}",
      err_base: "Base URL must start with http(s)://",
      err_model: "Model name is required",
      err_key: "Invalid key reference (re-enter if it shows a mask)",
      err_no_key: "No key environment variable found: set a key reference or export the variable first",
      err_need_init: "Pipeline not initialized: click Init in the Flow tab first",
      err_s2_not_approved: "S2 preflight not approved: run Preflight in the Flow tab first",
      err_rel: "Invalid file path (corpus must be \"category/file\", no .. or absolute paths)",
      err_big: "File exceeds the 4 MB limit",
      err_empty_dest: "Delivery folder is required",
      err_empty_path: "Enter a file or folder path to import",
      err_need_abs: "Delivery folder must be absolute (or start with ~ for your home directory)",
      err_empty_output: "Nothing to export yet: finish \"Generate code\" in the Flow tab first",
      sec_flow: "Pipeline overview",
      sec_ops: "Actions & results",
      sec_stages: "Stage details",
      pf_pass: "Preflight passed: all checks OK",
      pf_fail: "Preflight failed: fix the FAIL items and retry",
      env_stale_banner: "S2 preflight failed: the green statuses after it are historical records. Until the failed checks are fixed and preflight re-runs, every downstream stage refuses to advance.",
      stale_tag: "historical",
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
// 把 语言/主题 偏好同步给原生菜单栏，勾选项才能反映当前生效值。
function langIdx() { return langPref === null ? 0 : (langPref === "en" ? 2 : 1); }
function themeIdx() { return themePref === "light" ? 1 : (themePref === "dark" ? 2 : 0); }
// 系统深浅色的实时探针：auto 档时原生菜单/标题栏要跟着它走（webview 里 CSS 已自动跟随）。
var sysDarkMq = window.matchMedia("(prefers-color-scheme: dark)");
function syncPrefs() {
  try { invoke("sync_prefs", { lang: langIdx(), theme: themeIdx(), sysDark: sysDarkMq.matches }).catch(function () {}); } catch (e) { /* 非 Tauri 环境 */ }
}
function syncLang(l) {
  lang = l;
  langPref = l;
  try { localStorage.setItem("iw-lang", l); } catch (e) { /* 隐私模式下忽略 */ }
  applyStatic();
  pushMenuLang(l);
  syncPrefs();
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
  document.querySelectorAll("[data-i18n-ph]").forEach(function (el) {
    el.placeholder = U(el.getAttribute("data-i18n-ph"));
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
  syncPrefs();
}
applyTheme();
// 系统深浅色中途变化：CSS 会自动重绘页面，但 auto 档下要再把原生菜单/标题栏拨过去。
try {
  sysDarkMq.addEventListener("change", function () { if (themePref === "auto") { syncPrefs(); } });
} catch (e) { /* 老引擎不支持 addEventListener 时忽略 */ }

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

/* ---------- 工作区列表：选择记忆（上次选中跨重启恢复）+ 启动即高亮 + 空列表提示 ---------- */
var wsListCache = "";
function wsMemGet() { try { return localStorage.getItem("iw_ws"); } catch (e) { return null; } }
function wsMemSet(id) {
  try {
    if (id) { localStorage.setItem("iw_ws", id); } else { localStorage.removeItem("iw_ws"); }
  } catch (e) { /* 隐私模式下忽略 */ }
}

async function loadWorkspaces() {
  var ids = await invoke("ws_list");
  // 启动优先恢复记忆的工作区；不在列表（如已删除）回退第一个。须在建 DOM 前定好，高亮才不缺位
  if (ids.indexOf(selected) < 0) {
    var mem = wsMemGet();
    selected = ids.indexOf(mem) >= 0 ? mem : (ids[0] || null);
  }
  wsMemSet(selected);
  var joined = ids.join("\n") + "|" + lang;
  // 列表没变就不重建 DOM：每 2s 的 tick 重绘会打断高亮/悬停样式，切换看起来发迟
  if (joined !== wsListCache) {
    wsListCache = joined;
    var ul = $("ws-list");
    ul.innerHTML = "";
    if (!ids.length) {
      var none = document.createElement("li");
      none.className = "hint";
      none.textContent = U("ws_none");
      ul.appendChild(none);
    }
    ids.forEach(function (id) {
      var li = document.createElement("li");
      li.textContent = id;
      li.className = id === selected ? "active" : "";
      // 高亮立刻跟上点击，不等下一次 tick 重绘
      li.onclick = function () { selected = id; wsMemSet(id); markActive("ws-list", id); refresh(); };
      ul.appendChild(li);
    });
  }
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
  var envBad = statusOf(st, "S2") === "blocked_preflight" || statusOf(st, "S2") === "failed";
  var banner = $("env-banner");
  banner.classList.toggle("hidden", !envBad);
  if (envBad) { banner.textContent = "⚠ " + U("env_stale_banner"); }
  function isStale(s, i) {
    return envBad && i > 1 && (s.status === "approved" || s.status === "waiting_gate");
  }
  var step = $("stepper");
  step.innerHTML = "";
  st.stages.forEach(function (s, i) {
    var cell = document.createElement("div");
    cell.className = "step " + s.status + (isStale(s, i) ? " stale" : "");
    cell.title = (L().status[s.status] || s.status) + (isStale(s, i) ? " · " + U("stale_tag") : "");
    cell.innerHTML = "<b>" + esc(s.id) + "</b>" +
      "<span class='nm'>" + esc(titleOf(s.id)) + "</span>";
    step.appendChild(cell);
  });
  var rows = $("stage-rows");
  rows.innerHTML = "";
  st.stages.forEach(function (s, i) {
    var gate = s.gate
      ? (L().gate[s.gate.decision] || s.gate.decision) + " · " + s.gate.by
      : "";
    var tr = document.createElement("tr");
    if (isStale(s, i)) { tr.className = "stale"; }
    tr.innerHTML =
      "<td>" + esc(s.id) + "</td>" +
      "<td>" + esc(titleOf(s.id)) + "</td>" +
      "<td class='st-" + s.status + "'>" + (L().status[s.status] || s.status) +
      (isStale(s, i) ? "<span class='stale-tag'>" + esc(U("stale_tag")) + "</span>" : "") + "</td>" +
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
  { op: "extract", need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S3") !== "approved"; } },
  { op: "design_render", need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S3") === "approved"; } },
  { op: "design_approve", confirm: true, need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S4") === "waiting_gate"; } },
  { op: "design_reject", note: true, need: function (st) { return statusOf(st, "S4") === "waiting_gate"; } },
  { op: "generate", need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S4") === "approved"; } },
  { op: "verify", need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S5") === "approved"; } },
  { op: "delivery_render", need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S6") === "approved"; } },
  { op: "delivery_approve", confirm: true, need: function (st) { return statusOf(st, "S2") === "approved" && statusOf(st, "S7") === "waiting_gate"; } },
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
    if (spec.op === "preflight") { renderPreflight(out, label); }
    else { res.textContent = "✔ " + label + "\n" + out; }
    $("op-note").value = "";
  } catch (err) {
    res.textContent = "✘ " + label + "\n" + setErr(err);
  }
  busy = false;
  await refresh();
}

function renderPreflight(out, label) {
  var res = $("op-result");
  var data = null;
  try { data = JSON.parse(out); } catch (e) { data = null; }
  if (!data || !data.checks) { res.textContent = "✔ " + label + "\n" + out; return; }
  var html = "<div class='pf-verdict " + (data.all_ok ? "ok" : "bad") + "'>" +
    (data.all_ok ? "✔ " + U("pf_pass") : "✘ " + U("pf_fail")) + "</div>";
  data.checks.forEach(function (c) {
    html += "<div class='pf-row'><span class='pf-badge " + (c.ok ? "pass" : "fail") + "'>" +
      (c.ok ? "PASS" : "FAIL") + "</span><code class='pf-name'>" + esc(c.name) +
      "</code><span class='pf-detail'>" + esc(c.detail) + "</span></div>";
  });
  res.innerHTML = html;
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

function paintHistory(events) {
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

/* ---------- 明细标签：流程 / 需求语料 / 生成项目 ---------- */
var view = "flow";
function showViewTab(v) {
  view = v;
  ["flow", "corpus", "output"].forEach(function (k) {
    $("vtab-" + k).classList.toggle("active", k === v);
    $("vpane-" + k).classList.toggle("hidden", k !== v);
  });
  if (v === "corpus") { loadCorpus(); }
  else if (v === "output") { loadOutput(); }
}

/* ---------- 需求语料 / 生成项目：文件可看可改 ---------- */
var lastWs = null;              // 切换工作区时重置编辑区状态
var corpusRel = null, corpusBase = "";
var outRel = null, outBase = "";
var catsLoaded = false;
var importing = false;          // 语料导入进行中标记（防连点）
var mlangCur = "";              // 当前工作区已选定的模型操作语言（""=未选）

function panelStatus(id, msg) { $(id).textContent = msg; }

function renderFileList(ulId, files, curRel, openFn, emptyMsg) {
  var ul = $(ulId);
  ul.innerHTML = "";
  if (!files.length) {
    ul.innerHTML = "<li class='hint'>" + esc(emptyMsg) + "</li>";
    return;
  }
  files.forEach(function (f) {
    var li = document.createElement("li");
    li.textContent = f.rel;
    li.title = f.rel + " · " + f.bytes + " B";
    li.className = f.rel === curRel ? "active" : "";
    li.onclick = function () { openFn(f.rel); };
    ul.appendChild(li);
  });
}

function markActive(ulId, rel) {
  Array.prototype.forEach.call($(ulId).children, function (li) {
    li.classList.toggle("active", li.textContent === rel);
  });
}

function resetPanes() {
  corpusRel = null; corpusBase = "";
  outRel = null; outBase = "";
  ["corpus", "output"].forEach(function (k) {
    $(k + "-editor").value = "";
    $(k + "-edit-head").textContent = U("pick_file");
    $(k + "-save").disabled = true;
    panelStatus(k + "-status", "");
  });
}

function corpusDirty() { return corpusRel !== null && $("corpus-editor").value !== corpusBase; }
function outputDirty() { return outRel !== null && $("output-editor").value !== outBase; }

async function loadCorpus() {
  if (!selected) { return; }
  try {
    var paths = await invoke("ws_paths", { wsId: selected });
    $("corpus-dir").textContent = paths.corpus;
    mlangCur = paths.model_lang || "";
    var ms = $("corpus-mlang");
    ms.value = mlangCur;
    ms.title = U("model_lang_hint");
    if (!catsLoaded) {
      var cats = await invoke("corpus_cats");
      var sel = $("corpus-cat");
      sel.innerHTML = "";
      cats.forEach(function (c) {
        var o = document.createElement("option");
        o.value = c; o.textContent = c;
        sel.appendChild(o);
      });
      catsLoaded = true;
    }
    var files = await invoke("corpus_list", { wsId: selected });
    $("corpus-count").textContent = String(files.length);
    renderFileList("corpus-list", files, corpusRel, openCorpus, U("no_corpus"));
    if (corpusRel && !files.some(function (f) { return f.rel === corpusRel; })) {
      corpusRel = null;
      $("corpus-save").disabled = true;
      $("corpus-editor").value = "";
      $("corpus-edit-head").textContent = U("pick_file");
    }
  } catch (err) {
    panelStatus("corpus-status", "✘ " + setErr(err));
  }
}

async function openCorpus(rel) {
  if (corpusDirty()) { panelStatus("corpus-status", "✘ " + U("unsaved_first")); return; }
  try {
    var text = await invoke("corpus_read", { wsId: selected, rel: rel });
    corpusRel = rel; corpusBase = text;
    $("corpus-editor").value = text;
    $("corpus-edit-head").textContent = rel;
    $("corpus-save").disabled = false;
    panelStatus("corpus-status", "");
    markActive("corpus-list", rel);
    $("corpus-editor").focus();
  } catch (err) { panelStatus("corpus-status", "✘ " + setErr(err)); }
}

function addCorpus() {
  if (!selected) { panelStatus("corpus-status", "✘ " + U("set_pick_ws")); return; }
  if (corpusDirty()) { panelStatus("corpus-status", "✘ " + U("unsaved_first")); return; }
  var name = $("corpus-name").value.trim();
  if (!name) { panelStatus("corpus-status", "✘ " + U("corpus_empty_name")); return; }
  var rel = $("corpus-cat").value + "/" + name;
  var exists = Array.prototype.some.call($("corpus-list").children, function (li) {
    return li.textContent === rel;
  });
  if (exists) { panelStatus("corpus-status", "✘ " + U("corpus_exists")); return; }
  invoke("corpus_save", { wsId: selected, rel: rel, content: "" })
    .then(function () { $("corpus-name").value = ""; return loadCorpus(); })
    .then(function () { return openCorpus(rel); })
    .then(function () { panelStatus("corpus-status", "✔ " + fmt(U("corpus_added"), [rel])); })
    .catch(function (err) { panelStatus("corpus-status", "✘ " + setErr(err)); });
}

// 客户材料不必搬进工作区：给路径即可，引擎拷成 corpus/<分类>/ 快照，原件不动。
function importCorpus() {
  if (!selected) { panelStatus("corpus-status", "✘ " + U("set_pick_ws")); return; }
  if (corpusDirty()) { panelStatus("corpus-status", "✘ " + U("unsaved_first")); return; }
  var p = $("corpus-path").value.trim();
  if (!p) { panelStatus("corpus-status", "✘ " + U("err_empty_path")); return; }
  if (importing) { return; }
  importing = true;
  var btn = $("corpus-import");
  btn.disabled = true;
  panelStatus("corpus-status", "… " + U("corpus_import_busy"));
  invoke("corpus_import", { wsId: selected, path: p, cat: $("corpus-cat").value, update: $("corpus-update").checked })
    .then(function (r) {
      $("corpus-path").value = "";
      $("corpus-update").checked = false;
      var imported = r.copied.length + r.updated.length + (r.images ? r.images.length : 0);
      var msg = fmt(U("corpus_import_ok"), [String(imported), r.cat]);
      if (r.updated.length) { msg += fmt(U("corpus_import_upd"), [String(r.updated.length)]); }
      if (r.images && r.images.length) { msg += fmt(U("corpus_import_img"), [String(r.images.length)]); }
      if (r.identical) { msg += fmt(U("corpus_import_same"), [String(r.identical)]); }
      if (r.skipped_binary) { msg += fmt(U("corpus_import_bin"), [String(r.skipped_binary)]); }
      panelStatus("corpus-status", "✔ " + msg);
      return loadCorpus();
    })
    .catch(function (err) { panelStatus("corpus-status", "✘ " + setErr(err)); })
    .then(function () { importing = false; btn.disabled = false; });
}

// 模型操作语言：摄入时由客户选定（init 硬闸），与界面语言、供应商国籍无关。
function saveModelLang() {
  if (!selected) { panelStatus("corpus-mlang-status", "✘ " + U("set_pick_ws")); return; }
  var sel = $("corpus-mlang");
  var lang = sel.value;
  if (!lang) {
    // 「未选择」只是占位项：回退到当前值，不写空
    sel.value = mlangCur;
    panelStatus("corpus-mlang-status", "");
    return;
  }
  invoke("model_lang_set", { wsId: selected, lang: lang })
    .then(function (saved) {
      mlangCur = saved;
      sel.value = saved;
      panelStatus("corpus-mlang-status", "✔ " + fmt(U("model_lang_saved"), [saved]));
    })
    .catch(function (err) {
      sel.value = mlangCur;
      panelStatus("corpus-mlang-status", "✘ " + setErr(err));
    });
}

function saveCorpus() {
  if (!corpusRel) { return; }
  invoke("corpus_save", { wsId: selected, rel: corpusRel, content: $("corpus-editor").value })
    .then(function () {
      corpusBase = $("corpus-editor").value;
      panelStatus("corpus-status", "✔ " + fmt(U("saved"), [corpusRel]));
      return loadCorpus();
    })
    .catch(function (err) { panelStatus("corpus-status", "✘ " + setErr(err)); });
}

async function loadOutput() {
  if (!selected) { return; }
  try {
    var paths = await invoke("ws_paths", { wsId: selected });
    $("output-dir").textContent = paths.output;
    $("delivery-state").textContent = deliveryStateLine(paths);
    var files = await invoke("output_list", { wsId: selected });
    $("output-count").textContent = String(files.length);
    renderFileList("output-list", files, outRel, openOutput, U("no_output"));
    if (outRel && !files.some(function (f) { return f.rel === outRel; })) {
      outRel = null;
      $("output-save").disabled = true;
      $("output-editor").value = "";
      $("output-edit-head").textContent = U("pick_file");
    }
  } catch (err) {
    panelStatus("output-status", "✘ " + setErr(err));
  }
}

async function openOutput(rel) {
  if (outputDirty()) { panelStatus("output-status", "✘ " + U("unsaved_first")); return; }
  try {
    var text = await invoke("output_read", { wsId: selected, rel: rel });
    outRel = rel; outBase = text;
    $("output-editor").value = text;
    $("output-edit-head").textContent = rel;
    $("output-save").disabled = false;
    panelStatus("output-status", "");
    markActive("output-list", rel);
    $("output-editor").focus();
  } catch (err) { panelStatus("output-status", "✘ " + setErr(err)); }
}

function saveOutput() {
  if (!outRel) { return; }
  invoke("output_save", { wsId: selected, rel: outRel, content: $("output-editor").value })
    .then(function () {
      outBase = $("output-editor").value;
      panelStatus("output-status", "✔ " + fmt(U("output_saved"), [outRel]));
      return loadOutput();
    })
    .catch(function (err) { panelStatus("output-status", "✘ " + setErr(err)); });
}

function openDir(cmd, statusId) {
  if (!selected) { panelStatus(statusId, "✘ " + U("set_pick_ws")); return; }
  invoke(cmd, { wsId: selected })
    .then(function () { panelStatus(statusId, ""); })
    .catch(function (err) { panelStatus(statusId, "✘ " + fmt(U("open_dir_fail"), [err])); });
}

/* ---------- 交付目录：生成前由客户确认去向，S5 直接落盘、工作区不留副本 ---------- */
var delivering = false;

function deliveryStateLine(paths) {
  if (!paths) { return ""; }
  var dir = String(paths.delivery_dir || "").trim();
  if (paths.delivery_confirmed) { return "✔ " + fmt(U("delivery_state_ok"), [dir]); }
  if (dir) { return "⚠ " + fmt(U("delivery_state_pending"), [dir]); }
  return "⚠ " + U("delivery_state_none");
}

function confirmDelivery() {
  if (!selected) { panelStatus("output-status", "✘ " + U("set_pick_ws")); return; }
  if (delivering) { return; }
  var dir = $("delivery-dir").value.trim();
  delivering = true;
  $("delivery-go").disabled = true;
  panelStatus("output-status", "⏳ " + U("delivery_busy"));
  var chain = dir ? invoke("delivery_set", { wsId: selected, dir: dir }) : Promise.resolve(null);
  chain
    .then(function () { return invoke("delivery_confirm", { wsId: selected }); })
    .then(function (abs) {
      $("delivery-dir").value = "";
      panelStatus("output-status", "✔ " + fmt(U("delivery_ok"), [abs]));
      return loadOutput();
    })
    .catch(function (err) {
      panelStatus("output-status", "✘ " + setErr(err));
    })
    .then(function () {
      delivering = false;
      $("delivery-go").disabled = false;
    });
}

function initViewTabs() {
  ["flow", "corpus", "output"].forEach(function (k) {
    $("vtab-" + k).onclick = function () { showViewTab(k); };
  });
  $("corpus-add").onclick = addCorpus;
  $("corpus-name").addEventListener("keydown", function (e) {
    if (e.key === "Enter") { addCorpus(); }
  });
  $("corpus-import").onclick = importCorpus;
  $("corpus-path").addEventListener("keydown", function (e) {
    if (e.key === "Enter") { importCorpus(); }
  });
  $("corpus-save").onclick = saveCorpus;
  $("corpus-open").onclick = function () { openDir("open_corpus_dir", "corpus-status"); };
  $("corpus-mlang").onchange = saveModelLang;
  $("output-save").onclick = saveOutput;
  $("output-open").onclick = function () { openDir("open_output_dir", "output-status"); };
  $("delivery-go").onclick = confirmDelivery;
  $("delivery-dir").addEventListener("keydown", function (e) {
    if (e.key === "Enter") { confirmDelivery(); }
  });
}

/* 快速连点工作区时并发请求会交错：序号令牌保证只有最新一次 refresh 落盘渲染。 */
var refreshSeq = 0;

async function refresh() {
  if (!selected) { return; }
  var seq = ++refreshSeq;
  var ws = selected;
  if (ws !== lastWs) { lastWs = ws; resetPanes(); }
  var res = await Promise.all([
    invoke("ws_locale", { wsId: ws }),
    invoke("pipeline_status", { wsId: ws }),
    invoke("history_tail", { wsId: ws, n: 50 })
  ]);
  if (seq !== refreshSeq || ws !== selected) { return; }
  var loc = res[0], st = res[1], events = res[2];
  var next = langPref || (String(loc).trim().toLowerCase() === "en" ? "en" : "zh");
  if (next !== lang) {
    lang = next;
    applyStatic();
    pushMenuLang(next);
    syncPrefs();
  }
  $("empty").classList.add("hidden");
  $("detail").classList.remove("hidden");
  $("ws-title").textContent = ws;
  renderStatus(st);
  renderActions(st);
  paintHistory(events);
  if (view === "corpus") { await loadCorpus(); }
  else if (view === "output") { await loadOutput(); }
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
    need_init: "err_need_init",
    s2_not_approved: "err_s2_not_approved",
    invalid_rel: "err_rel",
    file_too_large: "err_big",
    empty_dest: "err_empty_dest",
    empty_path: "err_empty_path",
    need_abs: "err_need_abs",
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

/* ---------- 语言偏好：菜单栏子菜单驱动，auto 时回落到 workspace.locale ---------- */
function setLangPref(p) {
  if (p === "auto") {
    langPref = null;
    try { localStorage.removeItem("iw-lang"); } catch (e) { /* 隐私模式 */ }
    syncPrefs();
    if (selected) {
      refresh();
    } else {
      lang = "zh";
      applyStatic();
      pushMenuLang(lang);
    }
  } else {
    syncLang(p);
    refresh();
  }
}

function initSettings() {
  $("set-close").onclick = closeSettings;
  $("set-save").onclick = saveModel;
  $("set-discover").onclick = discoverModels;
  $("set-provider").onchange = fillFromProvider;
  $("set-found").onchange = function () { $("set-model").value = this.value; };
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

function menuOpenDir(cmd, doneKey) {
  if (!selected) { menuNote("✘ " + U("set_pick_ws")); return; }
  invoke(cmd, { wsId: selected })
    .then(function () { menuNote("✔ " + U(doneKey)); })
    .catch(function (err) { menuNote("✘ " + fmt(U("open_dir_fail"), [err])); });
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
    else if (a === "lang:auto") { setLangPref("auto"); }
    else if (a === "theme:auto" || a === "theme:light" || a === "theme:dark") {
      setTheme(a.slice("theme:".length));
    }
    else if (a === "open_corpus") { menuOpenDir("open_corpus_dir", "opened_corpus"); }
    else if (a === "open_output") { menuOpenDir("open_output_dir", "opened_output"); }
    else if (a === "delivery") {
      showViewTab("output");
      menuNote("ℹ " + U("delivery_hint"));
      try { $("delivery-dir").focus(); } catch (e) { /* 非 Tauri 环境 */ }
    }
  }).catch(function () {});
}

applyStatic();
syncPrefs();
initAside();
initWsCreate();
initViewTabs();
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
