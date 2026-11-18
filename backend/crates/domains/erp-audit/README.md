# erp-audit：安全业务事件与审计查询

## 职责

本 crate 记录并查询「谁在什么时候对哪个业务对象执行了什么操作」。`audit_logs` 保存业务操作的安全快照（新业务事件写入 `structured_event`），也保存登录等内部日志；`audit_attempts` 独立保存失败、拒绝和提交结果未知的命令尝试。身份治理的 `audit_events` 由 `erp-identity` 单独维护和查询；排查程序问题的 `tracing` 日志也不属于审计记录。

审计不是业务事实或回执。业务规则、写入授权、幂等键与回放、创建人、参与资格和终态由拥有领域或命名 Process 负责；不得从审计消息或结构化事件反推这些信息。按 [Cargo.toml](Cargo.toml)，读取依赖 `erp-identity` 的共享授权服务和 `erp-permissions` 的公共权限定义，身份域不反向依赖本 crate；跨域写入由业务 Process 编排，其他领域通过窄 Port 或应用适配器接入。统一分工见 [Crate 指南](../../README.md#代码中常见名称的含义)。

## 目录结构

| 源码入口 | 职责 |
| --- | --- |
| [lib.rs](src/lib.rs)、[catalog.rs](src/catalog.rs) | 对外导出实体、服务、执行边界和仓储接口；按稳定动作码与资源类型登记普通动作及中文名称。 |
| [entity/](src/entity/) | `AuditLog`、`AuditAttempt`、`BusinessEventContext` 及结构化事件、字段白名单和安全值校验。 |
| [service.rs](src/service.rs) | `AuditActorLogs`、`prepare_business_log`、`attempt_context` 构造或整理安全记录；`AuditLogService` 提供内部日志写入。 |
| [execution/](src/execution/) | 同事务执行与写入、直接开启事务的包装、事务外尝试留痕以及原命令结果查证。 |
| [read.rs](src/read.rs)、[dto.rs](src/dto.rs)、[error.rs](src/error.rs) | 受授权的管理端列表、参数与响应类型，以及身份和持久化错误的分类映射。 |
| [repository/](src/repository/)、[indexes.rs](src/indexes.rs) | 集合访问、保序写入、列表过滤分页、按对象读取时间线及索引定义。 |

## 架构设计

### 审计写入

认证入口从已验证的账号构造 `AuditActor`；调用方传递当时取得的安全名称快照和请求号，不让客户端指定审计动作、目标类型或执行结果。普通动作可用 `AuditActorLogs::resource_log` 准备日志，经 `registered_action` 精确匹配动作码与资源类型，未登记或错配即拒绝；类型化 Service 或 Process 显式定义 `AuditAction` 的代码、中文名称、版本、资源类型及投影白名单，不从权限、函数名或路由推导。

类型化命令在业务写入前以 [`BusinessEventContext::new`](src/entity/business_event.rs) 校验身份和动作，执行后再以 `BusinessEventContent` 提供目标 ID、业务编号快照与允许记录的事实。值只允许登记的代码和中文名称、精确数量或金额，或不带原值的「已变更」；不记录请求正文、凭据、密文或完整敏感值。命令执行成功不等于业务结果成功，例如 [确认服务履约的投影](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/service_confirm_audit.rs) 可以同时记录命令成功与服务结果失败。

| 接入场景 | 执行合同 |
| --- | --- |
| 已有事务，执行类型化命令 | [`execute_audited`](src/execution/command.rs) 以原 `Executor` 调用 `AuditedCommand`。`Fresh` 需通过成功标记和结果投影校验，再经 `AuditEventSink` 写入一条成功事件；`Replayed` 只返回领域恢复的原结果。此入口不开事务、不补记尝试。 |
| 由用例开启事务，执行类型化命令 | [`run_audited_event`](src/execution/transaction.rs) 建立唯一事务，在其中调用 `execute_audited`。例如 [合同导入确认](../../orchestration/erp-data-import/src/contract_import/archive.rs) 将归档与审计一起提交，[命令实现](../../orchestration/erp-data-import/src/contract_import/audit.rs) 在同一确认已处理时返回 `Replayed`。 |
| 普通业务写入，已有准备好的日志 | [`execute_prepared`](src/execution/command.rs) 先校验登记信息和成功标记，再以同一 `Executor` 执行业务闭包并保存审计。需要由用例开事务时用 [`run_audited`](src/execution/transaction.rs)，在开事务前准备并验证日志和尝试上下文。 |
| 自管事务中的单条或批量追加 | [`persist_log` / `persist_logs`](src/execution/command.rs) 复用原 `Executor`，验证登记配对并重建中文摘要，不复制自由说明。`persist_logs` 先校验整批，再以 `create_many_ordered` 按传入顺序写入；例如 [附件资产批次](../../orchestration/erp-file-assets/src/pending.rs)。 |
| 登录等内部日志 | [`AuditLogService::new(db).create(data)`](src/service.rs) 用 `NoTransaction` 直接写入 `audit_logs`，不生成结构化业务事件。比如 [后台登录](../../../apps/erp-server/src/http/handler/auth/login.rs) 尽力留痕，写日志失败不改变认证结果；不得以此替代正式业务的同事务审计。 |

正式业务写入和成功审计必须以同一 `Executor` 原子提交；业务、投影或审计写入任一步失败都不能提交。事务中只放需要原子的数据库校验与读写，外部 HTTP、S3 和文件 I/O 留在事务外。单事件序号默认 `1`，`event_sequence` 不允许零；同一命令的多事件共享 `command_id`，按实际业务写入顺序分配序号，并在首笔写入前检查批次容量和序号溢出。原名称、请求号和序号应沿入口、领域和适配器传递，缺失的名称和请求号不补造。身份策略写入仍走 [`RbacService::run_authorized_audited_policy_transaction`](../erp-identity/src/service/iam/rbac/transaction.rs) 的版本重验、原子审计和提交后刷新流程，不用普通事务包装替换。

### 独立尝试记录

`run_audited`、`run_audited_event` 在原事务返回后调用 [`finish_attempt`](src/execution/attempt.rs)。自管事务的用例也必须在外层事务返回后再调用，例如 [服务履约确认](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/service_confirm.rs) 在事务内执行类型化审计、在事务外记录尝试。工作项转交和关闭通过 [工作流审计适配器](../../../apps/erp-server/src/wiring/workflow/audit.rs) 的 `WorkflowAuditPort::persist_attempt` 接入同一集合。

`MongoAuditAttemptSink` 用独立 `NoTransaction` 保存 `event_kind=command_attempt`。成功不写尝试；失败由 `AuditFailure` 映射为 `failed`、`rejected` 或 `unknown`，只保存已验证动作、操作人、执行前已知目标以及命令和请求关联。未知目标保持缺失，不存原请求、错误正文或敏感字段。补记写入失败只输出固定的安全诊断，原错误及未知提交的来源不变。

普通事务包装直接等待事务与补记，不启动独立任务，也不保证调用方取消后继续收尾。暂态事务冲突不自动重跑事务；提交结果未知也不能自动重放。已有命令身份时，只能在当前授权下按原键查证拥有领域的回执；[`recover_command`](src/execution/recovery.rs) 处理查证结果，不执行业务写入，查无回执或查证失败都保留首次未知提交错误。

### 查询审计日志

[管理端路由](../../../apps/erp-server/src/http/routes/admin.rs) 先经 [JWT 认证中间件](../../../apps/erp-server/src/http/middleware/authentication.rs) 校验当前账号并注入 `AuditActor`；`permission_route` 只登记目录元数据，不执行授权。[列表 Handler](../../../apps/erp-server/src/http/handler/admin/audit_log.rs) 以应用数据库和进程共享的完整 `AuthorizationService` 构造 `AuditLogReadService::new(db, authorization)`，调用 `audit_log_list(&params, &actor)`。

[读取服务](src/read.rs) 在同一快照事务中先用 `AuthorizationService::require` 检查 [`admin::AUDIT_LOG_LIST_AUDIT_LOGS`](../../foundation/erp-permissions/src/admin.rs)（`audit_log:list`）和 `AccessTarget::Operation`，再验证、归一化查询参数，并以同一执行器读取分页和总数。无资格、身份设施失败或参数非法时不查询审计集合，错误按原分类返回。管理端列表没有免授权的服务入口，列表权限也不授予关联业务对象或敏感信息的读取资格。

列表只查 `audit_logs`，返回 `items + total`，保留单项既有字段及可选的 `structured_event`。`event_result` 按结构化命令结果精确筛选，`resource_number` 按事件中的业务编号快照作字面筛选，不解析消息。缺失的历史快照不在列表查询时回填。跨域读模型使用的是另一条路径：例如 [销售时间线](../../read-models/erp-read-models/src/sales_timeline/query.rs) 先授权销售单和关联对象，再以原执行器通过 [`timeline_events`](src/repository/timeline.rs) 读取这些对象的结构化成功事件；仓储方法本身不替调用方授权。

## 修改执行要求

无 I/O 的校验与不变式放在实体或值对象，Repository 复用调用方的 `Executor`，HTTP 优先复用本域 DTO。调整查询时同步评估索引；[服务端](../../../apps/erp-server/src/indexes.rs) 与 [CLI](../../../apps/cli/src/indexes.rs) 统一调用 `erp_audit::indexes::ensure`，按既有顺序注册 `audit_logs`、`audit_attempts` 的索引。新增或迁移审计行为时遵循[审计执行合同](../../../docs/audit-logging.md)，扩展公开接口时同步核对 Process、ReadModel 和应用调用方。

## 验证要求

仅改文档时在 `backend/` 对目标文档执行 `git diff --check`。改代码时按 [统一质量门禁](../../README.md#质量门禁) 验证，审计行为只补充和运行本域库单元测试，覆盖同一执行器、首次写入和回放、失败停止、中文白名单、安全值、快照及序列化边界，不执行集成测试或真实外部服务测试。
