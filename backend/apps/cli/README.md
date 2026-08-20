# ERP CLI

运维命令行，用于初始化超级管理员和重置管理员密码。本 crate 只依赖 `services`、
`database` 与 `config`，禁止依赖 `web-api`。

## 命令

```bash
# 在 backend 目录下
cargo run -p cli -- init-admin --account admin --name "System Admin"
cargo run -p cli -- reset-password --account admin
```

默认读取 `./config.toml`。可用 `--config-path` 覆盖，全局参数可放在子命令前或后。

密码读取顺序：

1. `--password`
2. 环境变量 `ERP_ADMIN_PASSWORD`
3. 交互式输入（会要求确认一次）

不要把密码写进日志或提交到仓库。脚本场景优先使用环境变量。

`init-admin` 调用 `AdminService::initialize_super_admin`：不存在则创建，已存在则写入
新密码、恢复启用并补绑 `role-root`。

`reset-password` 只改已有管理员的密码，不创建账号、不改角色、不恢复已删除账号。
超级管理员请用本命令重置；已删除账号需改走 `init-admin`。
