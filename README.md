# IceWright · 行业客服系统智能构建器

> 把行业规则、流程与需求语料交给它，它构建出一套贴合度 90–100% 的客服系统工程项目——不是拖拽式工作流引擎，而是"骨架模板 + AI 填槽 + 状态机流水线 + 人工闸门"的智能体构建器。

**适用人群**：需要交付客服/理赔/工单类智能坐席系统的企业项目团队。当前提供 CLI，桌面客户端在开发路线上。

## 它做什么

- **S1–S8 分步流水线**：需求摄入 → 环境预检 → 领域提取（规则/流程/数据字典/外部接口/技能绑定五类产物）→ 设计文档（闸门A 人工确认）→ 代码生成 → 自动验证（编译+单测+评测回放）→ 交付报告（闸门B 验收）→ 运行与迭代。
- **契约先行**：所有领域产物均受 JSON Schema（draft 2020-12）约束，跨产物引用做完整性校验（悬空字段/未知流程/未确认接口/失效技能绑定一律拦截）。接口存在性与技能生效均由人工逐条确认（模型一律输出未确认态），未确认接口/技能在 S5 生成阶段被硬闸拒绝；生成项目运行时只装载已确认技能。
- **生成系统不落业务数据**：会话槽位放 Redis，业务数据运行时实时调用客户核心系统 API（接口契约驱动，支持 mock 回放评测）。自带多语言聊天页面（zh/en 访客可选），固定话术与模型回复均跟随所选语言。
- **三目标栈生成**：`workspace.stack` 支持 `python`（FastAPI）、`java`（Spring Boot 3 / JDK 21）与 `go`（标准库 net/http + go-redis，领域产物 //go:embed 进二进制），三栈 HTTP 线格式一致（同一聊天页与 curl 冒烟通用）、规则/技能语义逐文件对齐；S2 预检探测对应工具链，S6 按栈分派验证（python 编译+pytest / java Maven compile+test / go build+test）。未实现的栈 S5 明确拒绝、绝不冒充交付。
- **可断点续跑**：每个 workspace 独立隔离，状态机可恢复；产物任何变更自动作废已有人工确认，杜绝"批过的设计"与"生成的代码"脱节。
- **自带模型接入（BYOK）**：任何 OpenAI-compatible 端点均可；供应商目录 + 在线模型发现，接入点与模型名不写死。
- **双语界面**：构建器 CLI 文案 zh/en 可选（`workspace.locale` 或 `ICERIGHT_LOCALE`）。

## 安装

```bash
cargo build --release        # 产物 target/release/icewright
```

## 快速开始

```bash
icewright ws new my-claim                          # 创建隔离 workspace
cp 需求语料.md ~/.icewright/workspaces/my-claim/corpus/
icewright model use my-claim deepseek              # 一键配置接入点/模型/密钥引用
icewright model probe my-claim                     # 验证端点+密钥+模型三件套
icewright config set my-claim workspace.pack "insurance/auto-claim@0.1.0"
icewright pipeline init my-claim
icewright build my-claim                           # 一键：S2→闸门A/B 顺序推进，闸门处停等确认，确认后重跑续进
# 以下步骤与 build 等价，可单独执行/重跑：
icewright pipeline preflight my-claim              # S2 预检
icewright pipeline extract my-claim                # S3 五类产物提取（校验-修复回环，可 --kinds 增补单类）
icewright design render my-claim                   # S4 设计文档
icewright design approve my-claim --by 张三         # 闸门A
icewright generate my-claim --out ./generated      # S5 生成目标工程
icewright verify my-claim --out ./generated        # S6 编译+单测
icewright evaluate my-claim --url http://127.0.0.1:8000  # 评测回放（红线用例把关）
icewright delivery render my-claim                 # S7 交付报告
icewright delivery approve my-claim --by 张三       # 闸门B
```

## 配置

每个 workspace 根目录下的 `icewright.toml` 是生效配置（TOML，Rust 生态通用格式），全部键可用 `icewright config set <ws> <点号键> <值>` 写入：

```toml
[workspace]
name = "my-claim"
pack = "insurance/auto-claim@0.1.0"   # 行业包
stack = "python"                       # python | java | go（三栈线格式一致）
locale = "zh"                          # CLI 文案语言：zh | en（见下"界面语言"）

[model]
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"
key_ref = "env://DEEPSEEK_API_KEY"     # 见下"模型接入与密钥"

[model.routing]
# extract = "strong-model"             # 分阶段模型路由
# script  = "cheap-model"

#[datasource.redis]                    # 填了 host 才纳入预检
#host = "127.0.0.1"
#port = 6379
#key_ref = "keyring://my-claim/redis"
```

### 界面语言（zh / en）

构建器 CLI 输出文案支持中英切换，优先级：环境变量 `ICERIGHT_LOCALE`（临时覆盖）> `workspace.locale`（持久，默认 zh）；`icewright config set <ws> workspace.locale en` 即时生效（该条输出即用新语言）。生成出来的客服系统前端语言不在此列——它自带聊天页面（`GET /`），访客在页面右上角选择 zh/en，固定话术与模型回复都跟随所选语言，未知语言回退 zh。clap 帮助文案与引擎内部报错目前仍为中文，随桌面客户端阶段一并 i18n。

### 模型接入与密钥（BYOK）

**这个密钥是构建器（本软件）调模型用的，不是生成出来的客服系统的运行密钥**——生成系统的模型配置在产出工程的 `.env.example` 中另行提供。

`model.key_ref` 支持三种引用格式：

| 格式 | 场景 | 说明 |
|---|---|---|
| `keyring://<service>/<account>` | 软件代存 | `icewright secret set keyring://...`（stdin 输入），落 `~/.icewright/secrets/` 0600 文件；系统 keyring 原生后端在路线图上 |
| `env://VAR_NAME` | 用户自配环境变量 | 只读取不接管，如 `env://OPENAI_API_KEY` |
| `plain:<literal>` | 明文配置项 | 桌面客户端"显示密钥"输入框场景；任何回显/日志一律掩码，文件须保持 0600，设计文档会标注 ⚠ 建议改引用 |

环境变量也可以由软件代配（用户选择"帮我们配"而非自己改 shell）：

```bash
echo 'sk-…' | icewright secret set-env DEEPSEEK_API_KEY
# 写入 ~/.icewright/env/icewright.env（0600）；source 即生效
# --shell-profile ~/.bashrc 可同时追加（原文件自动备份）
```

### 接入点不写死

```bash
icewright model providers                # 内置目录：deepseek/openai/moonshot/zhipu/dashscope/ollama
icewright model discover deepseek        # 实时拉取 GET {base}/models，列出当前可用模型名
icewright model use my-claim deepseek --model <在线名称>   # 写入配置
```

- 目录中的 `base_url` 只是官方文档当前值；`discover`/`probe` 每次都实连端点，以线上实况为准。
- **自部署/私有网关/接入点变更**：编辑 `~/.icewright/providers.json`（JSON）同名覆盖或新增条目：

```json
{
  "providers": [
    {
      "name": "deepseek",
      "display": "公司 DeepSeek 网关",
      "base_url": "https://llm.corp.internal/v1",
      "docs_url": "https://wiki.corp/llm",
      "default_model": "ds-pro",
      "key_envs": ["CORP_LLM_KEY"]
    }
  ]
}
```

- 完全没有目录项的端点也可直连：`icewright model discover --url http://10.0.0.9:8000/v1 --key-env MY_KEY`。

## 许可证

Apache-2.0
