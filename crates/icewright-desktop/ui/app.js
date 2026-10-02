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
    $("run-meta").textContent = "尚未初始化 pipeline（运行 icewright pipeline init 或 build）";
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
tick();
setInterval(tick, 2000);
