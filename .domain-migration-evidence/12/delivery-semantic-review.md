# 阶段 12 发货迁移语义核对证据

## 1. 固定输入与证据边界

| 项目 | 固定值 |
| --- | --- |
| 执行树 | `/private/tmp/erp-domain-crate-12-fulfillment` |
| Before | `0a297ab00d29b28047e05f780b4815c0a5456a2f`，简称 `0a297ab0` |
| Before 文件 | `backend/services/src/fulfillment/{delivery.rs,delivery_lines.rs,delivery_posting.rs}`，合计 963 行 |
| After | 当前未提交工作树的六个生产 leaf，见第 8 节内容指纹 |
| 核对方式 | 读取输入提交源码与迁后源码，逐符号对照；查询、视图与 NO_APPROVAL helper 另作命名空间和空白归一化比较 |
| 当前结论 | 所列最终实现未发现业务写序、首错、时间/ID 调用位置、来源类型或任务推进漂移 |
| 本分片验证 | 六个 leaf 定向 `rustfmt` 与 `git diff --check`；原测试入口保存及新增行为测试源码核对 |
| 未执行事项 | 未由本分片执行 Cargo、测试或数据库操作；统一门禁结果须引用集成负责人实际日志 |

本证据是静态语义核对及源码测试范围登记，不构成运行证明。**真实数据库运行未验证。** 不得将替身测试源码登记为真实事务回滚、并发防重、库存一致性或提交结果恢复已经验证。

## 2. 符号归属与实际提供方

以下路径均相对 `backend`。`domain` 指 `crates/erp-fulfillment/src/service`；`process` 指 `crates/erp-processes/src/fulfillment_execution`。

| Before 符号 | After 实际位置 | 归属及保持合同 |
| --- | --- | --- |
| `FulfillmentService::{delivery_list,delivery_detail}` | `domain/delivery.rs` 同名方法 | 本域订单及行仓储查询，保留过滤、排序、分页和读取顺序 |
| `impl From<Delivery> for DeliveryView`、`impl From<DeliveryLine> for DeliveryLineView` | `domain/delivery.rs` 同名转换 | 每个输出字段和转换表达式与 Before 一致 |
| `FulfillmentService::create_delivery` | `process/delivery.rs::FulfillmentProcess::create_delivery` | 组合入口调用 `self.domain().prepare_delivery` 后执行原创建根事务 |
| 原创建请求校验、表头及行构造 | `domain/delivery.rs::FulfillmentService::prepare_delivery` | 按原顺序校验请求、生成表头 ID、构造实体、注入行 ID、调用 `DeliveryLineBatch::build` |
| `FulfillmentService::update_delivery` | `process/delivery.rs::FulfillmentProcess::update_delivery` | 调用本域 `prepare_delivery_update`，随后审计构造、根事务、任务活动与审计写入 |
| 原更新准备和单据 CAS | `domain/delivery.rs::{prepare_delivery_update,persist_delivery}` | 请求校验、无事务加载、版本守卫、实体更新与调用方执行器 CAS |
| 创建业务表头及行写入 | `domain/delivery.rs::persist_created_delivery` | 实际仍调用本域 `fulfillment().create_delivery_with_lines` |
| `delivery_line_specs` | `domain/delivery_lines.rs::delivery_line_specs` | 逐请求行生成系统 ID，字段投影和原行序保持不变 |
| `receipt_reservation_specs` | `domain/delivery_lines.rs::receipt_reservation_specs` | 接收 `entity::facts::ReceiptReservationLineFact`；由调用方在原位置投影库存预占 ID、销售行 ID、预占数量 |
| `delivery_line_rule_source` | `domain/delivery_lines.rs::FulfillmentService::delivery_line_rule_source` | 稳定返回字符串 `entities::fulfillment::DeliveryLineBatch` 保持不变；字符串是 golden 数据，不是旧实体依赖 |
| NO_APPROVAL 决策、空适配器守卫、绑定命令和注册 helper | `process/delivery.rs` 中原同名函数 | 实际工作流提供方仍为 `erp_workflow` 与 `services::workflow_compose`；禁止绑定审批定义或启动审批 |
| `FulfillmentService::post_delivery` | `process/delivery_posting.rs::FulfillmentProcess::post_delivery` | 根事务、原时钟位置、仓发/直发分支、任务及审计 |
| 原过账加载与本域前置校验 | `domain/delivery_posting.rs::prepare_delivery_posting` | 读取发货单 → Draft 守卫 → 版本守卫 → 更新物流字段 → 读取行 → 非空检查 |
| 原发货状态迁移与 CAS | `domain/delivery_posting.rs::persist_posted_delivery` | 全部库存行或采购门槛通过后才 `mark_shipped` 并写入 |
| 原直发采购来源引用检查 | `domain/delivery_posting.rs::supplier_purchase_source` | 在直发分支原位置返回采购来源或原业务错误 |
| 原 `post_warehouse_ship_line` | `crates/erp-inventory/src/service/fulfillment/delivery.rs::{post_warehouse_ship_line,post_with_store}` | 完整库存校验、读取、预占消耗、库存写入下沉库存；真实 `MongoShipmentStore` 仍调用原 `InventoryExt` 仓储 |

