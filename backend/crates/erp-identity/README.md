# erp-identity：身份与访问控制

管理登录账号、组织与成员关系、角色权限和数据范围，提供后台身份认证与访问控制能力。

系统需要统一回答“当前操作人是谁、具有什么权限、属于哪些组织”。各业务领域在这些授权事实基础上，继续执行自身对象的访问和操作规则。

## 使用场景

- 修改后台认证、账号管理、角色及权限配置。
- 维护组织、成员和数据范围规则，调整授权解析。

## 协作示例

给账号配置角色和组织关系时使用本 crate；客户归谁负责由 [erp-customer](../erp-customer/README.md) 维护。HTTP 请求中的 JWT 校验和路由装配由 [web-api](../../apps/web-api/README.md) 接入，权限标注宏见 [permission-macros](../permission-macros/README.md)。

## 负责的数据与能力

- 账号、角色、权限、用户角色绑定和数据范围规则。
- 内部组织节点、成员及管理关系，组织变更预览与执行。
- 后台认证、IAM 管理、RBAC 服务与 MongoDB Casbin 适配。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 账号与授权规则由本域提供；HTTP JWT 中间件与路由装配由 web-api 负责。
- 审计等外域能力通过本域 Port 注入；不得在身份仓储中读写其他领域集合。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/iam/mod.rs](src/service/iam/mod.rs) | 账号、角色与 RBAC 服务入口 |
| [src/service/auth/mod.rs](src/service/auth/mod.rs) | 后台认证入口 |
| [src/service/organization.rs](src/service/organization.rs) | 组织状态、变更预览与执行 |
| [src/entity/organization.rs](src/entity/organization.rs) | 部门、团队、成员与管理关系 |
| [src/ports/mod.rs](src/ports/mod.rs) | 审计及授权合同 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes.rs](src/indexes.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-identity --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
