# erp-audit：审计与命令回执查询

## 职责合同

- 审计日志的构造、持久化、分页查询和操作人日志查询。
- 基于审计事实的命令回执读取，以及职责分离所需的审计事实。

## 依赖与协作边界

- 本 crate 拥有本域实体、DTO、规则、Service、Repository、集合访问器和索引。normal、build、dev 依赖均不得指向其他业务领域或组合层。
- 共享命令身份、指纹和回执匹配规则复用 application-core；本域负责审计事实读取。
- 跨领域业务写入与审计的一致性由 erp-processes 编排，调用方不得绕过事务写入约定。
- 外域适配器在组合层或应用组合根装配；公开入口以 [src/lib.rs](src/lib.rs) 为准。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service.rs](src/service.rs) | AuditLogService、AuditActorLogs 与 CommandReceiptServiceExt |
| [src/repository/mod.rs](src/repository/mod.rs) | AuditExt、AuditLogRepository 与 SeparationAuditFact |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
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
cargo check -p erp-audit --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-audit --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
