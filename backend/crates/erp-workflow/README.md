# erp-workflow：审批集成与工作项

将通用流程引擎接入 ERP，管理业务审批、审批参与者、业务单据关联和人工待办任务。

流程引擎能计算节点变化，但 ERP 还需要审批政策、账号资格、单据绑定、数据库记录和通知。本 crate 负责这些 ERP 层面的流程与任务能力。

## 使用场景

- 修改审批定义、运行、ERP 审批政策或通知待发送记录。
- 修改工作项创建、关闭、转派，以及单据参与者和关联。

## 协作示例

采购单提交审批后，[bpm](../bpm/README.md) 计算流程下一步，本 crate 持久化审批并管理工作项；审批通过后的采购生效由 [erp-processes](../erp-processes/README.md) 调用采购领域。工作台展示多种待办的业务摘要由 [erp-read-models](../erp-read-models/README.md) 提供。

## 负责的数据与能力

- ERP 审批政策、审批定义与运行服务、通知 Outbox 和 BPM 持久化适配。
- 业务单据登记、参与者与关系，以及工作项创建、关闭和转派。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 流程图和纯状态迁移规则复用 bpm；账号资格、单据事实和领域动作通过 Port 取得。
- 跨域审批动作由 erp-processes 分派；工作台聚合查询与正式任务事实读取由 erp-read-models 负责。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/approval/mod.rs](src/service/approval/mod.rs) | 审批定义、运行与政策入口 |
| [src/service/work_item/mod.rs](src/service/work_item/mod.rs) | 工作项命令入口 |
| [src/repository/bpm.rs](src/repository/bpm.rs) | BPM 持久化适配 |
| [src/entity/mod.rs](src/entity/mod.rs) | 本域实体、值对象和确定性规则 |
| [src/repository/mod.rs](src/repository/mod.rs) | 本域 MongoDB 仓储与集合访问器 |
| [src/indexes/mod.rs](src/indexes/mod.rs) | 公开索引注册入口 |

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入本域实体或值对象；Service 组织本域用例。
2. Repository 使用调用方传入的 `persistence_core::Executor`；跨集合原子写入由本域用例或 Process 控制事务。
3. 新增或调整集合查询时同步评估索引；组合根复用本域公开索引入口，保持既有逐集合注册顺序。
4. HTTP 请求和响应优先复用本域 DTO；扩展公开合同须同步检查 Process、ReadModel 和应用调用方。
5. 业务改动补充本域库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突路径。

## 验证要求

以下命令在 `backend/` 目录执行：

```bash
cargo check -p erp-workflow --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
