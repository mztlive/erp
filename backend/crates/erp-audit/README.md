# erp-audit：安全业务事件与审计查询

记录和查询“谁在什么时候对哪个业务对象执行了什么操作”，校验明确登记的中文动作与安全事实投影。

审计记录的格式、字段白名单、存储和授权查询在此维护。业务命令结果、创建人、参与资格及终态由拥有领域维护，禁止通过审计消息或结构化事件恢复业务事实。

## 使用场景

- 新增业务操作的审计记录，或调整操作日志查询。
- 按操作人、动作、命令执行结果及业务编号查询历史事件。

## 协作示例

销售单操作完成时，销售状态由销售领域维护，Process 将业务写入与审计写入放入约定事务；本 crate 保存并查询审计记录。用于排查程序问题的 tracing 日志仍由运行代码输出。

## 负责的数据与能力

- 审计日志及结构化安全事件的构造、校验、持久化和分页查询。
- 独立 `audit_attempts` 保存失败、拒绝及未知尝试的安全上下文；不提供业务恢复依据。
- 显式中文动作、字段及状态注册项的验证；精确数量、金额和脱敏变化的安全投影。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 命令身份和结果回执由拥有领域保存；本域仅保留追溯关联，不提供业务授权或命令重放依据。
- 跨领域业务写入与审计的一致性由 erp-processes 编排，调用方不得绕过事务写入约定。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/catalog.rs](src/catalog.rs) | 普通动作与目标类型的显式中文目录，未知配对拒绝 |
| [src/entity/attempt.rs](src/entity/attempt.rs) | 事务外安全尝试及 Failed/Rejected/Unknown 分类 |
| [src/service.rs](src/service.rs) | AuditLogService 与现有日志构造入口 |
| [src/entity/business_event.rs](src/entity/business_event.rs) | 显式动作、字段白名单、BusinessEventContext 与安全中文事实 |
| [src/repository/mod.rs](src/repository/mod.rs) | AuditExt、AuditLogRepository 与授权审计查询 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/indexes.rs](src/indexes.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 新增或迁移写用例使用 `erp-processes::audit::execute_audited` 或 `run_audited_event`；事实和审计复用同一 Executor，原结果回放不得重复保存成功事件。
6. 补充本域库单元测试，覆盖静态与结果校验、安全值、中文、快照及序列化边界。执行细节见[审计执行合同](../../docs/audit-logging.md)。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-audit --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-audit --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
