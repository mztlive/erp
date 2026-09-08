# test-support：库测试辅助与历史数据库夹具

## 职责合同

- 基于 axum::Router 的 TestApi、JWT 签发、账号种子和索引断言辅助。
- TestDb、MongoDB 环境识别与 require_mongo! 历史夹具。

## 使用与边界要求

1. 仅允许作为消费方 dev-dependency，不得引入生产依赖链。
2. TestApi 使用调用方提供的 Router，不负责启动 HTTP 服务；本 crate 不得依赖 web-api。
3. JWT 辅助须与 web-api 的实际令牌结构和签名合同同步；仅用于测试。
4. TestDb 与 require_mongo! 的存在不构成运行真实数据库测试的授权；按 backend/AGENTS.md，禁止新增、修改或执行集成测试，也不执行 include-ignored。
5. 不得启用 Cargo.toml 中已关闭的自动集成测试发现；新增测试优先放入拥有 crate 的库单元测试。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/api.rs](src/api.rs) | TestApi |
| [src/jwt.rs](src/jwt.rs) | mint_jwt |
| [src/seed.rs](src/seed.rs) | seed_admin_account |
| [src/indexes.rs](src/indexes.rs) | assert_indexes |
| [src/db.rs](src/db.rs) | TestDb 历史夹具 |
| [src/lib.rs](src/lib.rs) | 导出及 require_mongo! 门控 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p test-support --locked
env -u ERP_TEST_MONGO_URI cargo test -p test-support --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
