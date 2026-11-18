# application-core：共享应用合同

## 职责

`application-core` 统一多个应用用例对请求参数、分页结果、操作人、命令重试和错误的表达方式。它定义数据形状和通用规则；是否允许执行、如何读取和写入、怎样交付业务结果，由消费这些合同的用例决定。

### 负责的数据与能力

- 查询：分页参数与响应、文本和排序规范化、人员筛选、对象目录及跨页范围版本校验。
- 调用人：已取得的操作人身份、名称快照、请求关联与同一命令内的审计事件序号。
- 命令：稳定命令身份、版本化载荷指纹、结构化回执匹配，以及只读查证结果与首次提交错误的合并规则。
- 错误与执行辅助：应用错误分类、可公开的字段校验明细，以及等待独立任务完成关键操作的 `await_owned`。

### 使用场景

在领域 Service、跨域 Process、ReadModel 或 HTTP 入口复用这些合同。领域继续定义列表项、业务筛选、错误和回执结果；HTTP 入口提取参数并映射响应，不重复定义分页信封，也不替业务用例作授权判断。

## 目录结构

[Cargo.toml](Cargo.toml) 声明依赖；[src/lib.rs](src/lib.rs) 是公开模块和重导出的入口。实现平铺在 `src/`，按合同查找：

### 代码入口

| 模块 | 合同 |
| --- | --- |
| [page.rs](src/page.rs)、[query.rs](src/query.rs) | `Page`、`PageView`、`PageParams`、分页和排序规范化 |
| [scope_version.rs](src/scope_version.rs) | `ScopeVersion`、空串语义、后续页版本必填和精确比对 |
| [query_ids.rs](src/query_ids.rs)、[directory.rs](src/directory.rs) | `QueryIds`、`FilterOption`、`OwnershipPage`、对象目录查询和有界快照、`ScopedPage` |
| [context.rs](src/context.rs)、[audit_context.rs](src/audit_context.rs) | `AuditActor`、`AuditContext`、平铺预制审计的 `impl_audit_context!` |
| [command.rs](src/command.rs)、[structured_receipt.rs](src/structured_receipt.rs) | `CommandIdentity`、`CommandFingerprint`、`CommandReceipt`、`StructuredCommandReceipt` 及匹配分类 |
| [command_recovery.rs](src/command_recovery.rs)、[owned_task.rs](src/owned_task.rs) | `recover_resource` 合并只读查证结果；`await_owned` 等待独立任务 |
| [error.rs](src/error.rs)、[validation.rs](src/validation.rs)、[error_macros.rs](src/error_macros.rs) | `Error`、`ErrorClass`、`Result`、`ValidationIssues` 及领域错误转换和分类宏 |

## 架构设计

### 使用与边界要求

