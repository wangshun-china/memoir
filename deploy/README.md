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

手动检查：

```bash
cd /opt/memoir/deploy
docker compose --env-file .env --env-file .release.env ps
curl -fsS http://127.0.0.1:18080/health
```

公网链路：

```text
https://api.wangshun.work
  -> shared nginx gateway
  -> host port 18080
  -> memoir-api:8080
  -> memoir-postgres:5432
```

生产 PostgreSQL 不映射宿主机端口。`18080` 仅用于共享 nginx 网关转发，
应通过云防火墙限制直接公网访问。
