# 本地 XXL-JOB 执行步骤

## 1. 启动调度中心

在仓库根执行：

```bash
bash scripts/start-xxl-job.sh
```

- Docker 必须连接本机运行时；macOS 使用 Docker Desktop 或 OrbStack。
- 控制台：`http://127.0.0.1:18080/xxl-job-admin/`，账号 `admin`。
- 密码读取 `.local/xxl-job/credentials.env` 的 `XXL_ADMIN_PASSWORD`；该文件首次生成后复用，禁止提交。
- 项目名固定为 `erp-xxl-local`，使用独立 MySQL 数据卷，不连接 ERP 业务数据库或其他 MySQL 容器。
- 控制台仅绑定宿主机回环地址，MySQL 不发布宿主机端口。

## 2. 配置 ERP

启动脚本生成 `.local/xxl-job/erp-config.toml`。将其完整合入本地 API 和 Worker 配置，替换已有 `[xxl_job]` 节；只配置 admin_url、executor_url、app_name、access_token，不配置 enabled 或任务 ID。

API 的 `app.port` 使用 `10001`；Worker 使用 `10002`，并保存为 `backend/config.worker.local.toml`。两个文件必须保持相同业务数据库、业务密钥及供应商配置。Worker 的 `executor_url` 使用 `http://host.docker.internal:10002`，供容器内调度中心连接宿主机。

在 `backend/` 分别启动两个进程：

```bash
cargo run -p web-api --bin web-api -- --config-path ./config.toml
cargo run -p web-api --bin erp-worker -- --config-path ./config.worker.local.toml
```

API 固定不运行后台任务。Worker 必须装配供应商跟踪能力，并加载完整 XXL-JOB 配置；不支持内嵌模式或本地周期调度。

## 3. 执行测试

首次初始化自动建立执行器 `erp-local`，注册方式为自动注册，并创建以下停用任务：

| ID | Handler | 调度配置 |
| --- | --- | --- |
| 1 | `supplier_tracking_scan` | BEAN、FIRST、DISCARD_LATER、空参数、超时 0、失败重试 0 |
| 2 | `supplier_catalog_schedule` | BEAN、FIRST、DISCARD_LATER、空参数、超时 0、失败重试 0 |

1. 等待执行器列表出现 `http://host.docker.internal:10002`；注册和控制台刷新存在时间间隔。
2. 保持定时任务停用，先对目标任务执行一次手动触发。
3. 核对调度日志的接收结果、执行回报和 ERP 对应业务记录。目录任务执行成功仅表示本轮计划生成完成。
4. 确认单次执行结果后，按需要启用 CRON。初始 CRON 为 `0/5 * * * * ?`。

首次初始化后，启动脚本不得重置任务、密码或调度状态。新增、删除或重建控制台任务不要求修改 ERP 配置；使用表中的 JobHandler 即可选择能力。

## 4. 查询与停止

在仓库根执行：

```bash
docker compose --env-file .local/xxl-job/credentials.env -f deploy/local/xxl-job/compose.yaml ps
docker compose --env-file .local/xxl-job/credentials.env -f deploy/local/xxl-job/compose.yaml logs --tail 100 admin
docker compose --env-file .local/xxl-job/credentials.env -f deploy/local/xxl-job/compose.yaml stop
```

停止容器保留数据库和配置。禁止在有待回报任务时删除数据卷；重新启动使用第 1 节命令。

## 5. 上游来源

调度中心使用固定摘要的 XXL-JOB 2.4.1 镜像。`00-schema.sql` 来自 [XXL-JOB 2.4.1 官方建表脚本](https://github.com/xuxueli/xxl-job/blob/2.4.1/doc/db/tables_xxl_job.sql)，保留原始内容；上游 Apache-2.0 许可证见同目录 `LICENSE`。本地账号及任务在独立初始化事务中配置。
