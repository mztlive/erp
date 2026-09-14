# erp-procurement：采购订单与采购责任

管理向供应商采购的订单：采购内容、数量、提交版本、变更，以及由谁负责采购。

采购需要区分可编辑草稿、冻结的审批提交和已生效版本，并维护采购对销售需求的覆盖关系。本领域统一维护这些状态与数量规则。

## 使用场景

- 修改采购单草稿、提交、生效、变更和作废规则。
- 调整采购创建依据、覆盖关系、采购分配或责任人规则。

## 协作示例

依据销售需求创建采购单时，Process 组合销售当前版本和合格供给；本 crate 维护采购订单规则与记录。[erp-supplier](../erp-supplier/README.md) 提供供应商资料，[erp-supply](../erp-supply/README.md) 提供 SKU 供给，[erp-finance](../erp-finance/README.md) 维护应付与付款。

## 负责的数据与能力

- 采购单、草稿编辑、冻结提交、正式化、变更和作废规则。
- 采购覆盖、创建依据、分配维护以及采购责任人解析。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 跨域采购到付款流程进入 erp-processes::procure_to_pay；供应商资料和供应供给由各自领域提供。
- 创建依据、覆盖及责任资格通过 Port 或输入事实传入；本域仓储只写本域集合。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/purchase_order/mod.rs](src/service/purchase_order/mod.rs) | PurchaseOrderService |
| [src/service/procurement_responsibility/mod.rs](src/service/procurement_responsibility/mod.rs) | 采购责任服务与解析 |
| [src/ports/mod.rs](src/ports/mod.rs) | 创建依据、覆盖、变更和责任合同 |
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
cargo check -p erp-procurement --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
