# 后端开发规则

先按下表确定代码归属，再看目标 crate 的 README 和现有入口。函数拆分、命名、导入和注释形式由实现者决定，遵循目标模块的约定。用户明确要求时才使用浏览器或 Computer Use。

## 代码落点

| 要实现的逻辑                                   | 放置位置                                                                                                     |
| ---------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| 业务实体、值对象、输入规范化、状态规则和不变式 | 所属领域的 `entity/`                                                                                         |
| 单域查询和写入用例                             | 所属领域的 `service/`                                                                                        |
| 集合访问、过滤和分页、索引定义                 | 所属领域的 `repository/` 和 `indexes`。应用只调用公开索引入口                                                |
| 跨域写入、审批后的业务动作、事务编排           | `crates/orchestration/` 下负责该业务的 Process                                                               |
| 跨域查询、报表、工作台投影                     | `crates/read-models/erp-read-models/`                                                                        |
| 外域事实或外部能力接口                         | 消费方的 `ports/`。跨域实现在编排层或应用组合层接入，外部协议适配放 `crates/adapters/`                       |
| HTTP 参数提取、路由、响应、错误映射            | `apps/erp-server/src/http/`。复用用例所在层的 DTO，数据库访问经业务入口                                      |
| 共享资源和工厂、HTTP 专属状态、常驻任务        | 分别放 `apps/erp-server/src/wiring/`、`http/`、`worker/`。HTTP 和 worker 各自调用组合层                      |
| 共享类型与基础合同、通用技术组件               | 分别放 `crates/foundation/`、`crates/platform/`。ERP 审批政策和工作项归 `erp-workflow`，通用流程计算归 `bpm` |

业务领域可直接使用 `erp-identity` 的公开接口。其他领域间通过 Port 或显式传入的事实协作。ReadModel 可被编排库调用，编排库之间只允许已登记的单向依赖。新增或迁移 crate 按 [Crate 指南](crates/README.md#依赖与接入要求) 更新登记和入口文档。

应用启动、配置和 CLI 用法见 [后端 README](README.md) 与 [服务端 README](apps/erp-server/README.md)。

## 授权和事务

- 公共权限定义归 `erp-permissions`，权限判断归 `erp-identity`，Facts 与 Mapping 由 `wiring::authorization::build` 装配。管理端路由统一做 JWT 认证，`permission_route` 只登记目录元数据。受保护用例通过共享 `AuthorizationService` 执行 `require` 或 `query`，查询结果交付前复验凭证。HTTP 可通过 `has_permission` 判断按钮或可选字段的功能资格，业务用例负责完整授权。见 [授权 API](crates/domains/erp-identity/README.md#业务授权-api) 和 [权限目录](crates/foundation/erp-permissions/README.md)。
- Service 或 Process 为原子写入开启事务，Repository、Port 和业务步骤复用当前 `Executor`。事务内完成决定写入的授权与数据校验。外部 HTTP、S3、通知放在事务外，需要与写入一致时使用 outbox。提交结果未知时由调用方使用原幂等键重试。
- ReadModel 或本域查询用例可建立只读事务来取得一致快照，交付前按用例要求重新读取并复验。同一读取阶段的授权、取数和凭证校验复用原执行器。执行器选择见 [持久化合同](crates/foundation/persistence-core/README.md#读取边界)。
- 变更持久化字段或索引时处理旧数据与迁移回滚。列表、统计和导出共用授权和筛选口径。关键修改和权限拒绝按 [审计合同](docs/audit-logging.md) 记录。

## 验证

在 `backend/` 执行。仅修改文档、注释或配置示例时，只运行 `git diff --check`。日常代码改动检查受影响的 crate，工作区全量检查由 CI 执行。K8s 流水线通过 `RUN_QUALITY_CHECKS` 启用全量检查。

```bash
cargo fmt --all -- --check
cargo clippy -p <crate> --lib --bins --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib --locked
git diff --check
```

库单测覆盖改动的生产行为及相关失败边界，测试模块可随源码内联或拆文件。禁止新增、修改或运行集成测试，禁止运行历史验收例程、`--include-ignored` 或依赖真实 MongoDB 和外部服务的测试。

| 改动范围                                                                         | 追加检查                                                                                                                                  |
| -------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| 公开类型、trait 或函数签名                                                       | 对直接下游的库和二进制运行 `cargo check -p <downstream> --lib --bins --locked`                                                            |
| Crate 登记、依赖或跨层数据访问                                                   | `./scripts/check-domain-boundaries.sh --cutover`                                                                                          |
| `bpm` 或 `erp-workflow`                                                          | `./scripts/check-bpm-boundaries.sh`                                                                                                       |
| `erp-permissions`、HTTP 路由、路由说明或展示覆盖、`audit_actions.rs`、权限生成器 | 用 `python3 scripts/generate-permissions.py` 同步 `erp-client/lib/` 中的权限和审计目录生成物，运行 `./scripts/check-permissions-drift.sh` |
