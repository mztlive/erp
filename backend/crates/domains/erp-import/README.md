# erp-import：历史数据导入事实

`erp-import` 是历史数据导入（`legacy_import`，W18）的业务领域。它记录一批历史数据经过了哪些校验、映射和责任确认，以及每行最终如何处理；正式客户、订单、库存等业务数据仍由各自领域持有。

## 职责

### 负责的数据与能力

- **批次**：保存批次号、来源系统和对象集、业务基准日、导入规则版本、状态、结果计数及摘要，并引用成功包、清单和失败诊断包等文件资产。
- **明细**：按批次、来源对象类型和来源行键定位一行，保存规范化载荷或引用，跟踪解析、映射、应用状态，以及来源映射 ID、目标单据或错误诊断。导入结果随行保存，批次保存汇总。
- **确认**：按责任范围保存特定批次及试算版本的业务决定，关联正式工作项；批次上的确认摘要不是决定的事实源。
- **命令回执与查询**：保存 `CompleteImportBusinessConfirmation` 和导入执行命令的独立结构化回执；提供批次列表、详情及明细列表查询，以及确认事实的仓储查询合同。

### 使用场景

修改批次或明细的状态规则、责任确认矩阵、导入结果查询、命令身份或重放规则时，从本 crate 查找。修改模板解析、跨域写入、后台执行或工作项联动时，从对应编排入口查找。

## 目录结构

### 代码入口

| 路径 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明与公开入口；导出实体、DTO、服务、端口和仓储访问器。 |
| [src/entity/mod.rs](src/entity/mod.rs)、[legacy_import/mod.rs](src/entity/legacy_import/mod.rs) | 批次、明细与确认实体；行构造、应用结果校验和稳定命令身份。 |
| [src/entity/command_receipt.rs](src/entity/command_receipt.rs) | 确认完成和执行命令的不可变强类型回执。 |
| [src/dto/legacy_import.rs](src/dto/legacy_import.rs) | 请求、视图、分页筛选与强命令契约；由调用方复用。 |
| [src/service/legacy_import/mod.rs](src/service/legacy_import/mod.rs)、[query.rs](src/service/legacy_import/query.rs) | `LegacyImportService` 只读用例：功能授权、批次列表和详情、明细分页。 |
| [src/ports/mod.rs](src/ports/mod.rs)、[bulk_job.rs](src/ports/bulk_job.rs) | `BulkJobFactsPort` 按请求号读取关联后台任务 ID；未接线时返回错误。 |
| [src/repository/mod.rs](src/repository/mod.rs)、[src/indexes/mod.rs](src/indexes/mod.rs) | MongoDB 仓储与集合访问器，以及公开索引注册入口 `indexes::ensure`。 |

## 架构设计

### 批次、明细与确认状态

[批次状态机](src/entity/legacy_import/legacy_import_batch.rs)的正常路径为：待校验（`pending_validation`）→ 校验中（`validating`）→ 待确认（`pending_confirmation`）→ 待应用（`ready_to_apply`）→ 导入中（`importing`）。最后一项必要责任确认只使批次待应用；进入导入中须另行提交 `START_APPLY`。

- **应用结果**：仍有待导入行时保持导入中；处理完毕后，无失败行则为完成（`completed`），有失败行则为部分失败（`partial_failed`）。结果汇总本身不产生失败（`failed`）；状态机允许校验、取消等失败路径。待应用或导入中可按条件取消剩余项，已有处理结果不回滚；有已处理项时批次为部分失败，否则为失败。失败或部分失败的批次按重试规则可回到待应用，但不自动重启。
- **明细状态**：每行有三个分别记录的维度。映射要求解析有效；登记应用结果要求解析有效且已映射。见[导入行实体](src/entity/legacy_import/legacy_import_row.rs)。

| 明细维度 | 状态迁移 |
| --- | --- |
| 解析 `parse_status` | 待解析 `pending_parse` → 有效 `valid` / 无效 `invalid`。 |
| 映射 `mapping_status` | 待映射 `pending_mapping` → 已映射 `mapped` / 冲突 `conflict`。 |
| 应用 `import_status` | 待导入 `pending_import` → 已导入 `imported` / 失败 `failed` / 跳过 `skipped`。`RETRY_FAILED` 只将失败行重置为待导入，不回退已导入或已跳过的行。 |

[应用结果集](src/entity/legacy_import/apply_result_set.rs)在查库前校验行 ID 唯一性和结果字段：成功必须提供目标单据 ID，失败或跳过必须提供原因码，两类字段不能混用。

[确认矩阵](src/entity/legacy_import/legacy_import_confirmation.rs)由来源对象集推导所需责任范围，并按批次、试算和规则版本核对。确认记录从 `PENDING` 进入 `CONFIRMED`、`REJECTED` 或 `INVALIDATED`。全部必要范围通过且没有退回，批次才可待应用；退回要求修复并生成新试算。新试算可使旧待确认记录失效，已确认或已退回的决定保留，不作业务删除。

### 仓储、索引与读取边界

