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
      no_history: "暂无历史记录",
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
      no_history: "No history yet",
    },
  },
};

var lang = "zh";
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
}

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
    cell.innerHTML = "<b>" + esc(s.id) + "</b><i>" + esc(s.id.replace("S", "")) + "</i>";
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
  var next = String(loc).trim().toLowerCase() === "en" ? "en" : "zh";
  if (next !== lang) {
    lang = next;
    applyStatic();
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

invoke("app_version").then(function (v) { $("ver").textContent = "v" + v; });
initActions();
tick();
setInterval(tick, 2000);
