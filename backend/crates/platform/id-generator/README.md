# id-generator：内部 ID 与业务编号

<a name="使用场景"></a>

## 职责

`id-generator` 是后端共享的编号生成库。内部关联使用无业务含义的 `id`；人工查询、对账和追溯使用单号、作业号或主数据编码。两类标识不能互相替代。

| 入口 | 生成什么 | 何时使用 |
| --- | --- | --- |
| `next_id()` | 不带连字符的 32 位十六进制 UUID v4 字符串 | 新建对象时分配内部主键；不访问数据库，不需要进程锁或跨实例 worker ID。 |
| `DocumentNumberGenerator::next_number(kind, date, executor)` | 按种类分配业务编号，如 `SO20260701-000123` | 业务流程需要可展示编号时，从业务库原子取号。 |

本库维护前缀、格式和序号，不决定业务状态、取号时机、授权、幂等或业务事务边界。领域和编排用例负责这些规则，并将生成的编号写入对应字段；不要自行实现同用途的计数器或把内部 ID 拼成业务单号。

<a name="6-依赖与代码入口"></a>

## 目录结构

| 入口 | 内容 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | crate 清单。内部 ID 使用 `uuid`；业务取号使用 `mongodb`、`persistence-core`、`chrono` 等依赖；`autotests = false`。 |
| [src/lib.rs](src/lib.rs) | `next_id` 和公共导出：`DocumentNumberGenerator`、`DocumentNumberKind`、`format_number`、`Error`、`Result`、`Executor`、`NoTransaction`。 |
| [src/document_number.rs](src/document_number.rs) | 编号种类、持久化标识、格式化、计数器更新及相关库单元测试。 |
| [tests/](tests/) | 历史 MongoDB 集成测试源码，不在当前验证范围内。 |

## 架构设计

### 1. 业务编号格式

编号由**固定前缀 + 业务日期（`YYYYMMDD`）+ `-` + 序号**组成。例如 `SO20260701-000123` 表示业务日期为 2026-07-01 的销售单，使用销售单序列的第 123 号。序号至少六位，不足补零，超过六位原样展开。

调用方按用例规则传入业务日期。日期仅用于展示和追溯，不参与计数：同一业务库内，每个种类使用一条跨日递增的序列，不按日重置。`format_number(kind, date, seq)` 只拼接已有序号，不访问数据库，也不分配或保留序号。

### 2. DocumentNumberKind 完整清单

以 [源码中的 `document_number_kinds!` 声明](src/document_number.rs)为准，那里集中登记变体、前缀和持久化的计数器键。下表用于查找业务用途，不代表每个用例都已接入取号器。

| 变体 | 前缀 | 业务用途或字段 |
| --- | --- | --- |
| `SalesOrder` | `SO` | 销售单 `sales_orders.order_no` |
| `PurchaseOrder` | `PO` | 采购单 `purchase_orders.purchase_no` |
| `PurchaseReceipt` | `GRN` | 采购入库单 `purchase_receipts.receipt_no` |
| `Delivery` | `DN` | 履约发货单 `deliveries.delivery_no` |
| `CustomerAcceptance` | `CA` | 客户验收单 `customer_acceptances.acceptance_no` |
| `StockAdjustment` | `SA` | 库存调整单 `stock_adjustments.adjustment_no` |
| `CustomerReceipt` | `CR` | 客户回款单 `customer_receipts.receipt_no` |
| `SupplierPayment` | `PM` | 供应商付款单 `supplier_payments.payment_no` |
| `Invoice` | `INV` | 发票编号种类，销项和进项共用一条序列；不替代财务流程登记的票面号码。 |
| `SalesReturn` | `SR` | 销售退货／拒收单 `sales_return_cases.return_no` |
| `PurchaseReturn` | `PR` | 采购退货单 `purchase_return_orders.purchase_return_no` |
| `SupplierFulfillment` | `SF` | 供应商履约单；预留序列，业务流程未接入该序列。 |
| `SupplierSettlement` | `SS` | 供应商结算单；预留序列，业务流程未接入该序列。 |
| `Product` | `SPU` | 公司商品 `products.product_no`，内部建档未指定编号时生成。 |
| `ProductBrand` | `BR` | 商品品牌 `product_brands.brand_code` 的自动编码。 |
| `ProductCategory` | `CAT` | 商品分类 `product_categories.category_code` 的自动编码。 |
| `SupplierCatalogAdoption` | `SCA` | API 供应商批量入库作业 `supplier_catalog_adoption_batches.batch_no`。 |
| `SupplierDepositRecharge` | `DR` | 供应商预存款充值申请 `supplier_deposit_recharge_requests.request_no`。 |

