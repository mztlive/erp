# erp-sales：销售订单与销售变更

管理卖给客户的订单及后续变更，保存可编辑草稿、每次提交内容和正式生效版本。

销售单既需要支持持续编辑，也需要准确追溯审批时和生效时的内容。本领域维护销售状态、版本和变更规则，供采购、履约及财务引用。

## 使用场景

- 修改销售单草稿、冻结提交、正式版本和生命周期。
- 修改销售变更单、变更提交、生效或作废规则。

## 协作示例

业务员提交销售单时，本 crate 提供销售状态与冻结内容的规则；[erp-workflow](../erp-workflow/README.md) 维护审批运行，order_to_cash Process 协调完整用例。列表中的应收、采购和审批信息由 [erp-read-models](../erp-read-models/README.md) 组装。

## 负责的数据与能力

- 销售单草稿工作副本、冻结提交、正式修订和生命周期规则。
- 销售变更、变更提交及生效和作废规则。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 订单到收款与销售变更的跨域流程分别进入 erp-processes::order_to_cash 和 sales_change。
- 采购、履约、财务和审批事实通过窄合同取得；不得在本域直接维护外域事实。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/sales_order/mod.rs](src/service/sales_order/mod.rs) | SalesOrderService |
| [src/service/sales_review/mod.rs](src/service/sales_review/mod.rs) | SalesReviewService |
| [src/ports/mod.rs](src/ports/mod.rs) | 销售及变更消费方合同 |
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
cargo check -p erp-sales --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
