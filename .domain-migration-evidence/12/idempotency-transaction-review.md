# 阶段 12 幂等与事务符号核销合同

## 1. 输入、判定与使用边界

固定 before 为 `cf89fb4774fd3b81d477e9088be1d6c2f699f46e`，参考树为 `/private/tmp/erp-domain-crate-11-procurement`；after 固定为 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`；`/private/tmp/erp-domain-crate-12-fulfillment` 与该提交的全部受审源文件已逐字节核对一致。最终 raw 扫描目录为 `/private/tmp/erp-fulfillment12-contract-sealed`；初次 raw `/private/tmp/erp-fulfillment12-contract-initial` 也保留独立哈希。两次 changed symbol 集合均为 18 + 16，逐项对应一致。

判定：本快照的 **18 项 idempotency + 16 项 transaction，共 34 项 raw symbol 全部完成静态核销，未识别出语义漂移**。每项均记录真实 qualified symbol；其中 9 项 raw after 未命中仍有实际生产符号，按拆分/归属变化核销，不计业务删除。

本报告不更改 raw 的 `needs_review` 状态、不修改 allowlist。原始扫描哈希与本报告独立核销结果分别保留；详细机器记录见 [JSON](/private/tmp/fulfillment12-idempotency-transaction-review.json)。执行集成门禁时须同时保留 raw 与该记录。

本次核对展开 wrapper、领域事务内方法、真实 Mongo adapter、原 owned 仓储和错误转换；未运行 Cargo、MongoDB 或对象存储。真实数据库回滚、并发、重试与补偿运行结果未验证。授权的唯一源码修正为 `service_evidence.rs` 测试中单元素循环改直接调用，保留原断言；不包含生产逻辑变更。

## 2. 必须保持的真实生产顺序

### R1 采购收货根事务与库存真实adapter

1. 请求validate在事务前；事务内receipt find→draft/version→冻结仓校验→receipt.update（内存）→行读取/非空归属→采购读取/状态→PREPAY→当前版本再次读取/版本行→累计合格收货→Instant::now。
2. 每行先当前采购行查找→已收数量→ensure_within_revision；合格数<=0跳过库存，成功后才更新内存received；没有提前批量校验后续行。
3. 真实MongoReceiptStore：find_by_dimensions→increase_on_hand或next_id/new balance/create；之后movement ID→fact_no ID→movement create→apply_last_movement。
4. 采购allocations读取→空分配短路→版本quantity→reservation_shares（最后份额吸收6位尾差）→销售版本行批量读取→每个销售归属查找→reservation ID/Active/零consumed/released→reservation create→reserve_quantity CAS→entry ID/Establish→entry create。
5. receipt.mark_posted/update→complete task→采购progress计算/set/update→自动仓发草稿→audit构造/create；所有步骤传同session，失败?中止后续。
6. 已复看B完成的ReceiptReservationPosting::entry→self.store.entry→MongoReceiptStore::entry→原stock_reservation_entries.create，编辑中间态self.db引用已修正。

实际生产证据：

- `erp_processes::fulfillment_execution::purchase_receipt_posting::execute_posting` — [backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:139](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:139)。
- `erp_processes::fulfillment_execution::purchase_receipt_posting::post_receipt_line` — [backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:280](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:280)。
- `erp_inventory::service::fulfillment::execute_receipt_stock` — [backend/crates/erp-inventory/src/service/fulfillment.rs:116](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:116)。
- `erp_inventory::service::fulfillment::execute_receipt_reservation` — [backend/crates/erp-inventory/src/service/fulfillment.rs:266](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:266)。
- `erp_inventory::service::fulfillment::ensure_or_create_balance` — [backend/crates/erp-inventory/src/service/fulfillment.rs:214](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:214)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::balance` — [backend/crates/erp-inventory/src/service/fulfillment.rs:44](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:44)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::increase` — [backend/crates/erp-inventory/src/service/fulfillment.rs:57](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:57)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::create_balance` — [backend/crates/erp-inventory/src/service/fulfillment.rs:60](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:60)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::movement` — [backend/crates/erp-inventory/src/service/fulfillment.rs:64](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:64)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::last_movement` — [backend/crates/erp-inventory/src/service/fulfillment.rs:68](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:68)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::reservation` — [backend/crates/erp-inventory/src/service/fulfillment.rs:80](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:80)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::reserve` — [backend/crates/erp-inventory/src/service/fulfillment.rs:84](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:84)。
- `erp_inventory::service::fulfillment::MongoReceiptStore::entry` — [backend/crates/erp-inventory/src/service/fulfillment.rs:87](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:87)。
- `erp_inventory::service::fulfillment::ReceiptStockPosting::balance` — [backend/crates/erp-inventory/src/service/fulfillment.rs:133](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:133)。
- `erp_inventory::service::fulfillment::ReceiptStockPosting::movement` — [backend/crates/erp-inventory/src/service/fulfillment.rs:143](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:143)。
- `erp_inventory::service::fulfillment::ReceiptStockPosting::last_movement` — [backend/crates/erp-inventory/src/service/fulfillment.rs:168](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:168)。
- `erp_inventory::service::fulfillment::ReceiptReservationPosting::reservation` — [backend/crates/erp-inventory/src/service/fulfillment.rs:280](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:280)。
- `erp_inventory::service::fulfillment::ReceiptReservationPosting::reserve` — [backend/crates/erp-inventory/src/service/fulfillment.rs:300](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:300)。
- `erp_inventory::service::fulfillment::ReceiptReservationPosting::entry` — [backend/crates/erp-inventory/src/service/fulfillment.rs:312](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment.rs:312)。

