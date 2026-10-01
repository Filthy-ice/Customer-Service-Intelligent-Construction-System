# 贡献指南 · 行业包（Industry Pack）

IceWright 的行业知识全部装在**行业包**里：一个 `pack.manifest.json` 加五类目录物料。
引擎与包之间只通过契约（JSON Schema draft 2020-12）交流——包不改引擎代码，引擎不为包写特例。

本文档说明如何新增或修订一个行业包，以及提交前必须通过的校验。

## 1. 行业包的构成

一个包 = 一份清单 + 五个组件目录（路径由清单声明，仓库惯例如下）：

```
<industry>/<pack-name>/
├── pack.manifest.json   # 清单（受 pack.manifest.schema.json 约束）
├── rules/               # 监管基线：rules.*.json（须过 rules.schema）
├── lexicon/             # 领域词典（数据字典素材）
├── flows/               # 会话流程模板（须过 flows.schema）
├── evals/               # 评测用例套件（须过 eval-cases.schema，必选）
└── runtime/             # 运行时配置素材
```

当前引擎版本中，包的语料通过工作区 `corpus/` 目录参与 S3 提取（把包内文档放入
`~/.icewright/workspaces/<ws>/corpus/`），包引用串（如 `insurance/auto-claim@0.1.0`）
记录在流水线状态并出现在设计文档与交付报告中，用于能力边界明示与交付追溯。

## 2. 清单（pack.manifest.json）必填项

`pack.manifest.schema.json` 要求六个顶层字段，且 `additionalProperties: false`——
**新增字段属于契约变更**，必须走契约评审并升版本，不要在 PR 里直接夹带：

| 字段 | 约束 |
|---|---|
| `name` | `<industry>/<pack>` 形态，如 `insurance/auto-claim` |
| `version` | 语义化版本；样例中的 `-draft` 后缀表示未定稿 |
| `target_engine` | 引擎版本区间，如 `>=0.1 <1.0` |
| `tier` | `official` 或 `community`（社区包不得冒充官方包） |
| `scope` | `covers` 至少一项；`excludes` 明示不做的能力（闸门A 会展示这条边界） |
| `components` | 五个目录路径全部声明，缺一个即违反契约 |

可选字段：`inherits`（继承的其他包）、`rule_types_extended`、`changelog`、
`regulatory_review`（法规复核进度，`verified_ratio` 供人工评审填写）。

## 3. id 命名空间与引用完整性

所有跨产物引用使用纯 id 字符串，命名空间固定：

`R-` 规则 · `F-` 流程 · `SK-` 技能 · `API-` 接口 · `FLD-` 字典字段 · `TC-` 评测用例 · `S-` 脚本 · `DOC-` 检查清单

JSON Schema 只保证**形状**；悬空字段、未知流程、未确认接口、失效技能绑定这类
**引用完整性**由引擎在 S3 提取后校验。贡献包时请保证：

- `rules` 中 `conditions.field` 只引用字典里的 `FLD-` id；
- `flows` 的状态转移只回引真实存在的 `R-`/`FLD-` id；
- `evals` 用例的 `rule_refs` 与语义断言 `rubric_ref` 指向包内真实规则 id
  （裁判判据解析不到规则 id 时该断言记 deferred，不会假通过）。

## 4. 评测基线是必选组件

`evals/` 目录必须随包交付，用例按硬度分三档（`hard` / `hard-adversarial` / `soft`）：

- `hard` 用例构成**红线**：评测回放中任何 hard 用例失败都会把 S6 记为
  `eval_failed`，阻断交付报告与闸门B；
- 对抗（`hard-adversarial`）用例应包含诱导违例的话术（如索取"肯定赔"承诺）；
- 语义/裁判断言（`output_not_matches_semantic`、`llm_judge`）由 LLM 裁判判定，
  裁判不可用时如实记 deferred——用例设计时不要把 deferred 当成通过。

每个包至少给出一份可回放基线套件（参考 `contracts/examples/eval.sample.json`）。

## 5. 提交前自检（三条通道都要过）

```bash
# 1) 引擎内自检（Rust 通道，嵌入 schema）
cargo run -p icewright-cli -- contract check

# 2) 独立 Node 通道（ajv@8 + ajv-formats，双通道互验）
cd crates/icewright-artifact/contracts && npm ci && npm test

# 3) 完整门禁（与 CI 一致）
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
```

CI（`.github/workflows/gate.yml`）对每次 push / PR 自动执行同样的四项：
fmt、clippy（deny warnings）、workspace 测试、契约 ajv 复检。

## 6. 新增/修订包的 PR 流程

1. 按第 1 节目录建包，写 `pack.manifest.json` 并过第 5 节自检；
2. 附上至少一份语料样例与评测基线套件；
3. 若涉及契约字段变更：先改 `contracts/*.schema.json` + 样例 + 双通道校验，
   在 PR 描述中写明 breaking/non-breaking 与迁移说明，契约变更需单独评审；
4. `regulatory_review.reviewed_by` 如实填写——未复核的法规条目 `verified_ratio` 保持原值，
   不要为了显得成熟而虚报。

## 7. 边界与禁忌

- 包物料里**不得**出现任何密钥、真实客户数据或个人信息；接口示例一律用 mock 值；
- 包不携带可执行引擎逻辑；脚本类组件只声明 id 与用途，执行体在生成的客服系统中由人工确认后装载；
- `scope.excludes` 写不清的包过不了闸门A——边界含糊是设计上不允许的，不是评审没抓到。
