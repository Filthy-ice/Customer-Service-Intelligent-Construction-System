# IceWright · An AI Builder for Industry Customer-Service Systems

[中文](README.md) | English

> Hand it your industry rules, processes, and requirement corpus, and it builds a customer-service system project that fits 90–100% of the brief — not a drag-and-drop workflow engine, but an agent builder of "skeleton templates + AI slot-filling + state-machine pipeline + human gates".

**Who it's for**: enterprise delivery teams building customer-service / claims / ticketing intelligent agents. Two front doors: a CLI and a desktop client.

## What it does

- **S1–S8 staged pipeline**: requirement intake → environment preflight → domain extraction (five artifact kinds: rules / flows / data dictionary / external APIs / skill bindings) → design document (Gate A human approval) → code generation → automated verification (compile + unit tests + evaluation replay) → delivery report (Gate B acceptance) → operate and iterate.
- **Contract-first**: every domain artifact is constrained by JSON Schema (draft 2020-12), and cross-artifact references are checked for integrity (dangling fields, unknown flows, unconfirmed APIs, stale skill bindings are all rejected). API existence and skill activation are confirmed item by item by humans (the model always outputs the unconfirmed state); unconfirmed APIs/skills are hard-gated at S5, and the generated project loads only confirmed skills at runtime.
- **No business data lands in the generated system**: session slots live in Redis; business data is fetched live from the customer's core-system APIs at runtime (contract-driven, with mock replay for evaluation). Ships with a bilingual chat page (visitors pick zh/en; fixed scripts and model replies both follow the choice). The page matrix is identical across all three stacks: `/` developer debug chat (echoes matched rules and session slots), `/admin` developer console (read-only rules/skills/API contracts and runtime state), `/console` business console (optional; generated once `workspace.business_console` is enabled at Gate A; lists sessions flagged by blocking / escalation / forbidden-word rules).
- **Three target stacks**: `workspace.stack` supports `python` (FastAPI), `java` (Spring Boot 3 / JDK 21) and `go` (stdlib net/http + go-redis, domain artifacts embedded via //go:embed). All three share the same HTTP wire format (one chat page and one curl smoke test work against any stack), with rule/skill semantics aligned file by file; S2 probes the matching toolchain, S6 dispatches verification per stack (python compile+pytest / java Maven compile+test / go build+test). Unimplemented stacks are explicitly refused at S5 — never faked as delivered.
- **Configuration discipline for generated projects (the init/ bundle)**: generated systems read runtime configuration only from `IW_*` environment variables — local `.env`/yml as placeholders; with an enterprise config center (Nacos/Apollo/Spring Cloud Config/K8s) put values there and inject them into the process environment, zero code changes. Secrets, endpoints and DB addresses are never hardcoded. Every generation ships `init/config.example.env` (central inventory of all config keys and injection methods), `init/schema.sql` (example contract DDL for the read-only core-system tables, for integration reconciliation — never executed against the customer's production DB) and `init/README.md` (deployment steps); each stack's project README states plainly: "read init/ before deploying".
- **Generated projects stand on mature agent frameworks, and the choice must be customer-confirmed**: no hand-rolled AI-driving frameworks for customer service. The design document speaks in three layers — project foundation (Spring Boot/FastAPI/net/http), model access (OpenAI-compatible; on the java side handled by Spring AI), and customer-service agent foundation (per-stack built-in candidates plus a comparison table; research snapshot 2026-10-01: python defaults to AgentScope, java to Spring AI, go to CloudWeGo Eino). **The agent-framework choice is confirmed by the customer's technical side**: S5 requires `workspace.framework_customer_confirmed = true` (hard gate); other middleware (Redis/DB/page frameworks) needs no item-by-item sign-off; `workspace.agent_framework` can override the candidate; frameworks not yet integrated by the templates are honestly labeled in the design document.
- **External dependencies and interaction contracts**: generated systems inevitably call in and out of other customer-side systems. If the brief excludes some facility (e.g. no relational DB, files only), the customer supplies the dependency and utility classes; otherwise the engine generates a default implementation behind a stable interface (port + adapter in the integration/ layer) so the customer can swap in their own later without touching business code. Non-realtime interactions merge messages idempotently by msgid; async files are registered under the same Redis namespace (session+msgid double prefix, with TTL) so the counterpart fetches the right file and sessions never cross (see design document section 8).
- **Resumable**: each workspace is isolated and its state machine recoverable; any artifact change automatically voids all prior human approvals, so an "approved design" can never drift from "generated code".
- **Generation history and corpus categories**: every state advance (preflight, extraction, render, generate, verify, deliver, both gate decisions) appends to the workspace's `pipeline/history.jsonl`; `icewright pipeline history` replays the whole timeline. Requirement corpus never needs moving — `icewright corpus add <ws> <file-or-dir-path> [--cat category]` (desktop: "Requirement corpus" page → "Import from path") copies the customer's material in place into categorized snapshots under `corpus/{apis|flows|dictionary|rules|skills|other}/`; originals stay untouched, extraction reads recursively. Same name + same content is not duplicated; same name + different content gets a `-2/-3` suffix by default, and `--update` overwrites the old snapshot for S8 re-extraction after the customer refreshes material. `icewright corpus list` audits what landed and at what size.
- **Bring-your-own-key model access (BYOK)**: any OpenAI-compatible endpoint; vendor catalog + online model discovery, endpoints and model names never hardcoded.
- **Model operation language (decoupled from the UI locale)**: the language of prompts sent to the model (S3 extraction, S6 QA judge) and the generated system's default reply/script language are driven by `workspace.model_lang`, explicitly chosen by the customer at requirement intake — CLI `icewright corpus lang <ws> zh|en`, desktop: the dropdown at the top of the "Requirement corpus" page. It is a hard gate before pipeline init/build: unset means refused. It has nothing to do with the builder's UI `locale` or the model vendor's nationality.
- **Bilingual UI**: builder CLI text, engine errors, and the desktop interface all support zh/en (`workspace.locale` or `ICERIGHT_LOCALE`). The two language axes are independent — `locale` governs only the builder's own interface; model prompts and the generated project's default language follow `model_lang`.
- **Desktop client**: `icewright-desktop` (Tauri 2) shows live S1–S8 progress, gate states and generation history per workspace, and drives the pipeline right in the window: init, preflight, corpus extraction, design render, both gate decisions, generate, verify, deliver — the same state-machine path as the CLI, with the decision maker recorded as `desktop`. Dangerous actions (approving a gate) require a double-click confirmation. UI text and engine errors are bilingual: defaulting to the workspace `locale`, with manual language/theme preferences surviving restarts. Information architecture: the left sidebar holds only the workspace list (create/collapse); every other entry lives in the native menu bar — File (refresh F5 / reload Ctrl(Cmd)+R / open corpus dir / open output dir / delivery dir… / quit), Settings (model settings `Ctrl/Cmd+,` / UI language / theme, current choice checkmarked), Window, Help (about / GitHub). The window fades in at startup; light/dark follows the system or locks manually; on Linux the taskbar icon registers itself at startup (no more default gear icon). Model providers are configured under Settings → Model settings…: pick a vendor, discover models online, write endpoint/model/key reference (only `env://`/`keyring://` references are stored — never the secret itself). The "Requirement corpus" page creates/edits corpus in-window and imports from a path: the customer's file/directory is given by path (absolute or `~/`, directories collected recursively) and snapshotted into the workspace, originals untouched; tick "overwrite same names (refresh)" to mirror the CLI's `--update`. The delivery-directory row on the "Generated project" page confirms the destination before generation: S5 writes the project straight into the customer-specified directory (may contain `~`), and the workspace keeps only reusable content — corpus, artifacts, state, logs — with no copy of the output; the directory stays browsable/editable, and S8's protection rules for customized files are unchanged.

## Install

```bash
cargo build --release        # CLI lands at target/release/icewright
cargo build -p icewright-desktop   # desktop monitor (Linux system deps: libwebkit2gtk-4.1-dev librsvg2-dev libxdo-dev libssl-dev)
```

### Desktop installers (GitHub Release)

Pushing a `v*` tag auto-builds and publishes to [Releases](https://github.com/Filthy-ice/Ice-Wright/releases):

| Platform | Artifact | Install & notes |
| --- | --- | --- |
| Debian/Ubuntu (amd64) | `.deb` | Ubuntu ≥ 22.04 (ships webkit2gtk 4.1); `sudo apt install ./xxx.deb` pulls deps automatically |
| Fedora/RHEL/openSUSE | `.rpm` | `sudo dnf install ./xxx.rpm` |
| Other Linux | `.AppImage` | `chmod +x` and run; system needs libfuse2 and webkit2gtk-4.1 |
| Windows 10/11 (x64) | `.msi` / `*-setup.exe` | Unsigned — on first run choose "More info → Run anyway" in SmartScreen; Win11 ships the WebView2 runtime |
| macOS (Apple Silicon) | `.dmg` | Unsigned — first launch: right-click → Open to bypass Gatekeeper |

Running the bare binary without a package manager and missing libraries fails in the terminal **before** the app starts (e.g. `error while loading shared libraries: libwebkit2gtk-4.1.so.0`) — there is no in-window hint; always install via the table above. The CLI builds and runs on every platform via `cargo build` (HTTPS goes through pure-Rust rustls, no OpenSSL dependency); Windows/macOS installers are build-verified in CI, while day-to-day functional testing is done mainly on Linux.

## Quick start

```bash
icewright ws new my-claim                          # create an isolated workspace
icewright corpus add my-claim ./claims-requirements.md --cat rules   # customer material stays in place; import by path (directories work too, collected recursively)
icewright corpus lang my-claim zh                  # model operation language: chosen explicitly at intake (hard gate before init, zh|en)
icewright model use my-claim deepseek              # one-shot endpoint/model/key-reference setup
icewright model probe my-claim                     # verify endpoint + key + model as a trio
icewright config set my-claim workspace.pack "insurance/auto-claim@0.1.0"
icewright pipeline init my-claim
icewright build my-claim                           # one-shot: S2 → Gates A/B in order, pauses at gates for approval, re-run to continue
# The steps below are equivalent to build and can be run (or re-run) individually:
icewright pipeline preflight my-claim              # S2 preflight
icewright pipeline extract my-claim                # S3 extract all five artifact kinds (validate-repair loop; --kinds for a single kind)
icewright design render my-claim                   # S4 design document
icewright design approve my-claim --by Jane         # Gate A
icewright delivery set my-claim --dir ~/delivery/cs # customer specifies the delivery directory before generation (absolute or ~-prefixed)
icewright delivery confirm my-claim                # customer confirms the destination (hard gate: S5 refuses without it)
icewright generate my-claim                        # S5 generates straight into the confirmed delivery directory (no copy kept in the workspace)
icewright verify my-claim                          # S6 compile + unit tests (same delivery directory)
icewright evaluate my-claim --url http://127.0.0.1:8000  # evaluation replay (red-line cases enforced)
icewright delivery render my-claim                 # S7 delivery report
icewright delivery approve my-claim --by Jane       # Gate B
```

After a requirement change, go through S8 incremental regeneration: refresh the corpus (after the customer edits material, `icewright corpus add <ws> <path> --update` overwrites the same-named snapshot; on desktop tick "overwrite same names (refresh)"; or edit files under `corpus/` directly and audit with `corpus list`) → `pipeline extract --kinds <affected kinds>` → `design render` (prior gate approvals are automatically voided) → re-approve Gate A → `build`/`generate`. The engine tracks its managed files via `ICEWRIGHT-MANIFEST.json` and emits a file-level diff (created/updated/unchanged/removed; removals are reported, never deleted); files marked `# ICEWRIGHT-CUSTOM` are user customizations and are never overwritten.

## Configuration

The `icewright.toml` at each workspace root is the effective configuration (TOML, the common format in the Rust ecosystem); every key can be written with `icewright config set <ws> <dotted.key> <value>`:

```toml
[workspace]
name = "my-claim"
pack = "insurance/auto-claim@0.1.0"   # industry pack
stack = "python"                       # python | java | go (identical wire format)
locale = "zh"                          # builder UI language: zh | en (see "UI language" below)
model_lang = "zh"                      # model operation language, chosen at intake (see below)

[model]
base_url = "https://api.deepseek.com/v1"
model = "deepseek-chat"
key_ref = "env://DEEPSEEK_API_KEY"     # see "Model access & keys (BYOK)" below

[model.routing]
# extract = "strong-model"             # per-stage model routing
# script  = "cheap-model"

#[datasource.redis]                    # probed by preflight only once host is set
#host = "127.0.0.1"
#port = 6379
#key_ref = "keyring://my-claim/redis"
```

### UI language (zh / en)

Builder CLI output switches between Chinese and English by priority: environment variable `ICERIGHT_LOCALE` (temporary override) > `workspace.locale` (persistent, default zh); `icewright config set <ws> workspace.locale en` takes effect immediately (that very command's output already uses the new language). clap help text is bilingual too — `--help` prints before argument parsing, so the help language follows only `ICERIGHT_LOCALE=en`; the English tables are reconciled bidirectionally against the command tree by unit tests — a missing translation fails CI. Engine errors are bilingual the same way: toggling `ICERIGHT_LOCALE` / `workspace.locale` switches them on the CLI, and the desktop client syncs to the workspace language before every action; English tables vs. source call sites are reconciled by tests, missing translation fails CI. Note: `locale` governs only the builder's own interface — it does not decide the language of prompts sent to the model.

### Model operation language (zh / en)

`workspace.model_lang` drives two things: (1) the language of every prompt sent to the model during S3 extraction and S6 QA judging; (2) the generated customer-service system's default reply/script language (visitors can still toggle zh/en in the top-right of the chat page; unknown languages fall back to this default). The customer picks it explicitly at requirement intake: CLI `icewright corpus lang <ws> zh|en` (omit the value to view the current one); desktop: the dropdown at the top of the "Requirement corpus" page saves instantly. It is a hard gate at `pipeline init` and `build` — unset is always refused, no implicit default. It is unrelated to the vendor's nationality (a Chinese model serves a foreign team in English; an overseas model serves a Chinese team in Chinese) and decoupled from `locale` (you can drive the UI in English while prompting the model in Chinese). `model_lang` feeds the S5 input hash — changing it voids downstream gates and triggers regeneration.

### Model access & keys (BYOK)

**This key is what the builder (this software) uses to call the model — not the runtime key of the generated customer-service system.** The generated system's model configuration ships separately in the produced project's `.env.example` and `init/config.example.env` (environment-variable references only, per the configuration discipline above).

`model.key_ref` supports three reference formats:

| Format | Scenario | Notes |
|---|---|---|
| `keyring://<service>/<account>` | managed by the builder | `icewright secret set keyring://...` (secret via stdin), stored as a 0600 file under `~/.icewright/secrets/`; a native OS-keyring backend is on the roadmap |
| `env://VAR_NAME` | user-managed environment variable | read-only, never taken over, e.g. `env://OPENAI_API_KEY` |
| `plain:<literal>` | literal in config | the desktop "show key" input scenario; every echo/log is masked, the file must stay 0600, and the design document flags it ⚠ with a recommendation to switch to a reference |

Environment variables can also be configured by the builder on the user's behalf (choosing "set it up for us" instead of editing the shell personally):

```bash
echo 'sk-…' | icewright secret set-env DEEPSEEK_API_KEY
# writes ~/.icewright/env/icewright.env (0600); takes effect once sourced
# --shell-profile ~/.bashrc also appends (original file backed up automatically)
```

### Endpoints are never hardcoded

```bash
icewright model providers                # built-in catalog: deepseek/openai/moonshot/zhipu/dashscope/ollama
icewright model discover deepseek        # live GET {base}/models — lists model names available right now
icewright model use my-claim deepseek --model <live-name>   # writes it into config
```

- The catalog's `base_url` values are only what official docs currently say; `discover`/`probe` connect to the live endpoint every time and trust reality. **Self-hosted / private gateway / changed endpoints**: edit `~/.icewright/providers.json` (JSON) to override same-named entries or add new ones:

```json
{
  "providers": [
    {
      "name": "deepseek",
      "display": "Company DeepSeek gateway",
      "base_url": "https://llm.corp.internal/v1",
      "docs_url": "https://wiki.corp/llm",
      "default_model": "ds-pro",
      "key_envs": ["CORP_LLM_KEY"]
    }
  ]
}
```

- Endpoints absent from any catalog still work directly: `icewright model discover --url http://10.0.0.9:8000/v1 --key-env MY_KEY`.

## Contributing

Before adding or revising an industry pack (rule baselines, dictionaries, flow templates, evaluation suites), read [CONTRIBUTING.md](CONTRIBUTING.md) — packs talk to the engine only through JSON Schema contracts, and submissions must pass the dual-channel contract self-check and the four CI gates.

## License

Apache-2.0
