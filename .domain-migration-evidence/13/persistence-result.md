# 阶段 13 实体、仓储与索引交付合同

输入固定为 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`；唯一工作树 `/private/tmp/erp-domain-crate-13-returns`。本分片已迁出并删除 22 个旧源文件（10 entity + 8 owned + Ext/returns/posted_totals/indexes 4），不修改共享 lib/Cargo/旧根注册。

## 1. 公共导出与接线

- `erp_returns::entity::returns::{original public entities, data, updates, status enums and cumulative rule}`。
- `erp_returns::repository::ReturnsExt`。
- `erp_returns::repository::owned::{CustomerRefundRepository,PaymentReversalRepository,PurchaseReturnLineRepository,PurchaseReturnOrderRepository,ReceiptReversalRepository,SalesReturnCaseRepository,SalesReturnLineRepository,SupplierRefundRepository}`。
- `erp_returns::repository::returns::{ReturnsRepository,SalesReturnCaseFilter,SalesReturnCaseRow,PurchaseReturnOrderFilter,PurchaseReturnOrderRow,CustomerRefundFilter,CustomerRefundRow}`。
- `erp_returns::indexes::ensure(db)`。

所有已有专用方法和调用方 Executor 签名保持。四种 posted total 仍是对应 owned 类型的关联方法，只有实现文件归属迁移；ReturnsRepository 是原跨集合聚合仓储，不计为第 9 个 owned。

## 2. 不可变数据与事务合同

- funds_status_wire：['draft', 'IN_APPROVAL', 'posted', 'reversed']。
- posted_total_original_fields：['original_receipt_id', 'original_payment_id', 'original_customer_receipt_id', 'original_supplier_payment_id']。
- posted_total：['same caller executor session/none branch', '$match original equality + status=posted + id != exclude + deleted_at=0', '$group sum $amount as Decimal128', '$project _id=0', 'empty rows Amount::zero']。
- refund_original_lookup：Two nonempty ID lists remain AND; both empty remains find_many(empty filter), base repository appends soft-delete filter. Each production query now calls its own extracted unchanged filter construction, covered by the two new pure tests.。
- multi_collection_write：ReturnsRepository creates header with mongo_ops::insert_one, then first line with the same Executor. Both method bodies match input tokens. No root transaction or additional writes added.。
- existing_query_tests：Two ignored database query tests owned by procurement_supplier_detail: cfg registration removed from domain repository, test source migration/removal/RM registration handled by that shard. No domain test-support dependency added.。

8 个集合按原顺序创建索引，索引分布为 `3/2/2/2/3/3/2/2`，共 19 项；名称/键顺序/唯一性定义全部由原函数迁入。新增索引测试核对全部实际 IndexModel，不连接数据库。

## 3. 原测试与迁移证明

- 原 66 个内联测试全部保留：实体原 56（4 个完整 BSON 测试移至 repository::serialization_contract，原测试 fixture 通过 cfg(test) pub(crate) 复用）、仓储原 8、索引原 2。
- 新增 6 个纯测试：两个来源组合过滤的四种输入、Decimal128 累计结果反序列化、四种资金事实 BSON/JSON 往返、四种状态全部 wire 值、19 个索引全表合同。
- 当前本片测试入口为 entity 52、repository 17、indexes 3，合计 72。只完成源码保留核对，未由本分片执行测试。
- 原 257 个生产函数全部映射，255 个函数体经实体 import 路径与空白/注释归一化后相同；两个 find_refunds_by_originals 的查询构造抽成实际生产纯 helper，展开后与原 body 相同。10 个实体完整 production token（含所有 serde/字段/枚举）相同，8 个 owned 全量 token 除实体路径外相同。

## 4. 文件映射

| 原文件 | 目标文件 | 原测试数 |
| --- | --- | ---: |
| `backend/entities/src/returns/cumulative_limit.rs` | `backend/crates/erp-returns/src/entity/returns/cumulative_limit.rs` | 6 |
| `backend/entities/src/returns/customer_refund.rs` | `backend/crates/erp-returns/src/entity/returns/customer_refund.rs` | 8 |
| `backend/entities/src/returns/payment_reversal.rs` | `backend/crates/erp-returns/src/entity/returns/payment_reversal.rs` | 8 |
| `backend/entities/src/returns/purchase_return_line.rs` | `backend/crates/erp-returns/src/entity/returns/purchase_return_line.rs` | 4 |
| `backend/entities/src/returns/purchase_return_order.rs` | `backend/crates/erp-returns/src/entity/returns/purchase_return_order.rs` | 5 |
| `backend/entities/src/returns/receipt_reversal.rs` | `backend/crates/erp-returns/src/entity/returns/receipt_reversal.rs` | 8 |
| `backend/entities/src/returns/sales_return_case.rs` | `backend/crates/erp-returns/src/entity/returns/sales_return_case.rs` | 5 |
| `backend/entities/src/returns/sales_return_line.rs` | `backend/crates/erp-returns/src/entity/returns/sales_return_line.rs` | 4 |
| `backend/entities/src/returns/supplier_refund.rs` | `backend/crates/erp-returns/src/entity/returns/supplier_refund.rs` | 8 |
| `backend/entities/src/returns/mod.rs` | `backend/crates/erp-returns/src/entity/returns/mod.rs` | 0 |
| `backend/database/src/repository/owned/customer_refund.rs` | `backend/crates/erp-returns/src/repository/owned/customer_refund.rs` | 0 |
| `backend/database/src/repository/owned/payment_reversal.rs` | `backend/crates/erp-returns/src/repository/owned/payment_reversal.rs` | 0 |
| `backend/database/src/repository/owned/purchase_return_line.rs` | `backend/crates/erp-returns/src/repository/owned/purchase_return_line.rs` | 0 |
| `backend/database/src/repository/owned/purchase_return_order.rs` | `backend/crates/erp-returns/src/repository/owned/purchase_return_order.rs` | 0 |
| `backend/database/src/repository/owned/receipt_reversal.rs` | `backend/crates/erp-returns/src/repository/owned/receipt_reversal.rs` | 0 |
| `backend/database/src/repository/owned/sales_return_case.rs` | `backend/crates/erp-returns/src/repository/owned/sales_return_case.rs` | 0 |
| `backend/database/src/repository/owned/sales_return_line.rs` | `backend/crates/erp-returns/src/repository/owned/sales_return_line.rs` | 0 |
| `backend/database/src/repository/owned/supplier_refund.rs` | `backend/crates/erp-returns/src/repository/owned/supplier_refund.rs` | 0 |
| `backend/database/src/repository/extensions/returns.rs` | `backend/crates/erp-returns/src/repository/extensions/returns.rs` | 0 |
| `backend/database/src/repository/returns.rs` | `backend/crates/erp-returns/src/repository/returns.rs` | 6 |
| `backend/database/src/repository/returns_posted_totals.rs` | `backend/crates/erp-returns/src/repository/returns_posted_totals.rs` | 2 |
| `backend/database/src/indexes/returns.rs` | `backend/crates/erp-returns/src/indexes/returns.rs` | 2 |

完整函数去向、原测试入口、collections/index names 与源 SHA-256 见 [机器证据](/private/tmp/returns13-persistence-map.json)。

## 5. 验证与集成边界

只执行定向 rustfmt 和静态逐函数/字段/测试核对。未运行 Cargo、MongoDB、历史 tests/** 或外部服务；真实数据库回滚、并发及索引创建未验证。公共 Cargo/边界门禁由 root 统一执行。

跨域 ignored 查询库测试由 G 分片迁入 erp-read-models；本分片已删除返回域中的旧 #[path] 注册，未为 erp-returns 添加 test-support dev 回边。共享旧根、HTTP 与外部消费者接线由各负责人完成；不得恢复旧域 façade。
