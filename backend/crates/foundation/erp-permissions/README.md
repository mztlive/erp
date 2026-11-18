# erp-permissions

## 职责

`erp-permissions` 是后端共享的权限定义库。它统一 `resource:action` 权限值、内置常量、资源标识和公共展示元数据，让业务用例与目录生成器使用同一份定义。目录登记说明权限如何展示，不表示权限已授予账号，也不决定账号能操作哪些记录。

本 crate 不依赖 HTTP、数据库或业务领域。运行依赖仅有 `serde` 和 `thiserror`，单元测试使用 `serde_json`（见 [Cargo.toml](Cargo.toml) 和[依赖边界规则](../../../scripts/crate_layers.py)）。应用登记路由及其展示差异，[erp-identity](../../domains/erp-identity/README.md#业务授权-api) 负责判断账号资格、数据范围（DataScope）及对象访问。

## 目录结构

| 源码 | 职责 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 导出公共类型；`catalog_modules!` 同时导出目录模块并收集各模块的 `ENTRIES` 到 `MODULES`。 |
| [src/permission.rs](src/permission.rs) | 定义 `Permission` 和 `ParsePermissionError`，实现静态构造、动态解析、序列化及通配覆盖。 |
| [src/catalog_types.rs](src/catalog_types.rs) | 定义目录模块、权限条目和分组；条目以 `&'static Permission` 引用真实权限值，不另存编码。 |
| [src/resources.rs](src/resources.rs)、[src/groups.rs](src/groups.rs) | 分别定义内置资源标识、公共展示分组及分组说明。 |
| [src/sales_inquiry.rs](src/sales_inquiry.rs) 等业务模块 | 定义并复用权限常量；每个独立目录权限只在 `ENTRIES` 登记一次，附公共分组和默认说明。 |
| [src/policy_permission.rs](src/policy_permission.rs) | 登记无独立 HTTP 路由、但需要进入可配置目录的领域政策资格。 |
| [src/internal.rs](src/internal.rs)、[src/approval_admin.rs](src/approval_admin.rs) | 分别定义内部对象资格、按审批主体类型区分的定义和运行管理资格；两个模块不直接列入 `MODULES`。 |

## 架构设计

### 从权限值到应用目录

1. **定义值与公共元数据。** `resources::SALES_INQUIRY` 是 `sales_inquiry`，[询价模块](src/sales_inquiry.rs) 用它和 `update` 定义 `COMMANDS_SALES_SAVE`。该模块的 `ENTRIES` 用常量符号名索引这一个值，并登记「销售询价」分组及「保存询价需求」说明。符号名供构建期查找；权限身份由值决定，与符号名、分组和说明无关。
2. **在应用中登记路由。** [询价路由](../../../apps/erp-server/src/http/routes/sales_inquiry.rs) 在 `PUT /sales-inquiries/{id}/draft` 上用 `permission_route` 引用该常量。HTTP 方法、路径和处理器来自实际路由；[接口说明差异](../../../apps/erp-server/route_descriptions.rs) 和[少数展示覆盖](../../../apps/erp-server/build_route_overrides.rs) 按方法及不含 `/admin` 的路径补充元数据。没有覆盖时沿用公共目录；覆盖只影响展示，不改变运行时授权。
3. **构建期合成目录。** server 的 [build.rs](../../../apps/erp-server/build.rs) 读取管理端挂载前缀，按[扫描清单](../../../apps/erp-server/build_permissions.rs) 采集路由。[路由解析器](../../../apps/erp-server/build_routes.rs) 用 `syn` 读取显式权限引用，[目录构建逻辑](../../../apps/erp-server/build_catalog.rs) 通过已编译的 `MODULES` 解析 `模块::常量`，合并展示差异，并从真实 `Permission` 拆取资源和动作。缺失或未知的目录绑定、未登记的受保护路由模块、无效的展示覆盖会使生成失败。`policy_permission::FINANCE_LEDGER_READ` 则无需虚构路由：生成器补入 `finance_ledger:read`，标记为 `POLICY`，路径留空。
4. **输出给各消费者。** [客户端权限目录](../../../../erp-client/lib/permissions.generated.ts) 按组保留接口展示项，例如 `PUT /admin/sales-inquiries/{id}/draft` 对应 `sales_inquiry:update`。同一权限可显示在多条接口下；[授权文件校验白名单](../../../apps/erp-server/build_policy_catalog.rs) 按权限编码去重，写为 `OUT_DIR/policy_permissions.rs`，由 [server 的授权文件入口](../../../apps/erp-server/src/http/handler/access_control/policy_bundle.rs) 读取。`OUT_DIR/permission_routes.json` 记录路由、处理器、权限标记和展示信息，供合同对照；`OUT_DIR/permission_references.rs` 供测试核对符号索引与实际常量。构建入口还从 identity 的[审计动作定义](../../domains/erp-identity/src/entity/access_control/audit_actions.rs) 独立生成[客户端审计动作目录](../../../../erp-client/lib/audit-actions.generated.ts)；审计动作不是权限编码。

### 运行时授权

[管理端路由](../../../apps/erp-server/src/http/routes/admin.rs) 统一做 JWT 认证。[`permission_route`](../../../apps/erp-server/src/http/routes/mod.rs) 原样返回路由，不安装中间件、不执行 RBAC，也不把目录项当成授权结果。[server 装配入口](../../../apps/erp-server/src/wiring/authorization/mod.rs) 在启动时按资源绑定业务事实读取器（Facts），按权限绑定查询映射（Mapping），为 HTTP 与 worker 按同一方式构造 [`AuthorizationService`](../../domains/erp-identity/src/service/authorization/mod.rs)；装配不定义授权政策。

受保护的业务用例显式调用 identity 执行固定规则。例如，[保存询价用例](../../orchestration/erp-order-to-cash/src/sales_inquiry/execution/apply.rs) 用同一个 `COMMANDS_SALES_SAVE`，[按实际询价记录调用 `require`](../../orchestration/erp-order-to-cash/src/sales_inquiry/authorization.rs)，在写入事务内检查资格和对象范围。列表、统计等通过 `query` 编译完整授权条件，仓储在分页或聚合前应用条件，用例在交付前通过 `validate` 复验凭证。`has_permission` 只检查功能资格，可用于按钮或可选字段，不可替代完整授权。

## 使用权限

内置权限直接引用所属模块常量。数据库或配置读出的文本使用 `Permission::parse`；反序列化也走同一解析入口。

```rust
use erp_permissions::{Permission, sales_inquiry};

let permission = &sales_inquiry::COMMANDS_SALES_SAVE;
assert_eq!(permission.resource(), "sales_inquiry");
assert_eq!(permission.action(), "update");

let configured = Permission::parse(" Sales_Inquiry:UPDATE ")?;
assert_eq!(*permission, configured);
```

`Permission::new(resource, action)` 接收已规范化的静态文本，非法值在常量求值时导致编译失败。`Permission::parse` 裁剪整串首尾空白、转为 ASCII 小写，非法输入返回 `ParsePermissionError`。两种构造共享格式校验，按相同规则比较、哈希和排序，并序列化为一个 `resource:action` 字符串。`grant.covers(&required)` 只允许授予方的整个资源或动作分量为 `*` 时通配；资源路径前缀不覆盖子路径。

## 新增和复用

1. 新资源标识加入 [resources.rs](src/resources.rs)，新公共分组加入 [groups.rs](src/groups.rs)。在所属模块定义 `const Permission`；已有相同编码则引用现有常量，不重复构造。
2. 将独立目录权限的主常量加入模块的 `ENTRIES`，填写公共分组和可复用说明；纯复用符号不重复登记。新增公共目录模块时，将它加入 [lib.rs](src/lib.rs) 的 `catalog_modules!` 列表。可配置但无独立路由的政策资格登记在 [policy_permission.rs](src/policy_permission.rs)。
3. 在管理端通过 `permission_route(route, &permissions::<module>::<CONST>)` 直接引用已登记的主常量；多个接口复用同一主常量，不在公共目录保存 HTTP 信息。仅当接口说明不同才在 [route_descriptions.rs](../../../apps/erp-server/route_descriptions.rs) 登记差异。新增管理端路由模块时，同时接入[管理端路由](../../../apps/erp-server/src/http/routes/admin.rs)和生成器的 `DOMAIN_MODULES` [扫描清单](../../../apps/erp-server/build_permissions.rs)。
4. 在受保护业务用例接入 identity 授权，不以路由标记或展示项代替授权检查。修改权限定义或路由目录后，按下节重新生成并审阅差异，不手工修改生成物。

## 生成与检查

修改权限定义、路由或展示登记时，在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-permissions --lib
python3 scripts/generate-permissions.py --test
python3 scripts/generate-permissions.py
./scripts/check-permissions-drift.sh
```

[独立生成脚本](../../../scripts/generate-permissions.py) 用临时 Cargo 清单执行 server 构建所用的同一份 `build.rs`，只编译生成器、`erp-permissions`、`syn`、`serde_json` 及其依赖，不编译业务 crate。默认写入 `erp-client/lib/` 下的两个客户端目录；`--test` 在临时目录生成后运行生成器内联单测，包括目录与真实 Rust 常量的对应校验。本 crate 的单元测试由第一条命令单独运行。[漂移检查脚本](../../../scripts/check-permissions-drift.sh) 调用生成脚本的 `--check`，在临时目录生成并逐字节比较两个已保存的客户端目录，不改写生成物。
