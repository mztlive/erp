# bpm：纯流程模型与状态引擎

## 职责合同

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
