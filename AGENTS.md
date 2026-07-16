# Agent instructions — memoir

## Infrastructure rules (hard)

1. **Never install PostgreSQL/MySQL/Redis on the Windows host.** Use **WSL Docker** only.
2. Local DB: `docker compose -f docker-compose.dev.yml --env-file .env.development up -d` (port **5433**).
3. Prefer scripts: `scripts/dev-up.sh`, `scripts/dev-test.sh`, `scripts/dev-down.sh` (run in WSL).
4. Production stack lives under `deploy/`; reverse proxy snippet is `deploy/nginx-api.wangshun.work.conf`.
5. **Do not commit secrets.** No SSH passwords, JWT, AppSecret, LLM keys in git. Use `.env` (gitignored) or CI secrets.
6. Deployment patterns: follow `G:\project\template` (compose for infra, nginx host-based routing). Do not add Redis/Nacos/Kafka unless Stage requirements demand them.
7. Public API target: `http://api.wangshun.work` → host port **18080** (memoir-api). Stage 1 may stay HTTP until TLS is added on the shared gateway.

## App rules

- Stage scope follows `老人回忆录小程序_MVP设计与开发规格.md` §15 phases.
- Miniprogram must not embed secrets; only `API_BASE_URL` in `miniprogram/config/env.ts`.
