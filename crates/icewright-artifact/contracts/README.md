# IceWright M0 Contracts (machine-readable)

JSON Schema draft 2020-12. These files are the single shared contract between the engine, industry packs, and the gate review UI. Breaking changes require a version bump and a migration note.

| File | Frozen scope |
|---|---|
| rules.schema.json | Typed rule table (hard/soft rule kinds, enforcement points, verify blocks) |
| flows.schema.json | Conversation flow DSL (slots, states, transitions with rule back-references) |
| api-contract.schema.json | External core-system API contracts (auth refs, fallbacks, mock examples) |
| data-dictionary.schema.json | Field dictionary — the only source rules and flows may reference |
| skills.schema.json | Intent → capability bindings (flow / api_call / script / rag) |
| eval-cases.schema.json | Evaluation cases with hardness tiers (hard / hard-adversarial / soft) |
| pack.manifest.schema.json | Industry pack manifest (scope boundaries, tier, eval suite mandatory) |
| pipeline-state.schema.json | Pipeline state machine persistence (gate records, idempotency hashes, usage) |

Conventions:

- Cross-file references are plain id strings (`rule_ref`, `dict_field`, ...). Reference integrity is enforced by the engine after stage S3 — JSON Schema guarantees shape only.
- `additionalProperties: false` by default: adding a field is a contract change and goes through review.
- Id namespaces: `R-` rules, `S-` scripts, `DOC-` checklists, `F-` flows, `SK-` skills, `API-` contracts, `FLD-` dictionary fields, `TC-` eval cases.

Self-check: `icewright contract check` (also as `cargo test -p icewright-artifact`). `validate.js` is kept as an independent Node channel for re-verification (requires `ajv@8` + `ajv-formats`).
