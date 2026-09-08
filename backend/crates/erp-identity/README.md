# erp-identity：身份与访问控制

## 职责合同

- 账号、角色、权限、用户角色绑定和数据范围规则。
- 后台认证、IAM 管理、RBAC 服务与 MongoDB Casbin 适配。

## 依赖与协作边界

- 本 crate 拥有本域实体、DTO、规则、Service、Repository、集合访问器和索引。normal、build、dev 依赖均不得指向其他业务领域或组合层。
- 账号与授权规则由本域提供；HTTP JWT 中间件与路由装配由 web-api 负责。
- 审计等外域能力通过本域 Port 注入；不得在身份仓储中读写其他领域集合。
- 外域适配器在组合层或应用组合根装配；公开入口以 [src/lib.rs](src/lib.rs) 为准。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/iam/mod.rs](src/service/iam/mod.rs) | 账号、角色与 RBAC 服务入口 |
| [src/service/auth/mod.rs](src/service/auth/mod.rs) | 后台认证入口 |
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
