# Agent instructions — memoir

## Infrastructure rules (hard)

1. **Never install PostgreSQL/MySQL/Redis on the Windows host.** Use **WSL Docker** only.
2. Local DB: `docker compose -f docker-compose.dev.yml --env-file .env.development up -d` (port **5433**).
3. Prefer unified env switch (WSL): `scripts/miniprogram-env.sh local|remote|stop|status`.
   - `local` = stop remote containers + start local + mini-program local API
   - `remote` = stop local + start remote + mini-program `api.wangshun.work`
   - `stop` = stop both sides
   Wrappers: `dev-up.sh` → local, `dev-down.sh` → stop, `local_dev.sh` → same mapping.
4. Production stack lives under `deploy/`; reverse proxy snippet is `deploy/nginx-api.wangshun.work.conf`.
5. **Do not commit secrets.** No SSH passwords, JWT, AppSecret, LLM keys in git. Use `.env` (gitignored) or CI secrets.
6. Deployment patterns: follow `G:\project\template` (compose for infra, nginx host-based routing). Do not add Redis/Nacos/Kafka unless Stage requirements demand them.
7. Public API target: `http://api.wangshun.work` → host port **18080** (memoir-api). Stage 1 may stay HTTP until TLS is added on the shared gateway.

## App rules

- Stage scope follows `老人回忆录小程序_MVP设计与开发规格.md` §15 phases.
- Miniprogram must not embed secrets; only `API_BASE_URL` in `miniprogram/config/env.ts`.

<!-- gitnexus:start -->
# GitNexus — Code Intelligence

This project is indexed by GitNexus as **memoir** (950 symbols, 1706 relationships, 78 execution flows). Use the GitNexus MCP tools to understand code, assess impact, and navigate safely.

> Index stale? Run `node .gitnexus/run.cjs analyze` from the project root — it auto-selects an available runner. No `.gitnexus/run.cjs` yet? `npx gitnexus analyze` (npm 11 crash → `npm i -g gitnexus`; #1939).

## Always Do

- **MUST run impact analysis before editing any symbol.** Before modifying a function, class, or method, run `impact({target: "symbolName", direction: "upstream"})` and report the blast radius (direct callers, affected processes, risk level) to the user.
- **MUST run `detect_changes()` before committing** to verify your changes only affect expected symbols and execution flows. For regression review, compare against the default branch: `detect_changes({scope: "compare", base_ref: "main"})`.
- **MUST warn the user** if impact analysis returns HIGH or CRITICAL risk before proceeding with edits.
- When exploring unfamiliar code, use `query({search_query: "concept"})` to find execution flows instead of grepping. It returns process-grouped results ranked by relevance.
- When you need full context on a specific symbol — callers, callees, which execution flows it participates in — use `context({name: "symbolName"})`.
- For security review, `explain({target: "fileOrSymbol"})` lists taint findings (source→sink flows; needs `analyze --pdg`).

## Never Do

- NEVER edit a function, class, or method without first running `impact` on it.
- NEVER ignore HIGH or CRITICAL risk warnings from impact analysis.
- NEVER rename symbols with find-and-replace — use `rename` which understands the call graph.
- NEVER commit changes without running `detect_changes()` to check affected scope.

## Resources

| Resource | Use for |
|----------|---------|
| `gitnexus://repo/memoir/context` | Codebase overview, check index freshness |
| `gitnexus://repo/memoir/clusters` | All functional areas |
| `gitnexus://repo/memoir/processes` | All execution flows |
| `gitnexus://repo/memoir/process/{name}` | Step-by-step execution trace |

## CLI

| Task | Read this skill file |
|------|---------------------|
| Understand architecture / "How does X work?" | `.claude/skills/gitnexus/gitnexus-exploring/SKILL.md` |
| Blast radius / "What breaks if I change X?" | `.claude/skills/gitnexus/gitnexus-impact-analysis/SKILL.md` |
| Trace bugs / "Why is X failing?" | `.claude/skills/gitnexus/gitnexus-debugging/SKILL.md` |
| Rename / extract / split / refactor | `.claude/skills/gitnexus/gitnexus-refactoring/SKILL.md` |
| Tools, resources, schema reference | `.claude/skills/gitnexus/gitnexus-guide/SKILL.md` |
| Index, status, clean, wiki CLI commands | `.claude/skills/gitnexus/gitnexus-cli/SKILL.md` |

<!-- gitnexus:end -->
