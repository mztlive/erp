# ERP CLI

运维命令行，提供管理员维护、人员数据范围迁移和合同模板增量索引登记。业务用例必须复用拥有领域与组合层；数据库连接及事务能力校验必须使用 `persistence-core`，禁止依赖 `web-api`。

## 命令

```bash
# 在 backend 目录下
cargo run -p cli -- init-admin --account admin --name "System Admin"
cargo run -p cli -- reset-password --account admin
cargo run -p cli -- migrate-contract-templates
```

默认读取 `./config.toml`。可用 `--config-path` 覆盖，全局参数可放在子命令前或后。

`migrate-contract-templates` 必须在开放合同申请前执行，复用 `erp-contract::indexes::ensure_templates` 登记模板、主体编号绑定、年度流水及申请记录的索引；不得修改历史合同和已有流水。正常 API 启动也必须完成同一索引登记。迁移与回退执行[合同模板与申请合同](../../../docs/contract-template-contract.md)第 6 节。

密码读取顺序：

1. `--password`
2. 环境变量 `ERP_ADMIN_PASSWORD`
3. 交互式输入（会要求确认一次）

不要把密码写进日志或提交到仓库。脚本场景优先使用环境变量。

`init-admin` 调用 `AdminService::initialize_super_admin`：不存在则创建，已存在则写入
新密码、恢复启用并补绑 `role-root`。

`reset-password` 只改已有管理员的密码，不创建账号、不改角色、不恢复已删除账号。
超级管理员请用本命令重置；已删除账号需改走 `init-admin`。
