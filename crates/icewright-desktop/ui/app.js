"use strict";
// 与引擎 state.json 的 snake_case 线格式一致；状态枚举见 icewright-core/src/state.rs。
var invoke = window.__TAURI__.core.invoke;

var STATUS_LABEL = {
  pending: "待启动", running: "进行中", waiting_gate: "等闸门确认",
  approved: "已批准", failed: "失败", blocked_preflight: "预检受阻", skipped: "跳过",
};
var GATE_LABEL = { approved: "批准", rejected: "驳回", partial_edit: "部分编辑驳回" };

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
    $("run-meta").textContent = "尚未初始化 pipeline：点下方「初始化」或运行 icewright pipeline init";
    $("stepper").innerHTML = "";
    $("stage-rows").innerHTML = "";
    return;
  }
  $("run-meta").textContent =
    "run " + st.run_id + " · pack " + (st.pack_ref || "-") +
    " · 当前阶段 " + st.current_stage + " · 更新于 " + fmtTs(st.updated_at);
  var step = $("stepper");
  step.innerHTML = "";
  st.stages.forEach(function (s) {
    var cell = document.createElement("div");
    cell.className = "step " + s.status;
    cell.title = STATUS_LABEL[s.status] || s.status;
    cell.innerHTML = "<b>" + esc(s.id) + "</b><i>" + esc(s.id.replace("S", "")) + "</i>";
    step.appendChild(cell);
  });
  var rows = $("stage-rows");
  rows.innerHTML = "";
  st.stages.forEach(function (s) {
    var gate = s.gate
      ? (GATE_LABEL[s.gate.decision] || s.gate.decision) + " · " + s.gate.by
      : "";
    var tr = document.createElement("tr");
    tr.innerHTML =
      "<td>" + esc(s.id) + "</td>" +
      "<td>" + esc(titleOf(s.id)) + "</td>" +
      "<td class='st-" + s.status + "'>" + (STATUS_LABEL[s.status] || s.status) + "</td>" +
      "<td><code>" + shortHash(s.output_hash) + "</code></td>" +
      "<td>" + esc(gate) + "</td>";
    rows.appendChild(tr);
  });
}

/* ---------- 窗口内推进操作 ---------- */
// 按钮可用性只按状态机粗粒度门控；最终裁决在引擎（错误会显示在结果面板）。
var OPS = [
  { op: "init", label: "初始化", need: function (st) { return !st; } },
  { op: "preflight", label: "环境预检", need: function (st) { return !!st; } },
  { op: "design_render", label: "渲染设计", need: function (st) { return statusOf(st, "S3") === "approved"; } },
  { op: "design_approve", label: "批准闸门A", confirm: true, need: function (st) { return statusOf(st, "S4") === "waiting_gate"; } },
  { op: "design_reject", label: "驳回闸门A", note: true, need: function (st) { return statusOf(st, "S4") === "waiting_gate"; } },
  { op: "generate", label: "生成代码", need: function (st) { return statusOf(st, "S4") === "approved"; } },
  { op: "verify", label: "自动验证", need: function (st) { return statusOf(st, "S5") === "approved"; } },
  { op: "delivery_render", label: "渲染交付", need: function (st) { return statusOf(st, "S6") === "approved"; } },
  { op: "delivery_approve", label: "批准闸门B", confirm: true, need: function (st) { return statusOf(st, "S7") === "waiting_gate"; } },
  { op: "delivery_reject", label: "驳回闸门B", note: true, need: function (st) { return statusOf(st, "S7") === "waiting_gate"; } },
];

var busy = false;
var armedOp = null;   // 双击确认：第一次点击挂起，3 秒内再点才执行
var armTimer = null;

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
    btn.textContent = armedOp === spec.op ? "再点一次确认" : spec.label;
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
  var note = $("op-note").value.trim();
  busy = true;
  renderActions(st);
  var res = $("op-result");
  res.classList.remove("hidden");
  res.textContent = "⏳ " + spec.label + " 执行中…";
  try {
    var out = await invoke("run_op", {
      op: spec.op, wsId: selected,
      note: spec.note ? note : null,
    });
    res.textContent = "✔ " + spec.label + "\n" + out;
    $("op-note").value = "";
  } catch (err) {
    res.textContent = "✘ " + spec.label + "\n" + err;
  }
  busy = false;
  await refresh();
}

function initActions() {
  var box = $("op-buttons");
  OPS.forEach(function (spec) {
    var b = document.createElement("button");
    b.id = "btn-" + spec.op;
    b.textContent = spec.label;
    b.className = spec.confirm ? "confirm" : (spec.note ? "danger" : "");
    b.onclick = function () { runOp(spec); };
    box.appendChild(b);
  });
  $("op-note").addEventListener("input", async function () {
    renderActions(await currentStatus());
  });
}

// 阶段中文名与引擎 StageId::title 对齐。
var STAGE_TITLE = {
  S1: "需求摄入", S2: "环境预检", S3: "领域规则提取", S4: "设计文档与闸门A",
  S5: "代码生成", S6: "自动验证", S7: "验收交付与闸门B", S8: "变更/重生成",
};
function titleOf(id) { return STAGE_TITLE[id] || id; }

async function renderHistory() {
  var events = await invoke("history_tail", { wsId: selected, n: 50 });
  var ul = $("history");
  ul.innerHTML = "";
  if (!events.length) {
    ul.innerHTML = "<li class='hint'>暂无历史记录</li>";
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
    $("run-meta").textContent = "刷新失败：" + err;
  }
}

invoke("app_version").then(function (v) { $("ver").textContent = "v" + v; });
initActions();
tick();
setInterval(tick, 2000);
