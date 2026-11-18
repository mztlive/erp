# erp-inventory：库存与预占

## 职责

本库管理自有仓库中每个 SKU 的账面现存、有效预占和可用量，记录库存增减与预占变动，并维护库存调整单及其明细。余额、流水、预占和调整台账的本域查询也由本库提供。

### 负责的数据与能力

| MongoDB 集合 | 保存内容 |
| --- | --- |
| `stock_balances`、`stock_movements` | 按仓库和 SKU 保存当前余额；按业务来源追加现存量增减的正式流水。 |
| `stock_reservations`、`stock_reservation_entries` | 按稳定销售明细保存预占的来源、剩余量、已消耗量、已释放量及状态；追加建立、消耗、释放等分录。 |
| `stock_adjustments`、`stock_adjustment_lines`、`stock_adjustment_cancellations` | 保存调整单状态、原因和数量明细，以及按审批实例保存的不可变撤回事实；不从审计正文恢复撤回结果。 |

### 使用场景

修改库存数量规则、预占或释放规则、调整单和库存台账时，先查本库的实体与仓储。涉及收货、发货或审批过账时，再查负责开启或接续事务的 Process。

### 协作示例

采购收货时，[erp-fulfillment](../erp-fulfillment/README.md) 保存收货单及实收、合格、不合格数量，本库只对合格数量入账。[erp-warehouse](../erp-warehouse/README.md) 维护仓库资料、收发货经办人和仓库 SKU 策略；[erp-catalog](../erp-catalog/README.md) 维护 SKU 主数据。[erp-fulfillment-execution](../../orchestration/erp-fulfillment-execution/README.md) 协调跨域过账，不接管这些事实。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 命名、依赖与公开接口；导出实体、DTO、端口、仓储、`InventoryService` 和调整写入入口。 |
| [src/entity/mod.rs](src/entity/mod.rs) | 实体和值对象，约束本域数量、来源与状态迁移。 |
| [src/dto/mod.rs](src/dto/mod.rs) | 请求、查询参数、视图和分页范围元数据，供 HTTP 与调用方复用。 |
| [src/service/inventory/mod.rs](src/service/inventory/mod.rs) | 库存查询、调整单修改及 `apply_posted_adjustment`；[src/service/fulfillment.rs](src/service/fulfillment.rs) 提供入库和仓发的事务内库存入口。 |
| [src/repository/mod.rs](src/repository/mod.rs)、[src/indexes/mod.rs](src/indexes/mod.rs) | 集合访问器、授权分页与条件写入；通过 `indexes::ensure` 注册索引。 |
| [src/ports/mod.rs](src/ports/mod.rs)、[src/authorization.rs](src/authorization.rs) | 仓库、SKU、入库单号、调整审批人员、授权和审计的窄接口；本域向 identity 提供实际库存对象及其仓库归属。 |

## 架构设计

### 实体与仓储规则

- **余额。** [`StockBalance`](src/entity/inventory/stock_balance.rs) 校验现存、预占、可用量均非负，且 `可用量 = 现存量 − 预占量`。[余额仓储](src/repository/inventory/balance.rs) 对未删除记录做单文档原子更新：入库同时增加现存和可用量；预占在可用量足够时将其转入预占；释放在预占量足够时转回可用量；扣减在可用量足够时同时减少现存和可用量。数量条件与写入一起执行，不用先前读取的数量代替写入条件；数量或最后流水更新时递增余额版本。
- **流水。** [`StockMovement`](src/entity/inventory/stock_movement.rs) 保存正数数量、独立的增减方向、来源单据和行、发生时间与记录信息。实体校验流水类型与方向，流水只追加，纠错不覆盖原记录。`last_movement_id` 与正式流水及数量变动同事务更新；建立或释放预占本身不改变现存量。
- **预占。** [`StockReservation`](src/entity/inventory/stock_reservation.rs) 校验来源字段互斥及状态与数量一致；预占不自动过期，仅供其归属销售明细消耗。[预占仓储](src/repository/inventory/reservation.rs) 只消耗或释放有效、部分消耗的记录，消耗时检查剩余量并更新状态；`release_quantity` 只允许全额释放该条剩余预占。预占状态变动与相应余额更新及不可变分录在同一事务提交。
- **索引。** [本域索引](src/indexes/inventory.rs) 保证余额 `(warehouse_id, sku_id)` 全局唯一，流水按 `(source_document_id, source_line_id, movement_type)` 防止重复入账；预占按来源分别对「入库行 + 采购销售分配」或「现有库存分配动作 ID」设部分唯一索引。唯一键不能替代状态、版本或数量校验，也不等同于命令回执重放。

### 实际写入链路