销售变更单、采购变更单、电子交付和服务履约不使用本库的序列。商城订单、商城退款等外部追溯对象保留来源单号，不占用 ERP 序号。

### 3. 计数器集合与原子取号

`DocumentNumberGenerator::new(db)` 绑定业务库，不执行取号。该库的 `document_number_counters` 集合每种编号存一条文档：

| 字段 | 含义 |
| --- | --- |
| `_id` | 单据种类的固定 snake_case 标识，也是 `DocumentNumberKind` 的 serde 值，如 `sales_order`；不是展示前缀 `SO`。 |
| `seq` | 已消费的最高序号（`i64`）。 |
| `date` | 最近一次取号传入的业务日期字符串，如 `2026-07-01`；不参与查询或分配。 |
| `updated_at` | 通过 `$currentDate` 记录的 MongoDB 服务器时间（BSON 日期）。 |

取号只按 `_id` 匹配，使用单次 `find_one_and_update` 执行 `$inc: { seq: 1 }`、写入日期及更新时间，并启用 upsert、返回更新后的文档。首次自动建档并从 1 开始。同一业务库内，多实例对同一种类的自增由 MongoDB 串行化，不需要应用进程间协调。写入失败或未返回计数器文档时，取号返回错误。

**运维合同：**前缀启用后不得变更，已使用的计数器键也不得改名，否则会另起序列。删除单据、重置业务数据或恢复备份时，不得清空或回拨已使用的计数器；不能用当前单据的最大编号替代计数器，因为已消费的号码可能没有对应单据。[开发业务数据重置规则](../../../scripts/reset-dev-business-data.md#4-明确保留项)同样要求保留 `document_number_counters`。

### 4. 事务与回滚要求

- `next_number` 接收 `&mut dyn Executor`：在 `with_transaction` 回调中可传当前执行器，独立取号传 `&mut NoTransaction`。实现只读取执行器的快照边界用于调试日志，不把计数器更新挂到调用方会话上。**两种传法下计数器都独立自动提交。**
- 取号成功即消费序号，后续校验失败或业务事务回滚都不回收。例如空销售单计数器在事务内取到 `SO20260701-000001` 后回滚，若无其他取号，下一次会得到 `SO20260701-000002`。正式业务记录允许跳号，不能把编号连续性当作业务成功或提交顺序的依据。
- Service 或 Process 负责开启业务事务；决定写入的授权、事实校验、业务持久化及应一起提交的审计仍沿用原执行器。尽量在必要校验完成后取号。取号器不提供命令幂等：业务提交结果未知时使用原幂等键重试，先识别或返回已登记结果，不为同一已完成动作重新取号。参见[后端事务规则](../../../AGENTS.md#授权和事务)和[持久化合同](../../foundation/persistence-core/README.md#读取边界)。
- 普通草稿不预占正式号；一般在提交、过账或登记等正式化步骤取号。逻辑删除的普通草稿不应影响正式号序列。取号器本身不检查状态：已有履约流程会在登记发货、采购入库或客户验收草稿时分配各自的展示编号，已分配的号码即使未过账也不回收，见[履约取号入口](../../domains/erp-fulfillment/src/service/document_number.rs)。
- [API 供应商批量入库](../../orchestration/erp-supplier-catalog/src/supplier_catalog_adoption/batch_selection.rs)在创建选择快照时分配 `SCA` 作业编号，草稿和后续执行沿用同一编号。作业编号不表示商品已入库；选择快照创建失败，已消费序号同样不回收。

<a name="协作示例"></a>

## 5. 使用示例

下面演示独立取号；`db` 是调用方已装配的业务数据库。若从业务事务调用，将 `&mut NoTransaction` 换成回调传入的 `executor`，取号仍按[第 4 节](#4-事务与回滚要求)独立提交。

```rust
use chrono::NaiveDate;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, NoTransaction, next_id};
use mongodb::Database;

async fn sales_order_ids(db: Database, business_date: NaiveDate) -> id_generator::Result<(String, String)> {
    let id = next_id();
    let order_no = DocumentNumberGenerator::new(db)
        .next_number(DocumentNumberKind::SalesOrder, business_date, &mut NoTransaction)
        .await?;
    // 例如业务日期为 2026-07-01，取到序号 123 时，order_no 为 SO20260701-000123。
    Ok((id, order_no))
}
```

## 验证要求

仅改文档时，按[后端开发规则](../../../AGENTS.md#验证)只对变更文件执行 `git diff --check`。代码变更按[统一质量门禁](../../README.md#质量门禁)执行对应检查；本 crate 的库单元测试命令在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p id-generator --lib --locked
```

不执行集成测试或依赖真实数据库、外部服务的测试。