`delivery_list`、`delivery_detail`、两个 `From` 实现，以及 `delivery_create_binding_decision`、`ensure_delivery_skips_approval_binding`、`ensure_delivery_has_no_adapter`、`delivery_bind_command`、`persist_unbound_delivery_document`、`register_created_delivery_document`，经仅命名空间和空白归一化后与输入提交相等。

## 3. 创建、更新与 NO_APPROVAL 顺序

### 3.1 创建准备与事务

1. 请求 `validate`。
2. `DeliveryId::new(next_id())`。
3. `Delivery::new`；原地址密文及指纹输入仍为 `None`。
4. `delivery_line_specs` 按原行序逐条生成 `DeliveryLineId`。
5. `DeliveryLineBatch::build` 执行原类型、行归属和编号规则。
6. 构造 `delivery.create` 审计对象。
7. 进入原 `with_transaction`。
8. 构造绑定命令与 `BusinessDocument`，确认政策为 `NO_APPROVAL`、跳过绑定、无审批适配器。
9. 调用原统一绑定端口；校验返回空绑定；登记无审批业务单据。
10. 以同一执行器创建发货表头与行。
11. 调用原 `ensure_fulfillment_task` 创建履约执行任务。
12. 写入原创建审计。

无审批合同只禁止创建审批任务；第 11 步原有履约执行任务保留。迁后没有查询发布定义、启动审批实例或新增审批动作。

### 3.2 更新

顺序固定为：请求校验 → `NoTransaction` 加载发货单 → 版本校验 → `Delivery::update` → 构造审计 → 根事务内发货单 CAS → `record_fulfillment_activity` → 审计写入。原版本冲突错误保持“数据已被其他请求修改，请刷新后重试”。

## 4. 过账前置校验与逐行顺序

### 4.1 根事务及读取首错

1. 根入口仍在事务前校验 `PostDeliveryRequest` 并拆出版本、承运方、物流号。
2. 根事务内以请求构造的 `DeliveryId` 加载发货单；不存在返回原 `NotFound`。
3. 先检查 `DeliveryState::Draft`，再检查版本；不交换这两个错误的优先级。
4. 应用原 `DeliveryUpdate`，再查询发货行。
5. 行为空返回“发货单没有行，无法过账”。
6. 只有上述准备成功后，process 才在原位置调用一次 `Instant::now()`。
7. 仓发按行顺序逐条调用库存；直发只进入采购来源、采购状态和 PREPAY 检查。

当前生产 `execute_posting` 使用 `PostingStep::WarehouseLine(index)`，不是一次性整体行步骤。第 N 行失败后不调用第 N+1 行，不提前校验后续行的预占、仓库或余额。

### 4.2 库存单行读写顺序

`WarehouseShipmentLine` 仅携发货单 ID、发货行 ID、销售行 ID、可选预占 ID、可选仓库 ID和数量。该临时合同不持有履约完整聚合，不参与持久化编码。

