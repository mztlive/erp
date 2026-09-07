# 阶段 12 实物收货与发货执行合同

## 1. 固定范围与验收边界

- 输入提交：`0a297ab00d29b28047e05f780b4815c0a5456a2f`。
- 执行树：`/private/tmp/erp-domain-crate-12-fulfillment`。
- 拆迁源：`backend/services/src/fulfillment/{purchase_receipt.rs,purchase_receipt_lines.rs,purchase_receipt_posting.rs,purchase_context.rs,delivery.rs,delivery_lines.rs,delivery_posting.rs}`；七个旧文件全部删除。
- 本合同适用于实物收货、仓库发货、供应商直发及其采购来源适配。DTO 定义、履约实体/仓储、共享根和外部调用接线由对应分片提供。
- 原 15 个内联测试名称全部保留；新 18 个测试已落盘，合计 33 个。测试不得替换历史 `tests/**`，不得将源码登记当作运行通过。
- 本分片完成定向 `rustfmt` 和 `git diff --check`，未运行 Cargo 或 MongoDB。统一编译、Clippy 和内联测试通过状态必须引用集成负责人的实际日志。
- 发货完整逐符号核对以 `/private/tmp/fulfillment12-delivery-semantic-review.md` 为补充合同。

## 2. 唯一出口与符号归属

以下 Rust 路径均为实际生产出口；不保留旧服务转发。

| 原符号/职责 | 最终出口 | 行为边界 |
| --- | --- | --- |
| `FulfillmentService::{purchase_receipt_list,purchase_receipt_detail}` | `erp_fulfillment::service::FulfillmentService` 同名方法，`service/purchase_receipt.rs` | 原分页、排序、表头/行查询及 DTO 映射 |
| `FulfillmentService::{delivery_list,delivery_detail}` | `erp_fulfillment::service::FulfillmentService` 同名方法，`service/delivery.rs` | 原单域查询与映射 |
| 收货 create/update/post 根入口 | `erp_processes::fulfillment_execution::FulfillmentProcess::{create_purchase_receipt,update_purchase_receipt,post_purchase_receipt}` | 保持原参数、返回 DTO、根事务、任务和审计 |
| 发货 create/update/post 根入口 | 同一 `FulfillmentProcess::{create_delivery,update_delivery,post_delivery}` | 保持原参数、返回 DTO 和外部注入 |
| 收货创建构造 | `FulfillmentService::prepare_purchase_receipt(req)` | 返回 `(PurchaseReceipt, Vec<PurchaseReceiptLine>)`；原校验和 ID 生成时点 |
| 收货更新准备 | `FulfillmentService::prepare_purchase_receipt_update(id, req)` | 原无事务读取、版本、冻结仓库和实体更新顺序 |
| 收货本域创建/更新写入 | `FulfillmentService::{persist_created_purchase_receipt,persist_purchase_receipt}` | 接收调用方 `&mut dyn Executor`，仅履约表头/行写入 |
| 收货过账准备/状态写入 | `FulfillmentService::{prepare_purchase_receipt_posting,mark_purchase_receipt_posted}` | 原草稿/版本/仓库/行校验；逐行库存成功后才 mark/CAS |
| 入库生成仓发草稿 | `FulfillmentService::{draft_warehouse_delivery,create_receipt_stock_delivery,append_receipt_stock_delivery_lines}` | 单域草稿匹配、构造、去重追加与持久化；任务留 process |
| 收货/发货行规格转换 | `erp_fulfillment::service::{purchase_receipt_lines::receipt_line_specs,delivery_lines::{delivery_line_specs,receipt_reservation_specs}}` | DTO/fact 到实体规格；按原顺序注入行 ID |
| 采购资格与 PREPAY 适配 | `erp_processes::fulfillment_execution::purchase_context::{ensure_po_fulfillable,ensure_prepay_gate,load_po_current_revision,ensure_allocation_valid}` | 原参数语义，内部 session 泛化为 `&mut dyn Executor`；适配采购、财务与销售提供方 |
| 收货库存余额/流水 | `erp_inventory::service::fulfillment::post_receipt_stock(db, executor, ReceiptStockFact, occurred_at, actor_id)` | 返回余额 ID；原余额、movement、lastmovement 顺序 |
| 收货销售预占 | `erp_inventory::service::fulfillment::establish_receipt_reservation(db, executor, ReceiptReservationFact)` | 单条分配原 reservation、reserve、entry 顺序 |
| 仓发库存过账 | `erp_inventory::service::fulfillment::delivery::post_warehouse_ship_line(db, executor, &WarehouseShipmentLine, occurred_at, actor_id)` | 完整库存校验和逐行消耗、流水、余额写入 |

域构造器为 `FulfillmentService::new(Database)`，只持数据库。流程构造器保留 `FulfillmentProcess::new(db, key, Arc<SensitiveDataCodec>)` 及 `with_object_read`。DTO 定义统一由 `erp_fulfillment::dto` 根提供；四个 receipt/delivery `From` 映射由本域 service 原方法承载。

## 3. 消费事实与实际提供方

### 3.1 履约域消费事实

