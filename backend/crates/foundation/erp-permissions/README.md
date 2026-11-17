# erp-permissions

本 crate 提供公共权限值、内置权限常量、资源标识和展示目录。运行时与构建生成器必须使用同一份定义。账号资格、DataScope 和对象授权由 `erp-identity` 执行。

## 使用权限

内置权限直接引用所属模块常量：

```rust
use erp_permissions::{Permission, sales_inquiry};

let permission = &sales_inquiry::COMMANDS_SALES_SAVE;
assert_eq!(permission.resource(), "sales_inquiry");
assert_eq!(permission.action(), "update");

let configured = Permission::parse(" Sales_Inquiry:UPDATE ")?;
assert_eq!(*permission, configured);
```

`Permission::new(resource, action)` 用于已经规范化的静态文本，非法常量使编译失败。数据库、配置和反序列化输入必须经过 `Permission::parse`，错误类型为 `ParsePermissionError`。常量与动态值使用相同格式约束、相等判断、哈希、排序和通配覆盖规则，序列化为 `resource:action` 字符串。

## 新增和复用

1. 新资源标识加入 `src/resources.rs`。
2. 在所属模块定义 `const Permission`。相同权限编码通过常量引用复用，不重复调用构造函数定义相同编码。
3. 独立权限加入该模块的 `ENTRIES`，填写公共分组及可复用说明。纯复用符号不重复登记独立权限。
4. 新模块在 `src/lib.rs` 导出并登记 `MODULES`。公共目录不包含 HTTP 方法、路径或处理器。
5. HTTP 路由通过 `permission_route(route, &permission)` 引用公共 `ENTRIES` 登记的权限主常量。同一权限的接口复用该主常量，名称、分组及默认说明从公共目录读取；只有说明不同才在 server 的 `route_descriptions.rs` 登记差异。新增路由模块同步登记 server 的扫描清单。
6. 修改后运行下列检查并 review 生成结果。

目录项保存 `&'static Permission`，不得另存一份权限编码字符串。HTTP 专属信息由 server 的差异记录补充，公共 crate 不提供接口包装或展示权限与鉴权权限的绑定类型。`permission_route` 只供生成器读取路由元数据；运行时由受保护业务用例调用 identity 的 `require` 或 `query` 完成授权。非路由资格登记在 `policy_permission`，由生成器补入展示目录。

## 生成与检查

在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-permissions --lib
python3 scripts/generate-permissions.py --test
python3 scripts/generate-permissions.py
./scripts/check-permissions-drift.sh
```

独立生成命令与 server 构建共同执行 `apps/erp-server/build.rs`。工具使用临时构建清单，只编译本 crate、`syn`、`serde_json` 及其依赖；不构建业务 crate。`--test` 执行生成器与运行时目录的内联单元测试，`--check` 在临时目录生成后逐字节比较，不修改保存的生成物。

本 crate 的依赖边界由 `scripts/crate_layers.py` 检查：正常依赖限于 `serde`、`thiserror`，测试使用 `serde_json`。
