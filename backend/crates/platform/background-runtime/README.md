# background-runtime：后台执行与退出排空

为应用和业务任务提供顺序轮询、周期执行、停止排空及执行切片预算。具体业务认领、持久化、授权和恢复留在任务所属编排库；任务注册和生命周期由应用装配。

## 使用与依赖合同

1. 本 crate 只提供 `BackgroundTaskAdapter`、`TaskError`、`TaskResult`、`BackgroundRunner`、`ExecutionBudget` 和 `periodic` 等通用执行合同。
2. 不依赖 ERP 领域、编排、读模型、具体供应商协议或应用；不得在 runner 中解释业务身份或任务状态。
3. 任务实现按原顺序执行认领、处理和结果保存。预算到期、停止信号与 drain 保留现有切片和恢复语义，不得以退出为由丢弃已持久化工作。
4. 应用负责注入具体任务和停止信号；不得由本 crate 创建业务配置、数据库连接或具体任务注册表。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/adapter.rs](src/adapter.rs) | 通用任务执行接口与错误合同 |
| [src/runner.rs](src/runner.rs) | 顺序执行、停止及排空 |
| [src/periodic.rs](src/periodic.rs) | 周期调度与停止 |
| [src/budget.rs](src/budget.rs) | 执行切片预算 |

## 验证要求

在 `backend/` 执行 `env -u ERP_TEST_MONGO_URI cargo test -p background-runtime --lib --locked`。依赖、可见性或执行合同变更须执行[统一质量门禁](../../README.md#质量门禁)。只执行库单元测试，不运行真实数据库或外部服务验收。