### R2 发货根事务与库存真实adapter

1. 事务前validate；事务内delivery find→Draft→version→carrier/tracking update→lines非空→Instant::now。
2. WarehouseShip逐行：reservation_id存在→reservation查询→销售归属→预占量→发货仓存在/一致→balance查询→consume_quantity CAS→entry ID/Consume/create→release_reserved→deduct_available→movement ID/fact_no/create→apply_last_movement。
3. MongoShipmentStore每个方法转发原inventory owned仓储；既有repository quantity/state CAS文件未改变。
4. SupplierDirect仅来源采购ID→采购load→状态→PREPAY，没有库存写入；随后mark_shipped/update→complete task→ensure acceptance task→audit。

实际生产证据：

- `erp_inventory::service::fulfillment::delivery::post_warehouse_ship_line` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:51](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:51)。
- `erp_inventory::service::fulfillment::delivery::post_with_store` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:209](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:209)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::reservation` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:120](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:120)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::balance` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:138](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:138)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::consume` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:151](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:151)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::reservation_entry` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:158](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:158)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::release_reserved` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:166](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:166)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::deduct_available` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:178](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:178)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::movement` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:190](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:190)。
- `erp_inventory::service::fulfillment::delivery::MongoShipmentStore::last_movement` — [backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:194](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-inventory/src/service/fulfillment/delivery.rs:194)。

### R3 自动仓发草稿幂等复用与编号

1. 收货行空短路；按receipt line IDs读取未删除预占，无状态新增过滤；预占空短路；HashSet收集销售稳定行ID（保留原集合语义）→批量读取→缺归属原错；BTreeMap按sales_order_id+warehouse_id顺序处理。
2. 已有draft：delivery lines读取→existing reservation IDs→过滤pending→max line_no+1→仅pending逐项next_id→DeliveryLineBatch→逐行create→ensure task。
3. 新draft：先delivery ID再next_delivery_no再构造header，再逐行ID/Batch/header+lines写入，最后ensure task。
4. DocumentNumberGenerator仍以NoTransaction自增，Shanghai固定+08:00日期求值位置不变，号码消耗不随业务事务回收。

实际生产证据：

- `erp_processes::fulfillment_execution::purchase_receipt_posting::ensure_receipt_stock_delivery` — [backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:480](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt_posting.rs:480)。
- `erp_fulfillment::service::DocumentNumberGenerator::next_delivery_no` — [backend/crates/erp-fulfillment/src/service/document_number.rs:26](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/document_number.rs:26)。

### R4 发货创建与更新

1. create_delivery wrapper仍存在；prepare_delivery依次validate→header next_id→Delivery::new→delivery_line_specs逐行next_id→Batch；audit在事务前构造。
2. 组合persist_created_delivery只持一个根事务：NO_APPROVAL binding decision/adapter guard/registration→domain header+lines→ensure task→audit。
3. update_delivery读取/版本检查/实体update/audit构造在NoTransaction和根事务之前；事务内owned update→activity→audit。

实际生产证据：

- `erp_processes::fulfillment_execution::delivery::register_created_delivery_document` — [backend/crates/erp-processes/src/fulfillment_execution/delivery.rs:222](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/delivery.rs:222)。

### R5 采购收货创建与更新

1. create准备依次validate→header ID→PurchaseReceipt::new→line specs逐行ID/Batch；audit仍在persist根事务前。
2. 根事务registration→header/lines→ensure task→audit；registration仍NoApproval且没有审批任务/流程绑定新增。
3. update准备依次validate→NoTransaction find→version→冻结仓→实体update；audit构造在事务前，事务内update→activity→audit。

实际生产证据：

- `erp_processes::fulfillment_execution::purchase_receipt::register_created_purchase_receipt_document` — [backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt.rs:278](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/purchase_receipt.rs:278)。

### R6 客户验收reverse回执、写序和完成投影

1. reverse request validate→CommandReceipt::from_resource_parts固定prefix/action/resource/id/key/[expected_version,reason]→事务前committed_resource_id回放；事务失败后再次同query，成功回放detail，不扩大普通post幂等行为。
2. persist_customer_acceptance_reverse在前后函数体逐字相同：original load/reversible→原行→原分配→ensure_reversible_source→reverse header next_id+REV-no+Instant::now→镜像行next_id→按原分配顺序reverse allocation next_id+原allocation引用→header/lines create→逐分配create→reverse header mark_posted/update→original reverse/update。
3. write_acceptance_allocation和load_fulfillment_fact前后函数体逐字相同；履约头行仓储写序和serde不变。
4. completion真实DatabaseCompletion先同Executor读取销售current revision/lines/goods、三类履约、三类分配，再唯一资格builder/derive；builder只字段路径改为窄facts，保重复stable line后行覆盖required量与首次顺序。
5. None不写销售进度也不刷新money；有投影则显式三枚举等价映射→原sales progress writer；reverse仅remaining=true时ensure_customer_acceptance_task，随后原resource业务audit→command receipt audit。

实际生产证据：

- `erp_fulfillment::service::customer_acceptance_posting::write_acceptance_allocation` — [backend/crates/erp-fulfillment/src/service/customer_acceptance_posting.rs:391](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/customer_acceptance_posting.rs:391)。
- `erp_fulfillment::service::customer_acceptance_posting::load_fulfillment_fact` — [backend/crates/erp-fulfillment/src/service/customer_acceptance_posting.rs:455](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/customer_acceptance_posting.rs:455)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::finish` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:73](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:73)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::apply_projection` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:173](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:173)。
- `erp_fulfillment::service::acceptance_eligibility::build_line_eligibilities` — [backend/crates/erp-fulfillment/src/service/acceptance_eligibility.rs:49](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/acceptance_eligibility.rs:49)。
- `erp_read_models::fulfillment_center::repository::load_customer_acceptance_progress` — [backend/crates/erp-read-models/src/fulfillment_center/repository.rs:24](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-read-models/src/fulfillment_center/repository.rs:24)。
- `erp_processes::fulfillment_execution::customer_acceptance::task::ensure_customer_acceptance_task` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/task.rs:44](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/task.rs:44)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::DatabaseCompletion::refresh_sales` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:98](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:98)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::DatabaseCompletion::persist_task` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:117](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:117)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::DatabaseCompletion::business_audit` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:142](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:142)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::DatabaseCompletion::command_receipt` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:157](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:157)。
- `erp_processes::fulfillment_execution::customer_acceptance::completion::SalesProgressWriter::write` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:202](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/completion.rs:202)。

### R7 任务读取、冻结身份、授权与写入

1. 独立对比task及acceptance task原生产函数；除路径/可见性和record/complete抽Port外函数体同构。
2. record/complete展开command::execute→MongoTaskCommand：load_single_open_task→ensure frozen identity→Instant::now+WorkItem变更→ensure_current_owner_execution_access→owned update。没有将授权移到WorkItem业务错误之前。
3. 验收ensure_customer_acceptance_task与reopen来源reason枚举/ID/clock/owner/RBAC/open查询顺序保持。
4. 实施者额外证据仅作补充引用：/private/tmp/fulfillment12-task-evidence.json；本结论已独立展开真实adapter。

实际生产证据：

- `erp_processes::fulfillment_execution::task::command::execute` — [backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:50](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:50)。
- `erp_processes::fulfillment_execution::task::record_fulfillment_activity` — [backend/crates/erp-processes/src/fulfillment_execution/task.rs:232](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task.rs:232)。
- `erp_processes::fulfillment_execution::task::complete_fulfillment_task` — [backend/crates/erp-processes/src/fulfillment_execution/task.rs:254](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task.rs:254)。
- `erp_processes::fulfillment_execution::customer_acceptance::task::ensure_customer_acceptance_task` — [backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/task.rs:44](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/customer_acceptance/task.rs:44)。
- `erp_processes::fulfillment_execution::task::command::MongoTaskCommand::load_open` — [backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:34](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:34)。
- `erp_processes::fulfillment_execution::task::command::MongoTaskCommand::authorize` — [backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:42](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:42)。
- `erp_processes::fulfillment_execution::task::command::MongoTaskCommand::update` — [backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:45](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/task/command.rs:45)。

### R8 组合状态与本域服务归属

1. 原FulfillmentService五个字段db/fingerprint_key/sensitive_data/rbac/object_read完整转给FulfillmentProcess；new仍同shared_rbac_service、同FailClosed默认。
2. 新本域FulfillmentService仅db，domain()只clone db，无查询、ID、clock或新增根事务。

### D 电子交付/服务确认独立审核

1. 独立报告核销旧五叶35个生产函数：20个主目标函数体token相同，15个变化逐项展开真实Provider；六个支持实体文件64个生产函数体token相同；13个旧内联测试入口保留。
2. 电子确认保持本域读取/状态首错→来源采购/付款门槛→分配→confirm/update→complete task→无条件ensure acceptance task→audit，一个根事务。
3. 服务确认在事务外保留validate→证据resolve/all-used→规范化地点→原codec加密→同明文HMAC→时间转换/确认实体；关联错误类型保留services Error分类。
4. 事务内MongoServiceConfirmation依次load→purchase→allocation→evidence→pending persist（file assets后audit logs）→confirm/update→task→仅成功且eligible时acceptance→audit，全部同Executor。
5. 真实finance/sales事实Provider保持原条件；附件敏感性和保留枚举显式映射；OutcomeUnknown传播与HTTP保留已上传对象分类不变。
6. 独立报告以0a297ab0为输入；其26个before文件已逐字节核对等于本报告要求的cf89fb47，36个after文件在本报告合并时hash仍一致。

独立证据：[D 语义报告](/private/tmp/fulfillment12-electronic-service-semantic-review.md)、[D source map](/private/tmp/fulfillment12-electronic-service-source-map.json)。D 报告对任务入口/参数/时点的结论与本报告 R7 对任务内部的独立结论合并使用。

## 3. Raw 逐符号处置

表中目标均为当前生产定义；同 terminal name 的 process free function 与领域关联方法分别列出。`extracted` 表示原业务段抽取，不能据此计新增行为。

### idempotency（18 项）

| Raw symbol | 处置 / 证据 | 当前真实 qualified symbols | 必须采用的解释 |
| --- | --- | --- | --- |
| `fn::confirm_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::confirm_electronic_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_electronic_confirmation`<br>`erp_fulfillment::service::FulfillmentService::persist_electronic_confirmation` | original load/state guard and state mutation/write delegated to domain at unchanged process positions; procurement gates/task/acceptance/audit retained；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::confirm_service_fulfillment_with_assets` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::confirm_service_fulfillment_with_assets` | same request validation, pending resolve/all-used, confirmation factory before root; inject original codec adapter；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::create_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::FulfillmentProcess::create_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_delivery`<br>`erp_processes::fulfillment_execution::delivery::persist_created_delivery` | 原外层仍存在；请求校验→表头ID→Delivery构造→逐行ID→行批工厂前移到本域prepare，仍在根事务之前。扫描器未选wrapper不表示删除。 |
| `fn::create_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::create_electronic_delivery` | factory actor.id projection only; same validation/construction/persist order；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::create_receipt_stock_delivery` | `extracted_preserved` / R3 | `erp_fulfillment::service::FulfillmentService::create_receipt_stock_delivery` | 原ensure_receipt_stock_delivery的新建分支抽出；保持header ID→NoTransaction业务单号→header构造→逐行ID→header/line写入，随后wrapper创建任务。 |
| `fn::create_service_fulfillment` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::create_service_fulfillment` | factory actor.id projection only; same validation/construction/persist order；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::electronic_delivery_draft_from_request` | `relocated_or_extracted_preserved` / D | `erp_fulfillment::service::electronic_delivery_crypto::electronic_delivery_draft_from_request` | AuditActor projected to actor.id() by process; factory accepts actor_id; evaluation/ID/clock/fingerprint order retained；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::ensure_receipt_stock_delivery` | `split_preserved` / R3 | `erp_processes::fulfillment_execution::purchase_receipt_posting::ensure_receipt_stock_delivery`<br>`erp_fulfillment::service::FulfillmentService::create_receipt_stock_delivery`<br>`erp_fulfillment::service::FulfillmentService::append_receipt_stock_delivery_lines` | 原wrapper仍存在；现有同仓草稿补行与新建分支拆入本域，task仍在组合层，扫描器漏选不得判删除。 |
| `fn::persist_created_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::delivery::persist_created_delivery`<br>`erp_fulfillment::service::FulfillmentService::persist_created_delivery` | 组合free function保留根事务；领域同名关联方法仅写表头/行。必须按qualified owner区分，不按terminal name认定重复事务。 |
| `fn::persist_created_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::electronic_delivery::persist_created_electronic_delivery`<br>`erp_fulfillment::service::FulfillmentService::persist_created_electronic_delivery` | original single repository create delegated to domain service, at same position after no-approval registration and before task/audit；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::persist_created_service_fulfillment` | `split_preserved` / D | `erp_processes::fulfillment_execution::service_fulfillment::persist_created_service_fulfillment`<br>`erp_fulfillment::service::FulfillmentService::persist_created_service_fulfillment` | original single repository create delegated to domain service, at same position after no-approval registration and before task/audit；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::post_purchase_receipt` | `split_preserved` / R1 | `erp_processes::fulfillment_execution::FulfillmentProcess::post_purchase_receipt`<br>`erp_fulfillment::service::FulfillmentService::prepare_purchase_receipt_posting`<br>`erp_processes::fulfillment_execution::purchase_receipt_posting::post_receipt_line`<br>`erp_inventory::service::fulfillment::post_receipt_stock`<br>`erp_inventory::service::fulfillment::establish_receipt_reservation` | 原外层仍在；准备、本域状态、库存余额/流水/预占下沉到各领域的事务内接口，按原逐行顺序调用同一个Executor。 |
| `fn::prepare_delivery` | `extracted_preserved` / R4 | `erp_fulfillment::service::FulfillmentService::prepare_delivery` | 新增方法是原create_delivery的纯准备段；没有新增业务动作或幂等规则。 |
| `fn::service_confirmation_from_request` | `relocated_or_extracted_preserved` / D | `erp_fulfillment::service::service_fulfillment_confirm::service_confirmation_from_request`<br>`erp_processes::fulfillment_execution::service_crypto::ServiceCryptoAdapter::encrypt` | same normalized location and original codec via ServiceLocationCryptoPort; associated provider error preserves original error category；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::service_fulfillment_draft_from_request` | `relocated_or_extracted_preserved` / D | `erp_fulfillment::service::service_fulfillment_crypto::service_fulfillment_draft_from_request` | AuditActor projected to actor.id() by process; factory accepts actor_id; evaluation/ID/clock/fingerprint order retained；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::update_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::FulfillmentProcess::update_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_delivery_update`<br>`erp_fulfillment::service::FulfillmentService::persist_delivery` | 外层仍在：NoTransaction读取/版本/字段校验及audit构造均在根事务外；事务内update→activity→audit。 |
| `struct::FulfillmentProcess` | `ownership_split_preserved` / R8 | `erp_processes::fulfillment_execution::FulfillmentProcess` | 新增process承接原服务db/key/codec/rbac/object_read；FailClosed默认与真实入口配置保持。不是新增幂等过程。 |
| `struct::FulfillmentService` | `ownership_split_preserved` / R8 | `erp_fulfillment::service::FulfillmentService`<br>`erp_processes::fulfillment_execution::FulfillmentProcess` | 旧services类型已迁出；本域FulfillmentService仍存在，仅持db；跨域状态移给FulfillmentProcess。scanner未选本域类型不表示服务删除。 |

