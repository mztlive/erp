# bpm：纯流程模型与状态引擎

根据流程定义和当前状态，计算审批下一步应该进入哪个节点、产生哪些事件和任务意图。

流程图校验与状态推进可以独立于 ERP 单据和数据库计算。调用方提供 ID、时间和人员资格，引擎返回待执行的状态变化计划。

## 使用场景

- 修改流程定义、节点连线、图校验或流程状态迁移规则。
- 调整启动、决策、阻塞、恢复、取消所返回的计划与事件。

## 协作示例

一个审批节点通过后，本 crate 计算后续状态并返回 TransitionPlan；[erp-workflow](../erp-workflow/README.md) 负责保存审批结果和管理 ERP 待办，Process 负责触发业务单据动作。引擎计算本身不会保存数据或发送通知。

## 负责的数据与能力

- 流程定义、节点、连线、运行实例、节点执行和参与者模型。
- 定义图验证、启动、决策、阻塞、恢复与取消的纯计算。
- TransitionPlan、BpmEvent 及提交所需任务意图。

## 使用与边界要求

1. 不得依赖 ERP 业务领域、MongoDB、HTTP、配置、ID 生成器、权限或通知客户端。
2. ID、时间和已收敛的人员资格由调用方传入；引擎不得读取系统时钟或自行生成 ID。
3. ERP 政策、工作项、通知与持久化由 erp-workflow 适配；跨域业务动作由 erp-processes 编排。
4. 调用方依据引擎返回的计划执行持久化；不可提交的不变式错误不得改写为可提交的状态。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/model/mod.rs](src/model/mod.rs) | 流程定义与运行模型 |
| [src/graph/mod.rs](src/graph/mod.rs) | DefinitionGraph 和图校验 |
| [src/engine/mod.rs](src/engine/mod.rs) | 状态操作、计划、事件与 Eligibility |
| [src/ids.rs](src/ids.rs) | 流程 ID |
| [src/error.rs](src/error.rs) | 模型错误出口 |

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p bpm --locked
env -u ERP_TEST_MONGO_URI cargo test -p bpm --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