- [`LegacyImportExt`](src/repository/extensions/legacy_import.rs) 提供 `legacy_import_batches`、`legacy_import_rows`、`legacy_import_confirmations` 的集合访问器。单集合操作复用 `persistence_core::Repository`；更新按 `id + version` 做乐观锁。批次连同明细创建、[按批次限定应用行范围](src/repository/legacy_import/apply_scope.rs)、[失败行写回](src/repository/legacy_import/failed_retry.rs)和[旧确认失效写回](src/repository/legacy_import/supersede_batch.rs)均复用调用方传入的 `Executor`，仓储不另开事务。
- [列表仓储](src/repository/legacy_import/projected.rs)用相同筛选条件分页和计数，只读取展示所需字段，明细列表不返回规范化载荷。[批次列表与详情](src/service/legacy_import/query.rs)要求调用方提供快照，明细列表由服务建立快照；同一快照内完成授权和读取。批次详情用批次号通过 `BulkJobFactsPort` 补齐后台任务 ID，[服务端适配器](../../../apps/erp-server/src/wiring/import.rs)负责接线；[`import_center` 读模型](../../read-models/erp-read-models/src/import_center.rs)在原快照中补齐来源系统名称。
- [索引](src/indexes/legacy_import.rs)约束批次号、批内来源行身份、每轮试算的责任范围和工作项身份；[回执索引](src/indexes/command_receipt.rs)保证命令及成功事件关联唯一。现有行索引声明 30 天诊断保留期，但 `created_at` 存为秒级整数，不会触发 MongoDB 的 BSON Date TTL，不能依赖该索引自动清理失败诊断。

### 命令回执与重放

确认完成和执行命令从操作人、动作、资源和幂等键派生[稳定命令身份](src/entity/legacy_import/mod.rs)，并校验规范化载荷指纹。[独立回执仓储](src/repository/command_receipt.rs)保存于 `import_command_receipts`；回执与相关业务事实、任务变更及成功事件同事务提交，冻结原动作结果、关联任务身份、版本和成功事件 ID。恢复结果只读此集合中的回执，不读展示审计消息，也不回退到旧回执别名；未知 schema、同键异参、错误动作或不匹配任务均拒绝。

- [确认回放](../../orchestration/erp-data-import/src/import_apply/complete.rs)重验原决定、确认人、正式任务完成身份与当前责任资格。
- [执行回放](../../orchestration/erp-data-import/src/import_apply/execution.rs)核对批次和冻结的后台任务 ID，允许批次与任务版本继续推进，仍返回原命令结果而非最新进度。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。依据 [Cargo.toml](Cargo.toml) 与[依赖与接入要求](../../README.md#依赖与接入要求)，本域直接使用 `erp-identity` 的公开授权服务及 `erp-permissions` 的权限定义；常规、构建和测试依赖不得直接指向其他业务领域或组合层。其他领域的事实通过端口、显式入参或组合层接入。

| 归属 | 分工 |
| --- | --- |
| `erp-import` | 持有历史导入事实、无 I/O 的业务规则、查询服务与仓储；不编排正式业务写入，不持有后台任务或工作项。 |
| [erp-data-import](../../orchestration/erp-data-import/README.md) | [`import_apply`](../../orchestration/erp-data-import/src/import_apply/mod.rs) 的 `ImportApplyProcess` 组织历史导入的跨域命令和事务，包括创建批次及任务、创建和完成确认、执行命令，以及正式应用阶段的逐行结果回写。确认列表的[工作项与人员投影](../../orchestration/erp-data-import/src/import_apply/confirmation_query.rs)也在编排层完成。商品和供应商模板分别走 [`product_import`](../../orchestration/erp-data-import/src/product_import/mod.rs) 和 [`supplier_import`](../../orchestration/erp-data-import/src/supplier_import/mod.rs)。 |
| [erp-support](../erp-support/README.md)、[erp-workflow](../erp-workflow/README.md) | 支撑域持有来源系统、外部身份映射、文件资产和后台任务状态；工作流域持有正式确认工作项及流程动作。导入批次状态、明细状态和后台任务状态是不同事实。 |

### 协作示例

一批历史数据整理为来源行后，[创建批次](../../orchestration/erp-data-import/src/import_apply/create_batch.rs)在一笔事务中写入批次、明细、后台任务和审计；[创建确认](../../orchestration/erp-data-import/src/import_apply/create_confirmation.rs)冻结试算与责任范围并登记工作项。责任人完成确认后，批次按矩阵决定是否待应用。[`apply_batch`](../../orchestration/erp-data-import/src/import_apply/batch.rs) 接收已经形成的逐行结果，在导入中批次和可记录进度的任务上原子回写行状态、批次计数、任务进度及审计；它不是创建正式业务对象的执行器。

**当前实现限制**：历史期初导入执行器尚未接通，[执行命令入口](../../orchestration/erp-data-import/src/import_apply/execution.rs)对 `START_APPLY` 和 `RETRY_FAILED` 返回 `LEGACY_IMPORT_EXECUTOR_UNAVAILABLE`；`CANCEL_PENDING` 不受此开关限制，仍须通过正常的权限、版本与状态检查。具备状态机和回写接口不代表已经支持端到端应用。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放入实体或值对象；本域查询由 Service 组织，跨域命令由 Process 组织。
2. Repository 复用原 `Executor`；跨集合写入由用例或 Process 控制事务。调整查询时同步评估索引，组合根复用 `indexes::ensure` 并保持既有注册顺序。
3. HTTP 优先复用已有 DTO；扩展公开契约时检查 Process、ReadModel 和应用调用方。调整命令恢复逻辑时，保留上述回执与重验规则。
4. 业务改动补充库单元测试，覆盖成功、失败、边界及有关幂等和版本冲突的路径。

## 验证要求

仅修改文档时，按[后端开发规则](../../../AGENTS.md#验证)只检查目标文件的 `git diff --check`。业务代码改动时，在 `backend/` 执行对应的定向检查；本 crate 的测试命令为：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked
```

代码变更还须遵循[统一质量门禁](../../README.md#质量门禁)。测试仅限库单元测试，不运行集成测试或依赖真实外部服务的测试。
