# erp-audit：审计与命令回执查询

记录和查询“谁在什么时候对哪个业务对象执行了什么操作”，并提供已执行命令的回执查询。

多个业务流程都需要追溯操作和确认命令结果。审计记录的格式、存储和查询在此统一维护，业务流程通过约定入口提交审计事实。

## 使用场景

- 新增业务操作的审计记录，或调整操作日志查询。
- 查询某个操作人的历史记录，或根据命令身份寻找已执行结果。

## 协作示例

销售单操作完成时，销售状态由销售领域维护，Process 将业务写入与审计写入放入约定事务；本 crate 保存并查询审计记录。用于排查程序问题的 tracing 日志仍由运行代码输出。

## 负责的数据与能力

- 审计日志的构造、持久化、分页查询和操作人日志查询。
- 基于审计事实的命令回执读取，以及职责分离所需的审计事实。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 共享命令身份、指纹和回执匹配规则复用 application-core；本域负责审计事实读取。
- 跨领域业务写入与审计的一致性由 erp-processes 编排，调用方不得绕过事务写入约定。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

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