- `entity::facts::PurchaseOrderStatusFact`：`Effective`、`PartiallyExecuted`、`Other`。流程显式匹配采购状态；只有前两个通过原资格守卫。
- `PrepaymentRequirementFact`：`prepay_gate`、`prepay_minimum_amount`、`prepay_minimum_ratio`。原金额/比例类型不变。
- `PurchaseRevisionLineFact`：采购版本行 ID、可选数量。逐收货行校验时投影，不提前检查后续行。
- `PurchaseAllocationFact`：采购版本行 ID、销售版本行 ID。
- `ReceiptReservationLineFact`：预占 ID、销售稳定行 ID、预占数量；只在原仓发构造位置投影。
- `ReceiptFulfillmentProgress`：`Partial`/`Completed`；process 显式匹配为采购 `ProgressStatus::Partial/Completed`，不得误用销售同名进度。

### 3.2 库存域消费事实

`ReceiptStockFact` 只含仓库、SKU、合格数量、收货 ID、收货行 ID；时间和记录人按原流程参数传入。`ReceiptReservationFact` 只含仓库、SKU、销售行、采购销售分配 ID、收货行/单 ID、余额 ID和本分配数量。`WarehouseShipmentLine` 只含发货单/行 ID、销售行、可选预占/仓库和数量。库存不得引用履约、采购或销售完整实体。

### 3.3 提供方仓储保持合同

| 原读取 | 实际提供方 | 必须保持的语义 |
| --- | --- | --- |
| 采购来源应付账户 | `erp_finance::repository::PayableExt` 的 `payable_accounts().list_payable_accounts_for_purchase_order` | 原过滤与账户顺序；不替换为余额快捷值 |
| 应付分录/核销分配 | 原 `payable_entries().find_entries_by_accounts`、`payment_allocations().find_allocations_by_entries` | 同一执行器、相同 ID 列表与 Apply/Reverse 顺序 |
| 分配的销售版本行 | `erp_sales::repository::SalesOrderExt` 的 `sales_order_revision_lines().sales_revision_line_for_allocation` | 原单行条件与首错顺序 |
| 批量销售版本行/稳定行 | 对应销售拥有仓储的 `list_active_by_ids` | 原 ID 输入顺序、空列表处理与活动记录条件 |
| 本次收货预占 | `erp_inventory::InventoryExt` 的 `stock_reservations().list_stock_reservations_for_receipt_lines` | 原过滤和查询时点 |
| 收货累计 | 履约 `qualified_received_totals_by_purchase_revision_line` | 原聚合条件；不迁入 process 手工查询累计 |

PREPAY 执行顺序固定为：当前版本读取 → `prepay_gate` 关闭即返回 → 原账户查询 → 按账户取分录 → 保留 `source_document_id == po_id.to_string()` 的分录 → 按分录取核销分配 → 从金额零值依次 `Apply` 加、`Reverse` 减 → 原金额门槛先于比例门槛。不得改成账户 `settled_total`、额外筛选、排序或先聚合后扣减。

## 4. 收货创建与更新写序

创建固定为：请求校验 → 收货 ID → 表头构造 → 原请求行序生成行 ID → 批量工厂 → 创建审计对象 → 根事务 → 构造绑定命令/业务单据 → `NO_APPROVAL` 和无适配器守卫 → 原统一绑定端口 → 校验空绑定 → 登记业务单据 → 履约表头和行 → 履约执行任务 → 审计写入。

`NO_APPROVAL` 禁止审批任务、定义绑定及流程实例，原履约执行任务必须保留。六个原收货创建、质量和 NO_APPROVAL 测试均保持入口和断言；质量派生仍在实体批量工厂。

更新固定为：请求校验 → 无事务读取 → 版本冲突守卫 → 冻结仓库守卫 → 原实体更新 → 审计对象构造 → 根事务内收货 CAS → 履约活动记录 → 审计写入。冻结仓库拒绝文案仍为“采购入库单的目标仓库已冻结，不能在任务生成后变更”。

## 5. 收货过账原序

1. 事务前校验请求，按原输入建立 `receipt_id` 并拆出版本和仓库。
2. 根事务读取收货单；原 NotFound 优先。
3. 草稿/版本守卫 → 冻结仓库守卫 → 表头更新 → 读取收货行 → 原非空行守卫。
4. 读取采购单 → 采购资格 → PREPAY（读取当前版本）→ 再次读取当前版本 → 读取采购版本行 → 查询有效收货累计。
5. 在上述读取成功后的原位置取一次 `Instant::now()`。
6. 按收货行顺序执行：找采购版本行 → 取已收数量/零值 → 原 `ensure_within_revision` → 单行库存/预占 → 原累计数量 checked-add。不得预校验下一行。
7. 合格量不大于零时，单行库存阶段立即返回；不读取 SKU、分配或余额。
8. 合格行固定执行：校验对应采购行/SKU → 库存余额查找和增加/创建 → movement 构造及写入 → lastmovement → 采购销售分配查询 → 原比例及尾差分摊 → 销售版本行读取 → 按分配逐条校验销售归属 → reservation 构造/写入 → reserve_quantity → Establish entry 构造/写入。
9. 全部行成功后：`mark_posted`/收货 CAS → 完成收货履约任务 → 计算并写回采购履约进度 → 创建/追加仓发草稿及履约任务 → 构造并写入原过账审计。

