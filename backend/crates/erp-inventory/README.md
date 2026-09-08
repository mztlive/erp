# erp-inventory：库存与预占

## 职责合同

- 库存余额、流水、预占、调整单及相关数量规则。
- 库存查询，以及调整和履约引起的事务内库存写入。

## 依赖与协作边界

- 本 crate 拥有本域实体、DTO、规则、Service、Repository、集合访问器和索引。normal、build、dev 依赖均不得指向其他业务领域或组合层。
- 仓库、SKU、收货单、授权和审计事实通过 Port 获取。
- 跨域调整过账由 erp-processes::inventory_adjustment 编排；库存写入必须沿用调用方执行器。
- 外域适配器在组合层或应用组合根装配；公开入口以 [src/lib.rs](src/lib.rs) 为准。

## 代码入口

| 入口 | 用途 |
| --- | --- |
| [src/service/inventory/mod.rs](src/service/inventory/mod.rs) | InventoryService 与调整写入入口 |
| [src/service/fulfillment.rs](src/service/fulfillment.rs) | 履约库存合同 |
| [src/ports/mod.rs](src/ports/mod.rs) | 仓库、目录、履约和授权事实 |
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
cargo check -p erp-inventory --locked
env -u ERP_TEST_MONGO_URI cargo test -p erp-inventory --lib --locked
```

代码变更还须执行[统一质量门禁](../README.md#质量门禁)。测试范围限定为库单元测试，不执行集成测试或真实外部服务测试。
