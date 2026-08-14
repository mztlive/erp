# IAM 模块

`services/src/iam` 是管理员账号身份能力与 Casbin RBAC 的统一入口。

## 目录结构

- `account/`：管理员账号、当前账号资料服务与 DTO。
- `dto.rs`：角色请求与响应 DTO。
- `rbac/`：角色 CRUD、角色权限、账号角色绑定、授权判定与启动种子写入。
- `predefined_roles.rs`：业务预定义角色清单与启动编排。
- Casbin policy 的 MongoDB Adapter 位于 `database/src/casbin_adapter.rs`。

## 使用约定

- 角色、权限、绑定和判定统一通过共享 `RbacService`。
- 业务 Service 只编排账号生命周期，并调用 `RbacService` 更新绑定。
- 权限规则使用 `entities::Permission`，持久化格式为 `resource:action`。
- ERP 操作人员账号能力放在 `services::iam::account`。
- 启动时：`ensure_root_role` 保证超级管理员角色；`ensure_predefined_roles` 幂等写入业务岗位角色，并对已有角色补齐当前种子中缺失的权限与空岗位的公司级数据范围（见 `predefined_roles.rs`、`predefined_data_scopes.rs`、`rbac/seed.rs`）。
