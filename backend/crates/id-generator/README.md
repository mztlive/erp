# ID Generator（id-generator）

共享 ID 与可展示业务编号生成能力。

| 能力 | 用途 | 说明 |
| --- | --- | --- |
| `next_id()` | 内部主键 `id` | UUID v4，32 位十六进制，不承载业务含义（数据模型 4.1） |
| `DocumentNumberGenerator::next_number` | 业务编号 `*_no` | 原子取号，一经形成正式事实不得复用（数据模型 4.1） |

## 1. 业务编号格式

```
前缀 + 业务日期(YYYYMMDD) + "-" + 6 位补零序号段
```

示例：`SO20260701-000123`（销售单，业务日期 2026-07-01，第 123 号）。

- 日期段来自调用方传入的业务日期，仅作展示段，不参与序号分配；
- 序号段 6 位补零；超过 6 位（>999999）时直接展开位数，不截断；
- 序号按单据种类全局递增，跨业务日期连续，不按日重置（同一 kind 的序号空间全局唯一）。

## 2. DocumentNumberKind 完整清单

| 变体 | 前缀 | 中文名 | 阶段 | 数据模型落点 |
| --- | --- | --- | --- | --- |
| `SalesOrder` | `SO` | 销售单 | 一期 | `sales_order.order_no` |
| `PurchaseOrder` | `PO` | 采购单 | 一期 | `purchase_order.purchase_no` |
| `PurchaseReceipt` | `GRN` | 采购入库单 | 一期 | `purchase_receipt.receipt_no` |
| `Delivery` | `DN` | 履约发货单 | 一期 | `delivery.delivery_no` |
| `CustomerAcceptance` | `CA` | 客户验收单 | 一期 | `customer_acceptance.acceptance_no` |
| `StockAdjustment` | `SA` | 库存调整单 | 一期 | `stock_adjustment.adjustment_no` |
| `CustomerReceipt` | `CR` | 客户回款单 | 一期 | `customer_receipt.receipt_no` |
| `SupplierPayment` | `PM` | 供应商付款单 | 一期 | `supplier_payment.payment_no` |
| `Invoice` | `INV` | 发票 | 一期 | `invoice`（销项/进项共用序号空间） |
| `SalesReturn` | `SR` | 销售退货单 | 一期 | `sales_return_case.return_no` |
| `PurchaseReturn` | `PR` | 采购退货单 | 一期 | `purchase_return_order.purchase_return_no` |
| `SupplierFulfillment` | `SF` | 供应商履约单 | 二期（仅预声明） | `supplier_fulfillment_order` |
| `SupplierSettlement` | `SS` | 供应商结算单 | 二期（仅预声明） | `supplier_settlement_statement` |

数据模型未定义前缀时按业务缩写设计（SO=Sales Order、PO=Purchase Order、
GRN=Goods Receipt Note、DN=Delivery Note、CA=Customer Acceptance、
SA=Stock Adjustment、CR=Customer Receipt、PM=Payment、INV=Invoice、
SR=Sales Return、PR=Purchase Return、SF=Supplier Fulfillment、
SS=Supplier Settlement）。**前缀一经启用不得变更**（历史编号依赖前缀，见编号格式）。

未占用序号的说明：

- 销售变更单/采购变更单、电子交付、服务履约等对象在数据模型未定义 `*_no`，不在此列；
- 商城订单、商城退款等二期追溯对象由外部事实形成，保留外部单号，不占用 ERP 序号空间。

## 3. 计数器集合与原子取号

集合 `document_number_counters`，`_id` 为单据种类的 serde 标识（snake_case，
与持久化序列化一致），单文档单 kind：

```json
{
  "_id": "sales_order",
  "seq": 123,
  "date": "2026-07-01",
  "updated_at": "<服务器时间>"
}
```

取号使用 `find_one_and_update` + `$inc: { seq: 1 }` + upsert + 返回更新后文档
（findAndModify 原子计数器）：

- 单次文档级原子操作，并发取号在 MongoDB 侧串行化，无重复、无进程内锁、无跨实例协调；
- upsert 保证首次取号自动建立计数器并从 1 开始；
- **取号成功即消费序号**——消费发生在取号瞬间，与后续业务步骤是否成功无关。

## 4. 事务与回滚行为（重要）

- `next_number(kind, date, executor)` 接收执行器，以便在 `with_transaction` 事务代码内
  以相同签名调用（传入 `&mut ClientSession`），单集合独立取号时传 `&mut NoTransaction`；
- **计数器自增始终以自动提交方式独立执行，不挂到调用方事务会话上**：即使传入
  事务执行器，事务回滚也不会撤销已消费的序号；
- 因此"事务内取号 → 回滚 → 再取号"得到 `SO20260701-000001` 与 `SO20260701-000002`，
  1 号不再被回收——正式单据序列出现 1 号空缺（**跳号是预期行为**）；
- 原因（取舍）：**防重复优先于防跳号**。`*_no` 一经形成正式事实不得复用
  （数据模型 4.1）；跳号只影响编号美观，回收复用则会造成两张正式单据同号，
  破坏唯一性、对账与追溯；
- 取号时机约定：只在正式化（提交/过账/登记）时取正式号；草稿不预占正式号，
  逻辑删除的草稿不进入编号连续性（数据模型 4.5 第 2 条）。

## 5. 使用示例

```rust
use chrono::NaiveDate;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, NoTransaction};

let generator = DocumentNumberGenerator::new(db);
let mut executor = NoTransaction;
let number = generator
    .next_number(
        DocumentNumberKind::SalesOrder,
        NaiveDate::from_ymd_opt(2026, 7, 1).unwrap(),
        &mut executor,
    )
    .await?;
// number == "SO20260701-000123"
```

## 6. 测试

- 单元测试（无数据库环境）：`cargo test -p id-generator`
  ——覆盖全部 kind 的前缀/中文名/阶段、serde snake_case、日期段格式、序号补零；
- 集成测试（需真实 MongoDB；事务测试需副本集）：
  - `tests/number_concurrency.rs`：50 任务 × 20 次并发取号，断言 1000 个号
    全部唯一且序号段为 `1..=1000` 的连续排列（无跳号）；
  - `tests/number_rollback.rs`：事务内取号后回滚，断言序号不回收、不回收到
    同一号（正式序列跳号），计数器继续前进不回退；
- 集成测试以 `#[ignore]` 门控并读取环境变量 `ERP_TEST_MONGO_URI`，未设置时跳过：

  ```bash
  ERP_TEST_MONGO_URI=mongodb://127.0.0.1:27017 \
    cargo test -p id-generator -- --include-ignored
  ```

  多文档事务测试需要副本集连接串（如 `scripts/dev-mongo.sh` 启动的单节点副本集 `rs0`）。