1. **采购收货过账。** [收货 Process](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/purchase_receipt_posting.rs) 开启根事务，核验办理资格、草稿版本和采购收货数量；只有正数合格量交给 [`post_receipt_stock`](src/service/fulfillment.rs)。本库按仓库和 SKU 查找余额，存在则同时增加现存和可用量，否则新建零预占余额；随后追加 `PurchaseReceiptIn` 流水并登记最后流水。存在采购销售分配时，Process 取得按比例分摊的份额（末项吸收舍入尾差），本库逐条建立归属稳定销售明细的预占、冻结可用量并追加 `Establish` 分录。收货状态、采购进度、对应的履约任务和成功审计仍由 Process 协调同事务写入。
2. **分配现有库存。** [选源 Process](../../orchestration/erp-procure-to-pay/src/procure_to_pay/sourcing_create/stock_posting.rs) 沿原事务按余额汇总分配数量，先用本库仓储冻结可用量，再批量写入预占与 `Establish` 分录；不生成入库流水，也不伪造采购引用。
3. **仓发过账。** [发货 Process](../../orchestration/erp-fulfillment-execution/src/fulfillment_execution/delivery_posting.rs) 在根事务逐行调用 [`post_warehouse_ship_line`](src/service/fulfillment/delivery.rs)。库存入口核对预占的销售明细、剩余量和发货仓，消耗预占并写 `Consume` 分录，再把余额中同量预占转回可用量，随后扣减现存和可用量。必须先释放余额预占再扣减，因为待发数量在建立预占时已从可用量中扣除。最后追加 `WarehouseShipOut` 流水并登记最后流水。Process 继续推进发货状态、任务和审计；供应商直发不写自有库存。
4. **库存调整。** [`InventoryService::update_stock_adjustment`](src/service/inventory/update.rs) 在本域事务按实际调整单授权、核对期望版本，只编辑草稿或已驳回的单据，并同事务保存表头、变更明细和审计，不改变库存。审批最终通过时，[调整 Process](../../orchestration/erp-fulfillment-execution/src/inventory_adjustment/post.rs) 沿用审批决定事务，核对审批中状态及冻结的实例、执行、任务和主题版本，再调用 [`apply_posted_adjustment`](src/service/inventory/stock_write.rs)。本库逐行校验原因和方向、登记流水并更新已有余额：盘盈增加现存和可用量；盘亏、损坏先按建立时间全额释放同仓同 SKU 的可操作预占，累计释放达到或超过扣减量即停止，逐条同步余额并记 `Release` 分录，再按可用量扣减。每行同步最后流水；全部成功才保存已过账状态，成功审计由 Process 在原事务写入。

本域用例或跨域 Process 负责事务边界；审批回调沿用运行时的事务。事务内的授权、业务读取、库存步骤及仓储共用调用方的 `persistence_core::Executor`，仓储和事务内库存入口不得另开事务。库存入口将余额缺失、关键条件写入未命中或仓储失败交回根事务处理，不独立提交部分库存结果。

### 授权与读取

- 库存列表必须使用 [`InventoryQuery`](src/ports/authorization.rs) 的完整授权阶段，先授权再分页与计数，两者口径一致。业务筛选只能收窄结果。[库存 Mapping](src/repository/inventory/authorization.rs) 接入[仓库领域提供的 Mapping](../erp-warehouse/src/authorization.rs)，不预加载账号全部授权仓库 ID。[仓储](src/repository/inventory/shared.rs) 在原执行器校验查询前后的凭证边界，Service 在交付前复验授权依赖。
- [余额列表](src/service/inventory/balance.rs) 只对当前页仓库批量检查创建调整按钮资格，在原执行器取证和复验。按钮资格不是命令授权。[从余额创建调整](../../orchestration/erp-fulfillment-execution/src/inventory_adjustment/create.rs) 必须在原写事务对实际仓库调用 `require_warehouse`；修改、提交已有调整单时，则在原事务由 identity 按真实调整单与仓库来源完成固定对象授权。
- 审批管理走[工作流的固定管理入口](../erp-workflow/src/service/approval/scope.rs)和 [identity 的对象授权](../erp-identity/src/service/authorization/approvals.rs)。由 identity 按实际冻结类型在同一次对象检查中证明固定审批动作、审批实例读取和该类型管理资格，并校验实际调整单的仓库来源范围；不得分别判定后合并。普通撤回使用[专用对象授权分支](../erp-identity/src/service/authorization/approval_cancel.rs)，不套用通用管理分支。

### 依赖与协作边界

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。本库可直接使用 `erp-identity` 的公开接口；除该例外，常规、构建和测试依赖均不得直接指向其他业务领域或组合层。需要外域事实时，用窄 Port 或调用方传入的事实；[应用接线](../../../apps/erp-server/src/wiring/inventory.rs) 注入授权、审计和其他外域事实的实现。

## 修改执行要求

1. 无 I/O 的校验、状态迁移和不变式放在本域实体或值对象；单域用例由 Service 组织，跨领域原子写入由 Process 协调。
2. 新增或调整查询时同步评估索引；组合根复用本域公开索引入口并保持既有逐集合注册顺序。
3. HTTP 请求和响应优先复用本域 DTO；扩展公开合同时同步检查 Process、ReadModel 和应用调用方。
4. 业务代码改动补充本域库单元测试，覆盖成功、失败、边界及适用的幂等或版本冲突路径。

## 验证要求

在 `backend/` 执行本库单元测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-inventory --lib --locked
```

代码改动遵循[后端定向验证要求](../../../AGENTS.md#验证)及 [CI 统一质量门禁](../../README.md#质量门禁)。只运行库单元测试，不运行集成测试、忽略测试或依赖真实 MongoDB 或其他外部服务的测试。仅修改本 README 时，只对本文件运行 `git diff --check`。
