# 权限目录维护合同

## 权威来源

- `apps/web-api/permission_catalog/` 使用普通 Rust 常量定义权限代码、展示说明和分组。
- 路由通过 `with_permission(route, rbac, permissions::<module>::<ACTION>)` 指定入口权限。
- 构建生成器与运行时共同编译目录；处理函数不承担权限元数据声明。
- 非 HTTP 政策权限由 `erp-identity/src/entity/policy_permission.rs` 定义，构建目录引用该来源。

## 新增或修改接口

1. 在目录的业务模块定义 `PermissionDef`，并将符号名与常量加入该模块 `ENTRIES`。权限代码保持 `resource:action` 格式。
2. 新模块在目录 `mod.rs` 声明，并加入 `MODULES`。新展示分组在 `groups.rs` 定义。
3. 同一权限的复用引用直接引用既有常量；需要独立接口说明时使用 `described(group, description)`，不得重复声明权限代码。
4. 路由使用完整的 `permissions::<module>::<ACTION>` 引用，HTTP 方法和路径保持显式。新管理端路由文件必须登记到 `build_permissions.rs` 的 `DOMAIN_MODULES`。
5. 管理端路由接入统一认证链路；公开入口和已有仅登录目录保留独立认证规则。
6. 运行受影响库测试和生成物漂移门禁；前端生成文件不得手工修改。

```rust
use crate::permissions;

.route(
    "/purchase-orders/{id}/submit",
    with_permission(
        post(purchase_order::purchase_order_submit),
        rbac,
        permissions::purchase_order::SUBMIT,
    ),
)
```

## 复用与独立校验

多个接口可以共享权限代码，但必须保留各自的展示说明和分组。目录代码去重不得合并 HTTP 路由或减少既有权限检查层。

嵌套 `with_permission` 保留全部检查及其顺序。构建期路由合同记录从外到内的 `guards`，目录展示使用最内层定义。

供应商连接治理命令沿用入口 `detail`、目录动作 `command` 的分离：

```rust
permissions::supplier_api::CONNECTION_DETAIL
    .documented_as(permissions::supplier_api::CONNECTION_COMMAND)
```

`documented_as` 只指定目录展示项，不授予权限，不替代业务动作的权限检查。新增此类分离必须明确入口权限及业务层动作校验责任，并补齐行为测试。

## 生成物

编译 `web-api` 时必须生成：

- `erp-client/lib/permissions.generated.ts`：前端展示及权限选择目录，包含 HTTP 方法、路径、分组、说明和权限代码。
- `OUT_DIR/policy_permissions.rs`：授权文件可配置权限白名单。
- `OUT_DIR/permission_routes.json`：路由、处理函数、入口权限检查顺序和目录信息的可比对合同。

未知权限引用、重复模块或符号、非法代码、分组说明冲突、缺少权限绑定、无法解析的受保护路由必须使构建失败。构建失败时不得使用旧生成物发布。

## 验证

在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p web-api --lib --locked
./scripts/check-permissions-drift.sh
```

`apps/web-api/testdata/permission-routes.json` 是迁移前冻结的 678 条 HTTP 权限合同。行为保持型重构必须逐项一致；产品明确修改接口或权限时，应同步更新合同数据并说明变更依据。该合同不替代真实权限中间件行为测试。
