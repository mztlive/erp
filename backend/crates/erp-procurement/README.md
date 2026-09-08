# erp-procurement：采购订单与采购责任

## 职责合同

- 采购单、草稿编辑、冻结提交、正式化、变更和作废规则。
- 采购覆盖、创建依据、分配维护以及采购责任人解析。

## 依赖与协作边界

- 本 crate 拥有本域实体、DTO、规则、Service、Repository、集合访问器和索引。normal、build、dev 依赖均不得指向其他业务领域或组合层。
- 跨域采购到付款流程进入 erp-processes::procure_to_pay；供应商资料和供应供给由各自领域提供。
- 创建依据、覆盖及责任资格通过 Port 或输入事实传入；本域仓储只写本域集合。
- 外域适配器在组合层或应用组合根装配；公开入口以 [src/lib.rs](src/lib.rs) 为准。

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
