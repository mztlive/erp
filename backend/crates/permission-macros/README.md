# permission-macros：HTTP 权限标注宏

## 职责合同

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
