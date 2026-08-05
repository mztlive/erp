# 权限元数据生成说明

管理端权限元数据由后端 handler 声明生成，Casbin 负责运行时授权判定。

## 数据来源

- admin handler 上的 `#[permission_macros::permission(...)]`
- `apps/web-api/src/core/routes/admin.rs` 中的路由注册

编译 `web-api` 时，`apps/web-api/build.rs` 会生成：

- `fronts/admin/src/constants/permissions.generated.ts`

生成文件只用于前端菜单和权限勾选；运行时由 `with_permission` 中间件调用
Casbin，根据当前主体、`resource` 和 `action` 判定。

## 注解约束

```rust
#[permission_macros::permission(
    group = "账号管理",
    group_desc = "管理员账号的增删改查",
    desc = "创建管理员",
    resource = "admin",
    action = "create",
)]
pub async fn create_admin(...) -> Result<()> {
    // ...
}
```

- `resource` 与 `action` 必填。
- handler 必须注册到 admin 路由。
- admin 路由必须通过 `with_permission` 绑定该 handler 生成的
  `*_permission_key()`。
- 路由路径必须是字符串字面量。

## 生成方式

```bash
cargo check -p web-api
```

不要手工编辑 `permissions.generated.ts`。新增或修改 admin 路由后，应重新执行
生成命令并提交生成结果。
