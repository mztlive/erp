# erp-integration：集成消息与差异处理

记录外部系统入站消息、处理失败任务和对账差异，并管理处理决定及其证据。

外部消息与内部业务结果之间出现失败或差异时，需要可追踪的问题对象、处理状态和决定记录，不能只依靠程序日志。

## 使用场景

- 调整入站消息、集成错误任务或对账差异的状态与查询。
- 修改差异处理决定的校验、证据要求和持久化。

## 协作示例

外部消息处理失败后，本 crate 保存错误任务及后续处理决定；若解决问题需要变更正式单据，由 [erp-processes](../erp-processes/README.md) 的 integration_resolution 调用相应领域。供应商连接配置和调用能力归 [erp-supply](../erp-supply/README.md)。

## 负责的数据与能力

- 入站消息、集成错误任务、对账差异和处理决定事实。
- 处理决定校验、证据合同及本域状态持久化。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 跨域差异解决由 erp-processes::integration_resolution 编排；证据所需权威事实通过 Port 取得。
- 外部业务单据继续由各自拥有领域维护，不得在集成域绕过其规则修正事实。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/mod.rs](src/service/mod.rs) | IntegrationOpsService |
| [src/ports/evidence.rs](src/ports/evidence.rs) | 证据事实合同 |
| [src/entity/integration_ops/mod.rs](src/entity/integration_ops/mod.rs) | 消息、任务、差异和决定规则 |
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
cargo check -p erp-integration --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
