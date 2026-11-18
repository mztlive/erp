# background-runtime：后台执行与退出排空

## 职责

`background-runtime` 是后台 Worker 的通用执行库：定义任务适配器合同，提供按登记顺序轮询的 `BackgroundRunner`、独立周期循环 `periodic` 和执行切片预算 `ExecutionBudget`。它负责何时调用任务、何时停止轮询并等待收尾，不决定任务是否到期、谁能执行或如何恢复。

本 crate 仅依赖 `async-trait`、`tokio` 和 `tracing`，不依赖 ERP 领域、编排、读模型、具体供应商协议或应用。业务任务的认领、授权、处理、持久化和恢复归所属编排库；配置、资源注入、任务注册及进程生命周期归应用装配。本 crate 不创建业务配置、数据库连接或具体任务注册表。

<a name="代码入口"></a>

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | 包配置与依赖，关闭集成测试的自动发现。 |
| [src/lib.rs](src/lib.rs) | 统一导出 `BackgroundTaskAdapter`、`TaskError`、`TaskResult`、`BackgroundRunner`、`ExecutionBudget` 和 `periodic`。 |
| [src/adapter.rs](src/adapter.rs) | 任务名称、到期工作轮询及退出排空合同。 |
| [src/runner.rs](src/runner.rs) | 登记任务、执行单轮或常驻轮询、记录结果并排空适配器。 |
| [src/periodic.rs](src/periodic.rs) | 串行周期循环及停止信号判定。 |
| [src/budget.rs](src/budget.rs) | 按完成行数和耗时判断是否结束当前切片。 |

## 架构设计

### 使用与依赖合同

- `BackgroundTaskAdapter: Send + Sync`。`name()` 提供稳定的任务名，用于将轮询耗时、认领计数和错误归到对应任务。
- `run_due()` 认领并处理一批到期工作，可以等待工作完成，也可以派生异步工作并自行跟踪。`Ok(usize)` 报告本轮认领口径的数量，不是成功完成数；runner 不复核该数字。例如，[商品导入](../../orchestration/erp-data-import/src/product_import/execute.rs) 返回查出的候选任务数，实际执行时仍可能因并发认领而跳过。
- 认领查询等轮次级故障通过 `TaskResult<T>`（即 `Result<T, TaskError>`）返回，由 runner 记录；`TaskError` 保留原错误类型及其来源链。单项失败由适配器记录详情并按业务规则处置，不应因此中断整轮。
- `drain()` 等待该适配器已派生的工作收尾；没有派生工作的适配器可使用默认空实现。结果仍由原任务保存，排空不重新认领或重放业务。

### 顺序轮询与独立周期循环

`register` 追加一个具体适配器，`register_shared` 追加已有的 `Arc`，供其他入口共享同一实例。单个 runner 循环按登记顺序逐个等待 `run_due()` 返回，各轮不重叠；这不限制适配器自行派生的并发工作，也不提供跨进程互斥。某个适配器返回错误时，runner 记录任务名、耗时和错误，将该项计作 `0`，继续处理后续适配器。成功日志中的 `success_count` 仍是适配器报告的认领数。

`run_once()` 立即执行一轮，按登记顺序返回 `(任务名, 认领数)`，不等待间隔、不接收停止信号，也不调用 `drain()`。常驻 `run(stop)` 用 `periodic` 驱动：未停止时立即开始首轮，每轮结束后再等待配置的间隔，不按固定时刻对齐或补跑。也可直接用 `periodic` 运行独立的有界回调；回调自行处理和记录业务错误，运行时不捕获 panic。例如，[来源媒体 Worker](../../orchestration/erp-supplier-catalog/src/source_media/worker.rs) 独立轮询，并在条目边界检查停止，完成当前条目的写入和清理后退出。

### 执行切片预算

`ExecutionBudget::new(max_rows, duration)` 从创建时开始计时。`after_row()` 将当前行计入预算，达到行数上限或耗尽软时限即返回 `true`。预算不会阻止首行获得执行机会，也不是超时取消；调用方应在当前行及其进度保存后才让出，后续轮询从持久化状态继续。预算不负责事务、检查点或解除认领。

例如，[导入业务配置](../../orchestration/erp-data-import/src/background/import_budget.rs) 为每个任务的一个切片设置 10 行、1 秒的软预算。[品牌导入逐行实现](../../orchestration/erp-data-import/src/brand_import/rows.rs) 只处理尚无结果的行，每行提交后检查预算，到达切片边界后释放认领，已保存结果不受影响。

### 停止、排空与应用装配

停止信号为 `watch::Receiver<bool>`：值为 `true` 或所有发送端关闭均视为停止。`periodic` 在轮次边界检查，间隔等待可由信号变化唤醒；runner 还在调用每个适配器前检查。不再开始下一轮或调用下一个适配器，但不会取消正在等待的 `run_due()` 或独立回调；若需在当前批次内更早停止领取，由业务在其安全边界检查。常驻轮询退出后，runner 按登记顺序等待所有适配器的 `drain()`，不在通用层设置排空超时。

[应用 Worker](../../../apps/erp-server/src/worker/mod.rs) 复用 `wiring` 已装配的资源和业务入口，负责按顺序登记具体任务、启动循环、广播停止信号并监督意外退出。以[供应商目录适配器](../../orchestration/erp-supplier-catalog/src/background/supplier_catalog_adapter.rs)为例，应用将同一实例交给 runner 和 [XXL-JOB 入口](../../../apps/erp-server/src/worker/xxl/mod.rs)：XXL-JOB 触发生成到期任务并持久化，runner 轮询消费和恢复已有任务；适配器自行控制派生工作的并发和排空，超时后沿原检查点恢复。业务继续按原有顺序认领、处理和保存，不因预算耗尽或退出丢弃已持久化工作。

[应用进程](../../../apps/erp-server/src/runtime/server.rs) 先关闭新工作入口，再等待 Worker 与 HTTP 排空；当前总预算为 60 秒，超时返回进程错误。预算不属于 runner，也不保证所有业务任务在关停前完成；未完成工作按原持久状态恢复或核实。详见[服务端执行与退出合同](../../../apps/erp-server/README.md#执行与退出合同)。

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对变更文件运行 `git diff --check`。修改代码时，在 `backend/` 执行库单元测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p background-runtime --lib --locked
```

依赖、可见性或执行合同变更按[统一质量门禁](../../README.md#质量门禁)执行对应检查。不运行集成测试或依赖真实数据库、外部服务的验收。
