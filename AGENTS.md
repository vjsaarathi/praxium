## Agent skills

### Issue tracker

Issues and specs for this repo live as GitHub issues. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the five default triage labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repository. Use `GLOSSARY.md` for domain language and `docs/design/core-v1.md` as the accepted product and API contract.

### Core boundary

This repository builds a generic Rust library for schema-governed JSON collections. Keep task semantics, user workflows, UI, client plugin hosting, and runtime wiring outside this crate. Core owns persistence, validation, querying, revisions, and transactions.

Keep the public API deliberate and stable. Before changing schemas, persistence, query behavior, history, or transactions, read `docs/design/core-v1.md` and update it when an accepted decision changes. Record durable architectural choices in `docs/adr/` when they are hard to reverse, surprising without context, and result from a real trade-off.

The public API is synchronous and returns `Result` with the non-exhaustive Praxium error type. Expected failures must not panic; keep error categories/structured fields stable, preserve standard error sources, and do not expose SQLite types. Use `Database` operation groups and typed validated IDs, versions, names, and JSON Pointers. Transaction errors roll back; panics roll back during unwinding and propagate unchanged.

Keep source layered as API, use cases, domain, and private storage. SQLite belongs only in the private adapter. Use public API contract tests against temporary SQLite databases as the primary behavior seam.

Keep modules focused and cohesive; split files when their responsibilities stop fitting together.

## graphify

This project maintains a knowledge graph at `graphify-out/` capturing architecture, god nodes, community structure, and cross-file relationships.

**Agents MUST read and follow the `graphify` skill (`skills/graphify/SKILL.md`)** for codebase navigation and graph maintenance:
- **Codebase Exploration & Questions**: Whenever answering any user question about the codebase (architecture, how components work, data flow, call paths, or dependencies), read the `graphify` skill first. When `graphify-out/graph.json` exists, turn the user's inquiry into a targeted question and run `graphify query "<question>"` before performing any grep, search, or raw source file browsing. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. Read raw source files only for lines identified by the graph. Include this rule in every subagent prompt that explores code.
- **After Modifying Code**: After creating or modifying source files, always run `graphify update .` (AST-only, fast, zero API cost) to keep the knowledge graph synchronized with codebase changes.
- **Slash Command**: When the user invokes `/graphify`, strictly follow the instructions in the `graphify` skill before taking any other action.
- **Working Tree State**: Files in `graphify-out/` are expected to change after incremental updates. Never skip using graphify because files in `graphify-out/` are dirty, and do not treat `graphify-out/graph.json` diffs as churn.
