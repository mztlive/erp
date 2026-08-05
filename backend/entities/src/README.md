# Entities 模块

本目录是领域规则的单一来源。凡是不依赖数据库、网络、文件系统或事务结果的确定性规则，
优先由实体或值对象负责；Service 只保留仓储调用、事务边界与跨聚合编排。

## 模块与职责

| 模块 | 当前职责 |
| --- | --- |
| `account_core.rs` | `AccountCore`、后台 `AccountKind`/`AccountStatus`、账号资料规范化 |
| `role.rs` | `Role` 展示信息、启停和系统角色删除规则 |
| `rbac.rs` | `RoleId`、`RoleIdSet`、`Permission` 值对象 |
| `auth/` | `LoginAccount`、`Secret`、`PasswordVerification` |
| `audit_log.rs` | `AuditLog` 与创建数据 |
| `field_update.rs` | `FieldUpdate<T>` 对“未提供、显式清空、设置值”三态更新建模 |
| `validation.rs` | 被多个领域类型共享的字符串、邮箱和电话规范化 |
| `errors.rs` | 领域校验与业务规则错误 |

## 建模约定

- 持久化实体包含扁平化的 `BaseModel`，并通过 `Entity` 宏实现元数据访问。
- 创建参数使用 `*Data`，更新参数使用 `*Update`；构造和更新方法先完整校验，再修改实体。
- ID、角色集合、权限和登录账号等带不变式的输入使用值对象表达。
- 后台账号主数据统一放在 `AccountCore`。
- Profile 与账号的跨集合一致性、唯一性查询和事务写入仍留在 Service/Repository。
- 公共方法保持最小接口；不要为单一调用方增加只转发字段的包装方法。

## 凭证构造

```rust
use entities::{LoginAccount, Secret};

let account = LoginAccount::new(" example_user ")?;
let secret = Secret::new(account, "example-only-password")?;

assert_eq!(secret.account(), "example_user");
# Ok::<(), entities::Error>(())
```

`Secret` 的密码哈希不可直接读取，`Debug` 输出也会脱敏。密码校验与旧哈希迁移规则见
[`auth/README.md`](auth/README.md)。

## 新增或修改模型

1. 先确定规则是否与 I/O 无关；无关时放入实体或值对象。
2. 在 `new`/`update` 中复用同一规范化规则，避免 Service 私有 helper 漂移。
3. 为正常路径、非法输入和边界字符数补充内联单元测试。
4. 若持久化查询形态变化，同步评估 `database/src/indexes.rs`。
5. 运行 `cargo fmt --all -- --check`、全仓 check、clippy 与 test。