| 次序 | 原生产行为与错误边界 | After 实际位置 |
| --- | --- | --- |
| 1 | 检查预占引用；缺失时不读取仓储 | `post_with_store` |
| 2 | 读取预占；不存在返回“库存预占不存在” | `ShipmentStore::reservation` → `stock_reservations().find_by_id` |
| 3 | 先检查销售明细归属，再检查预占数量 | `post_with_store` |
| 4 | 检查发货仓存在，再检查预占仓与发货仓一致 | `post_with_store` |
| 5 | 按仓库和预占 SKU 读取余额；缺失时不消耗预占 | `ShipmentStore::balance` → `stock_balances().find_by_dimensions` |
| 6 | 条件消耗预占 | `consume` → `consume_quantity` |
| 7 | 生成 Consume 流水 ID、构造并写入预占流水 | `reservation_entry` → `stock_reservation_entries().create` |
| 8 | 先释放预占余额，恢复这笔预占对应的可用量 | `release_reserved` → 同名库存仓储方法 |
| 9 | 再扣减可用量及在库量 | `deduct_available` → 同名库存仓储方法 |
| 10 | 生成库存流水 ID和 `fact_no`，构造并写入出库事实 | `movement` → `stock_movements().create` |
| 11 | 写回余额最后流水引用 | `last_movement` → `apply_last_movement` |

第 8、9 步不得交换或合并。库存原注释说明预占建立时已扣减 available，仓发必须先释放再扣减，该注释随真实库存算法保留。

四个条件写入返回 `false` 时，仍依次使用原错误：“预占数量不足或状态不符，无法消耗”“预占余额不足，无法发货”“可用库存不足，无法发货”“库存余额行不存在”。每个读取、条件写入、实体构造和流水写入均保留首个错误并停止后续步骤。

### 4.3 直发与后续跨域步骤

- 直发在原时钟读取之后取得采购来源，读取原采购单，调用 process `purchase_context::{ensure_po_fulfillable,ensure_prepay_gate}`；没有调用自有库存写入。
- 库存逐行处理或直发门槛成功后，顺序固定为：`mark_shipped` / 发货单 CAS → `task::complete_fulfillment_task` → `customer_acceptance::task::ensure_customer_acceptance_task(DeliveryAvailable)` → `delivery.post` 审计。
- 原实现没有在本入口直接更新销售进度；迁后也未增加该步骤。
- 上述所有仓储及任务调用复用根事务传入的同一个 `Executor`；领域及库存 leaf 不创建事务。

## 5. 时间、ID 与事实来源

| 对象 | 最终合同 |
| --- | --- |
| 创建发货表头及行 | 请求校验后才生成表头 ID；表头构造成功后才逐行生成行 ID并执行批量规则 |
| 入库预占投影的仓发行 | 使用原预占 `base.id` 对应的消费事实 `reservation_id`；销售行与预占数量不变，行 ID仍在 `receipt_reservation_specs` 原逐条映射位置生成 |
| 过账业务时间 | 发货单及全部行读取、空行校验成功之后，在 process 中取得一次 `Instant::now()` |
| Consume 流水 | 条件消耗预占成功后才生成 ID、执行原 `StockReservationEntry::new` 并写入；`source_document_id` 仍为原加载发货单 ID |
| 出库流水 | 释放预占与扣减可用量成功后才生成 `StockMovementId`；`fact_no` 仍在原数据构造位置调用 `next_id()` |
| 出库分类 | 保留 `MovementType::WarehouseShipOut`、`MovementDirection::Decrease`、`SourceType::Erp` |
| 出库时间与操作人 | `occurred_at`、`recorded_at` 共用根流程取得的原时间；`recorded_by` 使用原审计操作人 ID |
| 出库来源 | 单据 ID与行 ID仍来自当前发货表头及当前行；SKU来自预占；仓库通过原归属校验；冲正引用仍为 `None` |
| 过账审计资源 | `DeliveryPosting` 保存原请求构造的 `delivery_id`；审计 `resource_id` 明确使用该请求 ID 的 `to_string()` |
| 最后流水引用 | 使用本行已成功写入的 `movement.base.id`，在相同执行器中调用原 `apply_last_movement` |

仅实际提供方和数据传递边界改变。库存预占与余额查询仍在当前行原位置执行；没有提前加载后续行或新增合并查询。

## 6. 测试入口保存与新增覆盖

原三个源文件共 5 个内联测试，迁后全部保存，无遗漏：

