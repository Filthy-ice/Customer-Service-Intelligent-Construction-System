# IceWright · 行业客服系统智能构建器

中文 | [English](README.en.md)

> 把行业规则、流程与需求语料交给它，它构建出一套贴合度 90–100% 的客服系统工程项目——不是拖拽式工作流引擎，而是"骨架模板 + AI 填槽 + 状态机流水线 + 人工闸门"的智能体构建器。

**适用人群**：需要交付客服/理赔/工单类智能坐席系统的企业项目团队。提供 CLI 与桌面客户端两种入口。

## 它做什么

- **S1–S8 分步流水线**：需求摄入 → 环境预检 → 领域提取（规则/流程/数据字典/外部接口/技能绑定五类产物）→ 设计文档（闸门A 人工确认）→ 代码生成 → 自动验证（编译+单测+评测回放）→ 交付报告（闸门B 验收）→ 运行与迭代。
- **契约先行**：所有领域产物均受 JSON Schema（draft 2020-12）约束，跨产物引用做完整性校验（悬空字段/未知流程/未确认接口/失效技能绑定一律拦截）。接口存在性与技能生效均由人工逐条确认（模型一律输出未确认态），未确认接口/技能在 S5 生成阶段被硬闸拒绝；生成项目运行时只装载已确认技能。
- **生成系统不落业务数据**：会话槽位放 Redis，业务数据运行时实时调用客户核心系统 API（接口契约驱动，支持 mock 回放评测）。自带多语言聊天页面（zh/en 访客可选），固定话术与模型回复均跟随所选语言。页面矩阵三栈一致：`/` 开发调试聊天页（回显命中规则与会话槽位）、`/admin` 开发后台（只读规则/技能/接口契约与运行态）、`/console` 业务人员后台（可选，闸门A 开启 `workspace.business_console` 后生成，列出命中拦截/转人工/禁语规则的待关注会话）。
- **三目标栈生成**：`workspace.stack` 支持 `python`（FastAPI）、`java`（Spring Boot 3 / JDK 21）与 `go`（标准库 net/http + go-redis，领域产物 //go:embed 进二进制），三栈 HTTP 线格式一致（同一聊天页与 curl 冒烟通用）、规则/技能语义逐文件对齐；S2 预检探测对应工具链，S6 按栈分派验证（python 编译+pytest / java Maven compile+test / go build+test）。未实现的栈 S5 明确拒绝、绝不冒充交付。
- **生成物配置纪律（init/ 交付物）**：生成系统运行时配置只读 `IW_*` 环境变量——本机用本地 `.env`/yml 占位，有企业配置中心（Nacos/Apollo/Spring Cloud Config/K8s）就托管到中心再注入进程环境，代码零改动；密钥、接入点、库地址永不写死。每次生成统一附带 `init/config.example.env`（全部配置项集中清单与注入方式说明）、`init/schema.sql`（核心系统只读表的示例契约 DDL，联调对账用，绝不执行到客户生产库）与 `init/README.md`（部署步骤）；三栈工程 README 均明文标注"部署前必读：init/"。
- **生成物站在成熟 Agent 框架上，且选型须客户确认**：客服项目不手搓驱动 AI 的框架。设计文档按三层表述——项目基础（Spring Boot/FastAPI/net/http）、模型接入（OpenAI-compatible，java 侧由 Spring AI 承担）、客服 Agent 基础（每栈内置候选与对比表，调研快照 2026-10-01：python 默认 AgentScope、java 默认 Spring AI、go 默认 CloudWeGo Eino）。**Agent 基础框架选型由客户技术侧确认**：`workspace.framework_customer_confirmed = true` 后方可进 S5（硬闸），其余中间件（Redis/DB/页面框架）无需逐项核对；`workspace.agent_framework` 可覆盖候选；模板未落地集成的框架在设计文档如实标注状态。
- **外部依赖与交互契约**：生成系统必然与客户侧其他系统进出双向调用；需求若排除某类设施（如不能用关系库、只能走文件），由客户提供依赖与工具类，未提供时引擎生成默认实现并置于稳定接口之后（integration/ 层端口+适配器形态），客户后续自接不改业务代码；非实时交互按 msgid 幂等合并消息，异步文件经同一 Redis 命名空间键（session+msgid 双前缀、带 TTL）登记文件标识供对方取回，防多会话取混（详见设计文档第 8 节）。
- **可断点续跑**：每个 workspace 独立隔离，状态机可恢复；产物任何变更自动作废已有人工确认，杜绝"批过的设计"与"生成的代码"脱节。
- **生成历史与语料分类**：每次有状态推进（预检、提取、渲染、生成、验证、交付、两道闸门决策）自动追加到 workspace 的 `pipeline/history.jsonl`，`icewright pipeline history` 按时间回看全程；需求语料不必搬运——`icewright corpus add <ws> <文件或目录路径> [--cat 分类]`（桌面端「需求语料」页「从路径导入」）把客户原处材料拷成 `corpus/{apis|flows|dictionary|rules|skills|other}/` 分类快照，原件不动，提取时递归读取；同名同内容不重复入库，同名不同内容默认加 `-2/-3` 后缀共存，加 `--update` 则覆盖旧快照用于客户刷新材料后的 S8 重提取；`icewright corpus list` 核对入库文件与大小。
- **自带模型接入（BYOK）**：任何 OpenAI-compatible 端点均可；供应商目录 + 在线模型发现，接入点与模型名不写死。
- **模型操作语言（与界面语言解耦）**：发给模型的提示词（S3 提取、S6 质检裁判）与生成系统的默认回复/话术语种，由需求摄入时客户显式选定的 `workspace.model_lang` 决定——CLI `icewright corpus lang <ws> zh|en`，桌面端「需求语料」页顶部下拉选择器；管线 init/build 前为硬闸，未选定一律拒绝。它与构建器界面语言 `locale`、模型供应商国籍均无关。
- **双语界面**：构建器 CLI 文案、引擎报错与桌面端界面均支持 zh/en（`workspace.locale` 或 `ICERIGHT_LOCALE`）；两条语言轴独立——`locale` 只管构建器自身界面，模型提示词与生成物默认语言走 `model_lang`。
- **桌面客户端**：`icewright-desktop`（Tauri 2）实时查看各 workspace 的 S1–S8 进度、闸门状态与生成历史，并可在窗口内直接推进：初始化、预检、语料提取、渲染设计、两道闸门决策、生成、验证、交付——与 CLI 走同一状态机路径，决策人记为 `desktop`。危险操作（批准闸门）需二次点击确认。界面文案与引擎报错均中英双语：默认跟随该 workspace 的 `locale`，手动语言/主题偏好跨重启记忆。信息架构：左侧栏只放工作区列表（新建/收起），其余功能入口全部收进原生菜单栏——「文件」（刷新 F5 / 重新加载 Ctrl(Cmd)+R / 打开语料目录 / 打开生成目录 / 交付目录… / 退出）、「设置」（模型设置 `Ctrl/Cmd+,` / 界面语言 / 主题，当前生效项带勾选）、「窗口」「帮助」（关于 / GitHub）。窗口带启动淡入动画，界面深浅色可跟随系统或手动锁定；Linux 下启动时自动注册任务栏图标（不再显示默认齿轮）。模型供应商配置在「设置 → 模型设置…」：可选供应商、在线发现可用模型、写入接入点/模型名/密钥引用（只存 `env://`/`keyring://` 引用，不存密钥本体）。「需求语料」页可在窗口内新建/编辑语料，也支持「从路径导入」：客户文件/目录只给路径（绝对或 `~/` 开头，目录递归收全），拷成工作区快照、原件不动；刷新材料时勾选「覆盖同名（刷新）」即 CLI 的 `--update`。「生成项目」页的交付目录行用于生成前确认去向：S5 直接把工程生成到客户指定目录（可含 `~`），工作区只保留语料、产物、状态与日志等可复用内容，不留生成物副本；目录可继续点开查看与编辑，S8 重生成对定制文件的保护规则不变。