### transaction（16 项）

| Raw symbol | 处置 / 证据 | 当前真实 qualified symbols | 必须采用的解释 |
| --- | --- | --- | --- |
| `fn::confirm_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::confirm_electronic_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_electronic_confirmation`<br>`erp_fulfillment::service::FulfillmentService::persist_electronic_confirmation` | original load/state guard and state mutation/write delegated to domain at unchanged process positions; procurement gates/task/acceptance/audit retained；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::confirm_service_fulfillment_with_assets` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::confirm_service_fulfillment_with_assets` | same request validation, pending resolve/all-used, confirmation factory before root; inject original codec adapter；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::create_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::FulfillmentProcess::create_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_delivery`<br>`erp_processes::fulfillment_execution::delivery::persist_created_delivery` | 原外层仍存在；请求校验→表头ID→Delivery构造→逐行ID→行批工厂前移到本域prepare，仍在根事务之前。扫描器未选wrapper不表示删除。 |
| `fn::create_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::create_electronic_delivery` | factory actor.id projection only; same validation/construction/persist order；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::create_service_fulfillment` | `split_preserved` / D | `erp_processes::fulfillment_execution::FulfillmentProcess::create_service_fulfillment` | factory actor.id projection only; same validation/construction/persist order；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::ensure_receipt_stock_delivery` | `split_preserved` / R3 | `erp_processes::fulfillment_execution::purchase_receipt_posting::ensure_receipt_stock_delivery`<br>`erp_fulfillment::service::FulfillmentService::create_receipt_stock_delivery`<br>`erp_fulfillment::service::FulfillmentService::append_receipt_stock_delivery_lines` | 原wrapper仍存在；现有同仓草稿补行与新建分支拆入本域，task仍在组合层，扫描器漏选不得判删除。 |
| `fn::persist_created_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::delivery::persist_created_delivery`<br>`erp_fulfillment::service::FulfillmentService::persist_created_delivery` | 组合free function保留根事务；领域同名关联方法仅写表头/行。必须按qualified owner区分，不按terminal name认定重复事务。 |
| `fn::persist_created_electronic_delivery` | `split_preserved` / D | `erp_processes::fulfillment_execution::electronic_delivery::persist_created_electronic_delivery`<br>`erp_fulfillment::service::FulfillmentService::persist_created_electronic_delivery` | original single repository create delegated to domain service, at same position after no-approval registration and before task/audit；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::persist_created_purchase_receipt` | `split_preserved` / R5 | `erp_processes::fulfillment_execution::purchase_receipt::persist_created_purchase_receipt`<br>`erp_fulfillment::service::FulfillmentService::persist_created_purchase_receipt` | 根事务仍只由process free function持有，本域同名方法只写头行；NO_APPROVAL注册→头行→执行任务→审计保持。 |
| `fn::persist_created_service_fulfillment` | `split_preserved` / D | `erp_processes::fulfillment_execution::service_fulfillment::persist_created_service_fulfillment`<br>`erp_fulfillment::service::FulfillmentService::persist_created_service_fulfillment` | original single repository create delegated to domain service, at same position after no-approval registration and before task/audit；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::post_delivery` | `split_preserved` / R2 | `erp_processes::fulfillment_execution::FulfillmentProcess::post_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_delivery_posting`<br>`erp_inventory::service::fulfillment::delivery::post_with_store`<br>`erp_fulfillment::service::FulfillmentService::persist_posted_delivery` | 真实MongoShipmentStore逐项访问原owned仓储；WarehouseShip原库存写序和SupplierDirect采购/PREPAY分支保留。 |
| `fn::post_purchase_receipt` | `split_preserved` / R1 | `erp_processes::fulfillment_execution::FulfillmentProcess::post_purchase_receipt`<br>`erp_fulfillment::service::FulfillmentService::prepare_purchase_receipt_posting`<br>`erp_processes::fulfillment_execution::purchase_receipt_posting::post_receipt_line`<br>`erp_inventory::service::fulfillment::post_receipt_stock`<br>`erp_inventory::service::fulfillment::establish_receipt_reservation` | 原外层仍在；准备、本域状态、库存余额/流水/预占下沉到各领域的事务内接口，按原逐行顺序调用同一个Executor。 |
| `fn::reverse_customer_acceptance` | `relocated_dependencies_preserved` / R6 | `erp_processes::fulfillment_execution::customer_acceptance::CustomerAcceptanceProcess::reverse_customer_acceptance`<br>`erp_fulfillment::service::FulfillmentService::persist_customer_acceptance_reverse`<br>`erp_processes::fulfillment_execution::customer_acceptance::completion::complete_acceptance`<br>`erp_read_models::fulfillment_center::repository::load_customer_acceptance_progress` | 命令回执前后查询、反向事实写序、完成副作用与条件reopen不变；查询服务域归属改变，未删除wrapper或增加回执。 |
| `fn::service_confirmation_from_request` | `relocated_or_extracted_preserved` / D | `erp_fulfillment::service::service_fulfillment_confirm::service_confirmation_from_request`<br>`erp_processes::fulfillment_execution::service_crypto::ServiceCryptoAdapter::encrypt` | same normalized location and original codec via ServiceLocationCryptoPort; associated provider error preserves original error category；静态核销依据 D 报告对应生产链，任务内部额外使用本报告 R7 独立核对。 |
| `fn::update_delivery` | `split_preserved` / R4 | `erp_processes::fulfillment_execution::FulfillmentProcess::update_delivery`<br>`erp_fulfillment::service::FulfillmentService::prepare_delivery_update`<br>`erp_fulfillment::service::FulfillmentService::persist_delivery` | 外层仍在：NoTransaction读取/版本/字段校验及audit构造均在根事务外；事务内update→activity→audit。 |
| `fn::update_purchase_receipt` | `split_preserved` / R5 | `erp_processes::fulfillment_execution::FulfillmentProcess::update_purchase_receipt`<br>`erp_fulfillment::service::FulfillmentService::prepare_purchase_receipt_update`<br>`erp_fulfillment::service::FulfillmentService::persist_purchase_receipt` | 原版本/目标仓冻结/实体update/audit构造保留事务外位置；事务内update→任务activity→audit保持。 |

## 4. 证据固定与再次核对条件

- 当前 JSON 固定 15 个本报告 before 源文件和 43 个 after 源文件；每项 raw 前后选中路径、行号、token hash 原样留存，真实符号附固定提交源文件 hash。JSON 同时保留初次 raw 和最终 sealed raw 的逐项命中记录。
- `persist_customer_acceptance_reverse`、`write_acceptance_allocation`、`load_fulfillment_fact` 三个函数体与固定 before 逐字节相同；任务与编号 35 个函数独立比较，33 个除明确 import 路径/注释/空白外相同，另外 2 个由 R7 展开真实 MongoTaskCommand。
- 9 个既有 inventory 仓储与 persistence-core 事务/Executor 文件逐字节等于 before；错误/CAS 语义未用新实现替代。
- D 的 35 个生产符号记录和 64 个支持规则比较按外部证据逐项引用；其 26 个 before 文件已核对等于本报告固定提交。D 实施者也是任务实现者，因此任务内部结论采用本报告的独立源比较与实际 adapter 展开。
- 任一记录的生产文件变化后，必须重新展开受影响调用链并更新 source hash，才能继续引用核销结论。只有测试修正或格式变化也须刷新快照，并确认生产 token 未改变。
- ReceiptReservationPosting::entry 在并行编辑中出现过旧引用，实施者已修为 `self.store.entry`；最终复核固定真实 Store 转发，未将编辑中间态作为最终行为。

采集完成 UTC：`2026-09-07T04:06:36.593000+00:00`。三个 raw JSON 的 SHA-256 已在最终采集时复验未变化。

固定输出提交：`de112614e03714bb6d1ca4ac9e25f580c5c273c8`。合并核对 UTC：`2026-09-07T04:07:49.370985+00:00`。本报告 43 个 after 文件和 D 的 36 个 after 文件均与该提交同 SHA-256；集成测试结果由 root 门禁证据另行记录。
