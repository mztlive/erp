# permission-macros：HTTP 权限标注宏

为 HTTP Handler 声明权限标识，并在编译时生成取得该权限键的函数。

路由权限与权限目录需要引用一致的 resource:action 标识。把声明放在 Handler 上，可供权限键使用和应用构建阶段的权限收集。

## 使用场景

- 新增接口时声明 resource、action 及权限描述元数据。
- 修改权限属性参数解析，或权限键辅助函数的生成方式。

## 协作示例

在名为 list_examples 的函数上标注 resource="example"、action="read" 后，宏生成 list_examples_permission_key()，返回 example:read 对应的 Permission。运行时是否允许访问由 [erp-identity](../erp-identity/README.md) 与应用鉴权链路判断。

## 负责的数据与能力

- permission 属性宏保留被标注函数，并生成同名后缀为 _permission_key 的公开函数。
- 生成函数返回 erp_identity::Permission，权限键由 resource:action 组成。

## 使用与边界要求

1. resource 与 action 必须提供字符串字面量；消费 crate 必须能访问 erp_identity。
2. group、group_desc、desc 等元数据用于应用权限收集；本宏的权限键生成只读取 resource 和 action。
3. 宏不执行运行时鉴权；管理员路由仍须装配 JWT 与 RBAC。
4. 新增或变更权限标注后运行权限漂移检查；不得手工编辑前端生成权限文件。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | permission 属性宏与参数解析 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p permission-macros --locked
env -u ERP_TEST_MONGO_URI cargo test -p permission-macros --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