| 迁后位置 | 原测试数 | 新增测试数 |
| --- | ---: | ---: |
| `erp-fulfillment/service/delivery.rs` | 2 | 0 |
| `erp-processes/fulfillment_execution/delivery.rs` | 3 | 0 |
| `erp-processes/fulfillment_execution/delivery_posting.rs` | 0 | 3 |
| `erp-inventory/service/fulfillment/delivery.rs` | 0 | 6 |
| 合计 | 5 | 9 |

新增测试使用真实生产顺序入口及非零大小 `TestExecutor { _identity: u8 }`。执行器身份通过原实例地址比较，不使用零大小替身。

- `warehouse_posting_preserves_each_line_and_following_task_order`：核对逐行库存、发货 CAS、执行任务、验收任务、审计顺序。
- `supplier_direct_posting_checks_purchase_before_delivery_without_stock`：核对直发只执行采购门槛和履约后续步骤。
- `posting_failure_stops_before_next_line_and_later_domains`：对三条仓发行和后续步骤逐个失败注入；包含中间 `WarehouseLine(1)` 失败不访问 `WarehouseLine(2)` 的前缀断言。
- `shipment_releases_reserved_before_deducting_and_keeps_frozen_facts`：从 available=0、reserved=2、on_hand=2 开始执行生产算法，验证先释放再扣减，并核对源引用、时间、数量、类型和操作人。
- `shipment_stops_after_each_repository_failure_with_same_executor`：逐个注入八个仓储边界的错误，核对原错及执行前缀。
- `shipment_keeps_conditional_write_failure_messages_and_stops`：核对四个 `false` 分支的原业务错误及停止位置。
- `shipment_requires_reservation_before_any_read`：缺少预占引用时不产生任何仓储读取。
- `shipment_preserves_reservation_quantity_and_warehouse_first_error_order`：同时提供多个非法事实，核对销售归属、数量和仓库的原首错顺序。
- `shipment_preserves_missing_stock_fact_errors`：核对预占及余额缺失的原错误和停止边界。

本表只记录已经落盘的测试入口和断言。本分片没有执行这些测试，不登记通过数量；不修改任何历史 `tests/**` 文件。

## 7. 删除、依赖及验证登记

- Before 三个旧源文件均已删除，无第二份业务实现。
- 本域三个 leaf 不导入库存、采购、工作流、身份、审计或旧三层；稳定 golden 字符串不构成依赖。
- 库存 leaf 不导入履约实体、服务或 DTO；只消费最小发货事实。
- 根事务、NO_APPROVAL、采购状态/PREPAY、执行任务、验收任务与审计均留 process。
- 本分片未修改 Cargo、共享根、HTTP、DTO 定义或历史测试；共享模块登记由对应集成负责人完成。
- 六个最终 leaf 已完成定向格式化与差异空白检查；Cargo和测试结果由统一集成日志单独登记。

## 8. After 内容指纹

下表 SHA-256 对应本报告登记时的最终文件内容。文件变化后须核对差异并刷新证据，不得以指纹替代编译、测试或真实数据库验收。

| After 文件（相对仓库根） | SHA-256 |
| --- | --- |
| `backend/crates/erp-fulfillment/src/service/delivery.rs` | `00aeb1f5413f02c4a7bc46dd2ae512d0d18220afcd39de757cb1292a6324901b` |
| `backend/crates/erp-fulfillment/src/service/delivery_lines.rs` | `b664384fdc14b98efa8580074c9752482e6b278f34aa459457e8a997cc3ce3ff` |
| `backend/crates/erp-fulfillment/src/service/delivery_posting.rs` | `f201c74f808bc7bce779d54b199ce3d898fa978eef47bfab3170a0e1db6f7770` |
| `backend/crates/erp-processes/src/fulfillment_execution/delivery.rs` | `4e9fb80f25df2f902b9c6f4b43e38b241417e4b9fe234eef97f4e9a61ad4083c` |
| `backend/crates/erp-processes/src/fulfillment_execution/delivery_posting.rs` | `a70303db50397ea847affcff834c0d604ab5d09e1b753893b39a3c3e6f42ff0a` |
| `backend/crates/erp-inventory/src/service/fulfillment/delivery.rs` | `e77bebc6f3f7764a9896fd014d9f75378eb7d5b915d32d78dd220a7dc4d7e118` |

本证据的最终边界固定为：静态语义核对完成；本分片未运行 Cargo 或测试；真实数据库运行未验证。