本 crate 只依赖 [erp-core](../erp-core/README.md) 和通用技术库；金额等共享值类型归 `erp-core`，数据库访问、仓储分页和执行器等机制归 [persistence-core](../persistence-core/README.md)。不得在本 crate 引入领域实体、领域仓储、HTTP 或组合层依赖，也不得在此读取业务事实、解析权限、管理事务或持久化回执与审计。代码归属遵循[后端开发规则](../../../AGENTS.md#代码落点)。

### 协作示例

#### 分页与范围版本

- **普通列表。** `normalize_paging` 补齐缺省值（第 1 页、每页 20 条、`created_at` 降序），检查显式排序字段是否在调用方白名单内，只接受 `asc`/`desc`，把页大小限制在 1–100，保留显式页码 `0` 供入口拒绝。[往来主体列表参数](../../domains/erp-party/src/dto/party/mod.rs)声明白名单和页码校验；[主体 Service](../../domains/erp-party/src/service/party/mod.rs)组织授权和查询，将[仓储](../../domains/erp-party/src/repository/party/record.rs)按同一过滤条件查得的当前页和总条数封装为 `PageView<PartyView>`（`items`、`total`、`page`、`page_size`）。只需要 `items` 和 `total` 时，使用轻量 `Page<T>`，例如[审计日志列表](../../domains/erp-audit/src/read.rs)。`total` 指符合筛选条件的总条数，不是当前页条数。
- **跨页一致性。** `ScopeVersion` 不裁剪字符串、不生成版本，只检查后续页是否带版本和已提供的版本是否精确匹配。入口须显式选择空串是 `Missing` 还是 `Provided`。[客户列表](../../domains/erp-customer/src/service/customer/query.rs)从第二页起要求回传版本，在同一读取阶段取数并在交付前复核。[客户范围适配](../../domains/erp-customer/src/service/customer/scope.rs)在必填检查时将空串视为缺失，在精确比对时将空串视为已提供，并将失败映射成 `DATA_SCOPE_CHANGED` 冲突。授权范围解析、版本来源、业务错误码和交付前复核仍由客户用例负责。`QueryIds` 只规范化筛选身份，不能授予访问资格；消费方必须将它与授权范围求交。

#### 调用人和预制审计

[管理端认证中间件](../../../apps/erp-server/src/http/middleware/authentication.rs)验签并核实当前账号后，将 `AuditActor` 放入请求扩展，附上当时取得的显示名及已有追踪号。`AuditActor` 不是授权凭证，也不决定审计动作或目标。`AuditContext::from` 保留身份与可选快照，事件序号从 1 开始；没有名称或请求号就保持缺失，不查询账号补齐。

[主体领域的预制审计](../../domains/erp-party/src/ports/audit.rs)保留自己的 `PreparedPartyAudit` 和 `PartyAuditPort`，用 `AuditContext` 与宏搬运操作人、名称、请求关联和序号。共享上下文不生成审计事件 ID 或持久化元数据，不替代安全校验；[组合层适配器](../../../apps/erp-server/src/wiring/audit_conversion.rs)还原事件时重新校验。业务动作、目标及事件顺序由用例按[审计合同](../../../docs/audit-logging.md)决定。

#### 命令回放与只读查证

1. 用 `CommandIdentity` 定位原命令，用 `CommandFingerprint` 区分同一身份下的载荷。指纹使用版本化的 `sha256-v1` 和长度前缀编码。分量须按固定顺序提供，集合须按业务语义规范化；不要使用 `Debug`、不稳定序列化或分隔符拼接。`CommandReceipt::from_payload` 对 JSON 对象键排序但保留数组顺序；`from_resource_parts` 将目标 ID 纳入命令身份。回执只保存规范化幂等键的摘要，不保存原键。
2. 新的独立领域回执应嵌入 `StructuredCommandReceipt`，直接保存命令 ID、操作人、动作、资源类型、作用域、幂等键摘要与版本化载荷指纹。共享规则区分同载荷、同身份异载荷和回执损坏，不读取审计消息。[退货逆向回执](../../domains/erp-returns/src/entity/command_receipt.rs)还校验强类型结果和审计关联：同载荷返回原单据 ID，异载荷报冲突，损坏报内部错误，不当作未执行。实际回执、业务结果和审计记录由各自拥有者保存；用例负责事务边界和回放时的当前授权及视图读取。
3. [逆向创建提交](../../orchestration/erp-reverse-flow/src/reverse_flow/commit_access.rs)在写事务结束后，仅对可能已提交的错误按原身份做只读查证，复验授权，不重跑资金或审批步骤。[查证结果合并](../../orchestration/erp-reverse-flow/src/reverse_flow/command_recovery.rs)调用 `recover_resource`：查到则返回原结果，未查到则保留首次错误；查证失败时，首次错误若为提交结果未知仍保留，否则返回查证错误。提交结果未知时，调用方须沿原命令合同处理，重试使用原幂等键。

#### 错误类型与等待边界

- 共享 `Error` 只有 `Internal`、`ValidationError` 和 `InvalidInput`；后两者归为 `ErrorClass::BusinessRule`。`ValidationIssues` 保留字段路径和可公开的说明，不携带原始输入；跨层不得压成一条字符串。
- [退货逆向领域错误](../../domains/erp-returns/src/error.rs)用共享宏转换校验错误并保留字段明细，为领域错误实现分类；领域自己拥有 `ConflictError`、`Forbidden`、`OutcomeUnknown` 等变体。提交结果未知归 `Internal` 类，但保留原变体与来源，不当作可重试冲突。[HTTP 错误映射](../../../apps/erp-server/src/http/errors.rs)负责状态码、公开错误码与字段明细，例如同键异载荷映射为 `409`，提交结果未知保留 `OUTCOME_UNKNOWN`。`ErrorClass` 不是 HTTP 状态码表；映射应使用类型和稳定错误码，不用展示文案推断错误。
- 仅在关键操作需要脱离等待方取消时显式使用 `await_owned`。任务启动后，取消等待不会取消该独立 Tokio 任务；操作错误原样返回，任务异常终止映射为内部错误。这不提供事务、自动重试或持久化任务调度。[身份策略事务](../../domains/erp-identity/src/service/iam/rbac/transaction.rs)用它保护提交和提交后的策略收尾，事务内容与一致性处理仍由身份领域负责。

## 验证要求

仅修改文档时，核对相对链接并对改动文件执行 `git diff --check`，不编译或运行测试。代码变更按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)执行；本 crate 的定向库单测命令在 `backend/` 下运行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p application-core --lib --locked
```

不运行集成测试或依赖真实外部服务的测试。
