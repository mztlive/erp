# erp-inventory：库存与预占

管理每个仓库中 SKU 的库存余额、增减流水、预占和库存调整。

收货、发货和库存调整都会影响数量，必须共同遵守同一套库存记录与数量规则。预占用于记录已为业务用途保留的库存。

## 使用场景

- 调整库存余额、流水、预占及释放规则。
- 修改库存调整单、库存查询，或履约引起的库存写入。

## 协作示例

仓库收货后，收货单及实际验收数量归 [erp-fulfillment](../erp-fulfillment/README.md)，余额和入库流水归本 crate；仓库名称、处理人和 SKU 策略归 [erp-warehouse](../erp-warehouse/README.md)。完整收货过账由 Process 协调。

## 负责的数据与能力

- 库存余额、流水、预占、调整单及相关数量规则。
- 库存查询，以及调整和履约引起的事务内库存写入。

## 依赖与协作边界

术语与统一分工见[总索引](../README.md#代码中常见名称的含义)。

- 本领域的数据结构、业务规则、服务和数据库仓储在此维护。常规、构建和测试依赖均不得指向其他业务领域或组合层。
- 仓库、SKU、收货单、授权和审计事实通过 Port 获取。
- 跨域调整过账由 erp-processes::inventory_adjustment 编排；库存写入必须沿用调用方执行器。
- 需要其他领域的能力时，由组合层或应用将本域接口接到实际提供方；公开入口见 [src/lib.rs](src/lib.rs)。

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
