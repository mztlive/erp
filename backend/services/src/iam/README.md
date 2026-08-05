# IAM 模块

`services/src/iam` 是管理员账号身份能力与 Casbin RBAC 的统一入口。

## 目录结构

- `account/`：管理员账号、当前账号资料服务与 DTO。
- `dto.rs`：角色请求与响应 DTO。
- `rbac.rs`：角色 CRUD、角色权限、账号角色绑定和授权判定。
- Casbin policy 的 MongoDB Adapter 位于 `database/src/casbin_adapter.rs`。

## 使用约定

- 角色、权限、绑定和判定统一通过共享 `RbacService`。
- 业务 Service 只编排账号生命周期，并调用 `RbacService` 更新绑定。
- 权限规则使用 `entities::Permission`，持久化格式为 `resource:action`。
- 消费者账号能力放在 `services::consumer`，后台管理员账号能力放在 `services::iam::account`。