## 安装

```bash
cargo build --release        # 产物 target/release/icewright
cargo build -p icewright-desktop   # 桌面监控端（Linux 需系统依赖：libwebkit2gtk-4.1-dev librsvg2-dev libxdo-dev libssl-dev）
```

### 桌面端安装包（GitHub Release）

打 `v*` 标签自动构建并发布于 [Releases](https://github.com/Filthy-ice/Ice-Wright/releases)：

| 平台 | 产物 | 安装与注意 |
| --- | --- | --- |
| Debian/Ubuntu (amd64) | `.deb` | Ubuntu ≥ 22.04（自带 webkit2gtk 4.1）；`sudo apt install ./xxx.deb` 自动补依赖 |
| Fedora/RHEL/openSUSE | `.rpm` | `sudo dnf install ./xxx.rpm` |
| 其他 Linux | `.AppImage` | `chmod +x` 后运行；系统需 libfuse2 与 webkit2gtk-4.1 |
| Windows 10/11 (x64) | `.msi` / `*-setup.exe` | 未签名，首次运行 SmartScreen 选「更多信息 → 仍要运行」；WebView2 运行时 Win11 内置 |
| macOS (Apple Silicon) | `.dmg` | 未签名，首次右键 → 打开绕过 Gatekeeper |

绕过包管理器裸跑二进制且缺库时，会在程序启动**之前**由动态加载器报终端错误（如 `error while loading shared libraries: libwebkit2gtk-4.1.so.0`），不会有界面内提示——请始终走上表的包管理器安装。CLI 全平台 `cargo build` 可用（HTTPS 走 rustls 纯 Rust 实现，无 OpenSSL 依赖）；Windows/macOS 安装包由 CI 构建验证，日常功能实测以 Linux 为主。

## 快速开始

```bash
icewright ws new my-claim                          # 创建隔离 workspace
icewright corpus add my-claim ./需求语料.md --cat rules   # 客户材料留在原处，按路径导入（目录亦可，递归收全）
icewright corpus lang my-claim zh                  # 模型操作语言须摄入时显式选定（init 前硬闸，zh|en）
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
icewright delivery set my-claim --dir ~/桌面/客服交付 # 生成前由客户规定交付目录（绝对路径或 ~ 开头）
icewright delivery confirm my-claim                # 客户确认去向（硬闸：未确认 S5 拒绝生成）
icewright generate my-claim                        # S5 直接生成到已确认的交付目录（工作区不留副本）
icewright verify my-claim                          # S6 编译+单测（同一交付目录）
icewright evaluate my-claim --url http://127.0.0.1:8000  # 评测回放（红线用例把关）
icewright delivery render my-claim                 # S7 交付报告
icewright delivery approve my-claim --by 张三       # 闸门B
```

需求变更后走 S8 增量重生成：刷新语料（客户改材料后 `icewright corpus add <ws> <路径> --update` 覆盖同名旧快照，桌面端勾选「覆盖同名（刷新）」；也可直接编辑 `corpus/` 内文件，`corpus list` 核对入库状态）→ `pipeline extract --kinds <受影响类型>` → `design render`（旧闸门自动作废）→ 重新过闸门A → `build`/`generate`。引擎按 `ICEWRIGHT-MANIFEST.json` 追踪其托管文件，输出文件级 diff（新增/更新/未变/移除；移除仅报告不删除）；带 `# ICEWRIGHT-CUSTOM` 标记的用户定制文件永不被覆盖。

## 配置

每个 workspace 根目录下的 `icewright.toml` 是生效配置（TOML，Rust 生态通用格式），全部键可用 `icewright config set <ws> <点号键> <值>` 写入：

```toml
[workspace]
name = "my-claim"
pack = "insurance/auto-claim@0.1.0"   # 行业包
stack = "python"                       # python | java | go（三栈线格式一致）
locale = "zh"                          # 构建器界面语言：zh | en（见下"界面语言"）
model_lang = "zh"                      # 模型操作语言：摄入时选定，见下"模型操作语言"

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

构建器 CLI 输出文案支持中英切换，优先级：环境变量 `ICERIGHT_LOCALE`（临时覆盖）> `workspace.locale`（持久，默认 zh）；`icewright config set <ws> workspace.locale en` 即时生效（该条输出即用新语言）。clap 帮助文案同样双语——`--help` 在参数解析前打印，故帮助语言只看 `ICERIGHT_LOCALE=en`；英文表与命令树由单测双向对账，漏译即 CI 失败。引擎内部报错同样双语：CLI 在 `ICERIGHT_LOCALE` / `workspace.locale` 切换时一并生效，桌面客户端每次操作前按 workspace 语种同步；英文表与源码取词点由单测双向对账，漏译即 CI 失败。注意：`locale` 只管构建器自身界面，不决定发给模型的提示词语言。

### 模型操作语言（zh / en）

`workspace.model_lang` 决定两件事：① S3 领域提取与 S6 质检裁判发给模型的全部提示词语种；② 生成客服系统的默认回复/话术语种（访客仍可在聊天页右上角切换 zh/en，未知语言回退该默认值）。由客户在需求摄入时显式选定：CLI `icewright corpus lang <ws> zh|en`（不带值则查看当前值），桌面端在「需求语料」页顶部下拉选择器即时保存。`pipeline init` 与 `build` 处为硬闸——未选定一律拒绝，不给隐式默认。它与供应商国籍无关（国产模型对外国团队、海外模型对中国团队都按操作者语言出稿），也与 `locale` 解耦（可以英文界面操作、中文提示词出稿）。`model_lang` 计入 S5 输入哈希，改动即自动作废下游闸门、触发重生成。

### 模型接入与密钥（BYOK）

**这个密钥是构建器（本软件）调模型用的，不是生成出来的客服系统的运行密钥**——生成系统的模型配置在产出工程的 `.env.example` 与 `init/config.example.env` 中另行提供（只收环境变量引用，见上条配置纪律）。

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

- 目录中的 `base_url` 只是官方文档当前值；`discover`/`probe` 每次都实连端点，以线上实况为准。- **自部署/私有网关/接入点变更**：编辑 `~/.icewright/providers.json`（JSON）同名覆盖或新增条目：

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

## 贡献

新增或修订行业包（规则基线、词典、流程模板、评测套件）请先读 [CONTRIBUTING.md](CONTRIBUTING.md)——
包与引擎只通过 JSON Schema 契约交流，提交前需过双通道契约自检与 CI 四项门禁。

## 许可证

Apache-2.0
