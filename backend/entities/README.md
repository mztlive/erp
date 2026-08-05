# Entities

`entities` crate 保存不依赖数据库和外部 I/O 的领域模型、值对象、不变式与规范化规则。
Service 负责编排流程，Repository 负责持久化；两层都不应复制实体已经提供的规则。

## 当前模型

- `AccountCore`：后台统一账号，包含 `AccountKind`、`AccountStatus` 和登录凭证。
- `Consumer`：消费者账号及其启用状态、昵称和凭证。
- `Role`：角色展示信息、系统角色和启用状态。
- `RoleId`、`RoleIdSet`、`Permission`：RBAC 输入规范化与校验。
- `LoginAccount`、`Secret`、`PasswordVerification`：认证值对象。
- `AuditLog`：结构化审计记录。

所有持久化实体通过 `#[serde(flatten)]` 包含 `entity_core::BaseModel`，统一保存 `id`、
`version`、`created_at`、`updated_at` 和 `deleted_at`。

## 使用示例

新凭证必须先构造 `LoginAccount`，再传给 `Secret::new`：

```rust
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret,
};

let login = LoginAccount::new(" example_admin ")?;
let secret = Secret::new(login, "example-only-password")?;
let account = AccountCore::new(
    "account-example".to_string(),
    AccountCoreData {
        secret,
        name: "示例管理员".to_string(),
        kind: AccountKind::Admin,
        status: AccountStatus::Active,
        email: None,
        phone: None,
        avatar: None,
    },
)?;
# Ok::<(), entities::Error>(())
```

示例密码只用于展示 API 形态，不应复制到真实配置、测试夹具或生产数据。

更详细的模块边界与扩展约定见 [`src/README.md`](src/README.md)，认证安全语义见
[`src/auth/README.md`](src/auth/README.md)。
