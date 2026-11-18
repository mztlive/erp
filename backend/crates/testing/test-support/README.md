# test-support：库测试辅助与历史数据库夹具

## 职责

`test-support` 提供可跨 crate 复用的测试辅助，不承载生产逻辑。新测试放在拥有被测行为的 crate 中，以不依赖真实外部服务的库单元测试为限。

### 负责的数据与能力

- **无外部 I/O 的单测助手**：`matches_filter` 用固定 JSON 样本在内存中求值 BSON 过滤条件，只支持字段等值、`$and`、`$or`、布尔 `$expr` 和单键 `$exists`、`$in`；求值遇到不支持的操作时中止测试。它不是 MongoDB 模拟器，不能验证授权政策、查询计划或事务行为。`mint_jwt` 在本地签发与种子管理员身份对应的测试 JWT，不查库、不创建账号，也不授予权限。
- **进程内 HTTP 辅助**：`TestApi` 通过 `tower::ServiceExt::oneshot` 调用已装配的 `axum::Router<()>`，不绑定端口、不启动服务。`get`、`post` 和通用 `request` 都可附带 Bearer 令牌并返回状态码与 JSON，`post` 和 `request` 还支持 JSON 请求体；非 JSON 正文返回 `Value::Null`，响应体读取上限为 8 MiB。路由及中间件仍会执行，能否用于库单测取决于它们是否依赖真实外部服务。
- **历史 MongoDB 夹具**：`TestDb` 直连 MongoDB，以随机库名隔离用例并创建 `_fixture` 标记集合；`cleanup` 等待删库并返回结果，析构只尝试后台清理。`seed_admin_account` 写入 `accounts`、`roles`、`casbin_rules`，返回账号 ID；角色只包含 `role`、`admin`、`audit_log` 的 `list` 权限，种子口令不是可用于登录的哈希。`assert_indexes` 向数据库查询索引名称，不检查索引键、唯一性或查询是否使用索引。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | crate 依赖和 `autotests = false`。 |
| [src/lib.rs](src/lib.rs)、[src/error.rs](src/error.rs) | 公开导出、MongoDB 环境门控及共享 `Error`／`Result`。 |
| [src/filter.rs](src/filter.rs)、[src/jwt.rs](src/jwt.rs) | 内存过滤条件求值、测试 JWT 签发及对应库单测。 |
| [src/api.rs](src/api.rs) | 接受调用方路由的进程内 HTTP 客户端。 |
| [src/db.rs](src/db.rs)、[src/seed.rs](src/seed.rs)、[src/indexes.rs](src/indexes.rs) | 历史测试数据库管理、账号种子和索引名称断言。 |
| [tests/smoke.rs](tests/smoke.rs) | 历史集成测试源码，包含进程内路由示例和真实 MongoDB 夹具用例；未登记为测试目标，不运行。 |

## 架构设计

### 使用场景

优先在消费方的库单测中用 `matches_filter` 检查过滤条件或用 `mint_jwt` 构造测试令牌。需要验证路由行为时，只向 `TestApi` 传入不依赖真实外部服务的路由；已有数据库夹具和验收例程仅供查阅，不作为新测试范本。

### 协作示例

- [可售 SKU 仓储库单测](../../read-models/erp-read-models/src/catalog_supply/repository/sellable.rs) 先断言完整 BSON，再用 `matches_filter` 验证其支持的 `$or` 条件，确认 SKU 与修订 ID 必须成对匹配。
- [历史 smoke 文件](tests/smoke.rs) 用本地 `/ping` 和 `/echo` 路由展示 `TestApi`；[历史 S2 HTTP 验收例程](../../../apps/erp-server/examples/s2_http_acceptance.rs) 则使用 `seed_admin_account` 写入真实 MongoDB，并自行启动环回监听，不是 `TestApi` 的用法。
- [结算仓储的历史库测试](../../orchestration/erp-supply-settlement/src/supply_settlement/repository/persistence_tests.rs) 以 `#[ignore]` 和 `require_mongo!` 双重门控调用 `TestDb`。`mongo_env_present` 和宏只检查 `ERP_TEST_MONGO_URI` 是否存在且非空白，不探测连通性或验证副本集；缺少配置时宏打印提示并从当前测试函数返回。

### 使用与边界要求

1. 消费方只能在 `[dev-dependencies]` 声明 `test-support = { workspace = true }`，例如 [erp-read-models 的清单](../../read-models/erp-read-models/Cargo.toml)。本 crate 不进入生产依赖链，也不得反向依赖 `erp-server`（包含 `web-api`）或历史 `database`、`services` crate，以免形成依赖环。
2. 路由及其状态由调用方装配；历史夹具直接使用 `mongodb` 驱动，种子角色使用 `erp-identity` 类型。`mint_jwt` 与种子共用登录名派生规则和账号版本，签名与声明须和[服务端 JWT 实现](../../../apps/erp-server/src/http/auth/jwt.rs)保持同步。它签发后台管理员令牌，采用 HMAC-SHA256；密钥至少 32 字节，用于路由测试时须与测试应用的 `app.secret` 一致。
3. 按[后端开发规则](../../../AGENTS.md#验证)，不得新增、修改或运行集成测试，不得执行历史验收例程、`--include-ignored`，或依赖真实 MongoDB、S3 等外部服务的测试。真实 MongoDB 单节点副本集及 `ERP_TEST_MONGO_URI` 只是历史夹具的前置条件，不是运行许可。保留 `autotests = false`，不得显式登记或恢复 `tests/smoke.rs` 等历史测试目标。

## 验证要求

仅修改本文档时，在 `backend/` 下只运行 `git diff --check -- crates/testing/test-support/README.md`。代码变更遵循[统一质量门禁](../../README.md#质量门禁)，本 crate 的定向库单测命令为 `env -u ERP_TEST_MONGO_URI cargo test -p test-support --lib --locked`，不包含历史集成测试或外部服务验收。
