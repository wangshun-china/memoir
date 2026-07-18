# 生产部署

生产目录：

```text
/opt/memoir/deploy/
  docker-compose.yml
  deploy.sh
  .env
  .release.env
```

- `.env`：GitHub Actions 根据 Secrets 生成的运行时配置。
- `.release.env`：当前不可变镜像标签。
- 服务器不保存源码，也不编译 Rust。
- 发布镜像同时写入 GHCR 和阿里云 ACR；部署优先使用 GHCR，拉取失败时回退 ACR。

手动检查：

```bash
cd /opt/memoir/deploy
docker compose --env-file .env --env-file .release.env ps
curl -fsS http://172.17.0.1:18080/health
```

公网链路：

```text
http://api.wangshun.work
  -> shared nginx gateway
  -> host port 18080
  -> memoir-api:8080
  -> memoir-postgres:5432
```

生产 PostgreSQL 不映射宿主机端口。API 只绑定 Docker 宿主网关
`172.17.0.1:18080`，供共享 nginx 通过 `host.docker.internal` 转发，
不直接绑定服务器公网网卡。