每个数据库调用和任务端口接收根事务的同一个 `Executor`。域服务和库存服务不另开事务，失败保持原错误类型及消息并立即传播。

仓发草稿仍按 `(sales_order_id, warehouse_id)` 的 `BTreeMap` 遍历；销售行 ID 去重仍使用原 `HashSet`，不得附加排序。已存在草稿时读取行、按预占 ID 去重、从最大行号 + 1 追加；新草稿先生成 Delivery ID，再取编号，再构造表头和行。任务在本域草稿持久化之后执行。

## 6. 生产 Port 与内联测试证据

| 生产入口 | 真正调用的提供方 | 新测试数量及范围 |
| --- | --- | --- |
| `purchase_receipt_posting::execute_posting` | `MongoReceiptPosting` 调用单行库存、履约写入、任务、采购进度、仓发草稿和审计 | 2；三条收货行及后续步骤原序、逐步失败、同一非零 Executor、原错误 |
| 库存 `execute_receipt_stock` / `execute_receipt_reservation` | `ReceiptStockPosting` / `ReceiptReservationPosting` → `ReceiptInventoryStore` → `MongoReceiptStore` → 原 InventoryExt 仓储 | 4；balance/movement/lastmovement、reservation/reserve/entry 顺序及失败前缀 |
| 同一库存生产构造器和 `ReceiptInventoryStore` | Mongo adapter 与 RecordingStore 共享真实构造/条件写算法 | 3；新建/已有余额、真实 movement/reservation/entry 事实字段、每个仓储失败、三个 CAS false 原文案 |
| 发货 process `execute_posting` | `DeliveryPosting` | 3；仓发逐行、直发采购门槛、三行中间失败不访问后续行 |
| 发货库存 `post_with_store` | `ShipmentStore` → `MongoShipmentStore` | 6；八仓储步、四 CAS false、原首错、available=0/reserved=2 的先释放再扣减及冻结事实 |

所有 Executor 身份断言使用非零大小 `TestExecutor { _identity: u8 }`。源字符串断言仍保存原架构合同，但不得作为生产事务顺序的唯一证据。真实 MongoDB 回滚、并发唯一索引竞争、网络失败与提交结果恢复未在本分片运行验证。

## 7. 内容指纹登记

下表固定本合同写入时的源码；后续编译修复必须先核对差异并刷新，不以指纹代替测试。

| 最终文件 | SHA-256 |
| --- | --- |
| `backend/crates/erp-fulfillment/src/service/delivery.rs` | `00aeb1f5413f02c4a7bc46dd2ae512d0d18220afcd39de757cb1292a6324901b` |
| `backend/crates/erp-fulfillment/src/service/delivery_lines.rs` | `b664384fdc14b98efa8580074c9752482e6b278f34aa459457e8a997cc3ce3ff` |
| `backend/crates/erp-fulfillment/src/service/delivery_posting.rs` | `f201c74f808bc7bce779d54b199ce3d898fa978eef47bfab3170a0e1db6f7770` |
| `backend/crates/erp-fulfillment/src/service/purchase_receipt.rs` | `9c318c3d67c28637d980dec73f36511f9ffceac7ee1cc79f02b7b77e57ea7bc1` |
| `backend/crates/erp-fulfillment/src/service/purchase_receipt_lines.rs` | `9414b022e194134ac526a564f2b7c6be035e9276eb9f5d41d4005a885d19e3d2` |
| `backend/crates/erp-fulfillment/src/service/purchase_receipt_posting.rs` | `880f029f16a21f30aeb6b3733df86dc9d7e30323a262dc234270ff9e4f6ee336` |
| `backend/crates/erp-inventory/src/service/fulfillment/delivery.rs` | `e77bebc6f3f7764a9896fd014d9f75378eb7d5b915d32d78dd220a7dc4d7e118` |
| `backend/crates/erp-inventory/src/service/fulfillment.rs` | `ea4356f3e6d6c40f0ac6cc6127cc6c4b98608dda9f7d624bbc784d82599d0f50` |
| `backend/crates/erp-processes/src/fulfillment_execution/delivery.rs` | `4e9fb80f25df2f902b9c6f4b43e38b241417e4b9fe234eef97f4e9a61ad4083c` |
| `backend/crates/erp-processes/src/fulfillment_execution/delivery_posting.rs` | `a70303db50397ea847affcff834c0d604ab5d09e1b753893b39a3c3e6f42ff0a` |
| `backend/crates/erp-processes/src/fulfillment_execution/purchase_context.rs` | `05f17f3694b50b6965410d14267907220fe7426e89548e360bc1e39481b5f54c` |
| `backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt.rs` | `76ba4da93f4ff90839f643d5cb1f33e66e359ddf3eeb99e0d05990ff8099f152` |
| `backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs` | `ac934fd8ef8ced06db019f671128b96a58e114ee892bbd041daf167858dfca58` |
