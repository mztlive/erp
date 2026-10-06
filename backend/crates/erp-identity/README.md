# erp-identity：身份与访问控制

管理登录账号、组织与成员关系、角色权限和数据范围，提供后台身份认证与访问控制能力。

系统需要统一回答“当前操作人是谁、具有什么权限、属于哪些组织”。各业务领域在这些授权事实基础上，继续执行自身对象的访问和操作规则。

## 使用场景

- 修改后台认证、账号管理、角色及权限配置。
- 维护组织、成员和数据范围规则，调整授权解析。
- 校验、预览、原子应用和导出授权 JSON 文件；角色、真实绑定、人员范围及命令回执由本域维护。

## 协作示例

给账号配置角色和组织关系时使用本 crate；客户归谁负责由 [erp-customer](../erp-customer/README.md) 维护。HTTP 请求中的 JWT 校验和路由装配由 [web-api](../../apps/web-api/README.md) 接入，权限标注宏见 [permission-macros](../permission-macros/README.md)。

## 负责的数据与能力

- 账号、角色、权限、用户角色绑定和数据范围规则。
- 内部组织节点与成员关系，组织变更预览与执行；旧部门管理关系仅保留历史记录。
- 后台认证、IAM 管理、RBAC 服务与 MongoDB Casbin 适配。
- 独立供应商门户账号、固定维护或只读岗位、单供应商绑定及会话版本失效。门户不建立内部角色或组织关系。

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
| [src/service/access_control/policy_bundle/mod.rs](src/service/access_control/policy_bundle/mod.rs) | 授权文件预览、应用及导出 |
| [src/entity/authorization_bundle/mod.rs](src/entity/authorization_bundle/mod.rs) | 文件格式、权限差异、审核摘要和重放规则 |
| [src/service/auth/mod.rs](src/service/auth/mod.rs) | 后台认证入口 |
| [src/service/portal/mod.rs](src/service/portal/mod.rs) | 供应商身份、绑定、账号管理及密码步骤 |
| [src/entity/portal.rs](src/entity/portal.rs) | 门户固定岗位及账号绑定不变式 |
| [src/service/organization.rs](src/service/organization.rs) | 组织状态、变更预览与执行 |
| [src/entity/organization.rs](src/entity/organization.rs) | 部门、团队、成员及旧管理关系存储类型 |
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
6. 门户密码哈希在事务外完成；开通、启停及自助密码的持久化步骤接收调用方 `Executor`。供应商启用事实、内部管理权限和对象范围由组合层在同一写入事务中重验。每次门户访问必须重验账号版本、绑定版本、绑定启用及供应商启用；后台认证固定接受 `Admin`。

## 部署岗位角色

1. 使用超级管理员登录，进入“角色与权限”→“从内建岗位生成”。
2. 选择销售、销售领导、采购、运营、仓储、财务总监、出纳、开票、管理层及系统管理员；默认选择尚未生成且可授权的十个分岗模板。综合财务是可选模板，包含完整财务能力，不应与分岗限制混用。
3. 生成前核对各岗位操作权限及人员上岗配置。批量生成沿用角色创建权限和授予上限，角色、权限策略、审计及策略版本在同一事务提交。
4. 既有固定角色身份保持原配置，包括改名、权限调整、停用和软删除。生成动作不补权、不恢复账号、不修改绑定。启动仅维护超级管理员、组织基础及目录资格，不再创建或升级业务角色。
5. 在“组织与人员”分配生成的角色，并设置本人以外业务范围、目录可见和仓库范围；审批、付款、开票及履约责任另行指定。角色生成不创建演示账号、密码、部门、业务数据或审批定义。

接口及部署约束执行[人员配置合同第 9 节](../../../docs/identity-access-workflow-contract.md#9-内建岗位角色生成)。

## 外部授权配置

使用仓库根 `scripts/authorization-policy.mjs` 调用正式后台 API；顺序为 validate、preview、审核差异、apply。export 必须明确选择人员或角色。文件格式、权限要求、命令示例及回滚要求执行[授权 JSON 配置合同](../../../docs/authorization-policy-json-contract.md)；不得直接写入 Casbin 或人员范围集合。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-identity --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
