//! 可展示业务编号（`*_no`）生成能力。
//!
//! 数据模型 4.1 约定 `*_no` 是可展示业务编号，一经形成正式事实不得复用；
//! 数据模型 4.5 约定逻辑删除的草稿不进入编号连续性。本模块通过 MongoDB 集合
//! `document_number_counters` 上的原子计数器（findAndModify 模式）取号，
//! 保证并发环境下取号唯一、序号连续递增，且序号一经消费永不回收。

use std::fmt::{Display, Formatter, Result as FmtResult};
use std::result::Result as StdResult;

use chrono::NaiveDate;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::ReturnDocument;
use persistence_core::{Error as DatabaseError, Executor};
use serde::{Deserialize, Serialize};

/// 计数器集合名称。
pub const COUNTER_COLLECTION: &str = "document_number_counters";

/// 序号段补零位数。
pub const SEQ_WIDTH: usize = 6;

/// 编号生成错误。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 计数器 upsert 后仍未返回文档，理论上不可达。
    #[error("document number counter missing after upsert for kind '{kind}'")]
    CounterMissing { kind: DocumentNumberKind },

    /// 底层 MongoDB 错误。
    #[error("database error: {0}")]
    Database(#[from] DatabaseError),
}

/// 编号生成结果类型.
pub type Result<T> = StdResult<T, Error>;

macro_rules! document_number_kinds {
    ($($(#[$doc:meta])* $variant:ident => ($prefix:literal, $counter:literal)),+ $(,)?) => {
        /// 可展示业务编号（`*_no`）的单据种类。
        ///
        /// 每个变体包含业务前缀（见 `prefix`），serde 序列化为 snake_case 字符串
        /// （如 `sales_order`），同时作为计数器集合 [`COUNTER_COLLECTION`] 的 `_id`，
        /// 保证持久化标识与应用层一致。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum DocumentNumberKind {
            $(
                $(#[$doc])*
                #[serde(rename = $counter)]
                $variant,
            )+
        }

        impl DocumentNumberKind {
            /// 返回该单据种类固定的大写业务前缀（如 `SO`）。
            pub fn prefix(self) -> &'static str {
                match self {
                    $(Self::$variant => $prefix,)+
                }
            }

            fn counter_id_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $counter,)+
                }
            }
        }
    };
}

document_number_kinds! {
    /// 销售单（一期），对应 `sales_order.order_no`。
    SalesOrder => ("SO", "sales_order"),
    /// 采购单（一期），对应 `purchase_order.purchase_no`。
    PurchaseOrder => ("PO", "purchase_order"),
    /// 采购入库单（一期），对应 `purchase_receipt.receipt_no`。
    PurchaseReceipt => ("GRN", "purchase_receipt"),
    /// 履约发货单（一期），对应 `delivery.delivery_no`。
    Delivery => ("DN", "delivery"),
    /// 客户验收单（一期），对应 `customer_acceptance.acceptance_no`。
    CustomerAcceptance => ("CA", "customer_acceptance"),
    /// 库存调整单（一期），对应 `stock_adjustment.adjustment_no`。
    StockAdjustment => ("SA", "stock_adjustment"),
    /// 客户回款单（一期），对应 `customer_receipt.receipt_no`。
    CustomerReceipt => ("CR", "customer_receipt"),
    /// 供应商付款单（一期），对应 `supplier_payment.payment_no`。
    SupplierPayment => ("PM", "supplier_payment"),
    /// 发票（一期），对应 `invoice`，销项与进项共用同一序号空间。
    Invoice => ("INV", "invoice"),
    /// 销售退货/拒收单（一期），对应 `sales_return_case.return_no`。
    SalesReturn => ("SR", "sales_return"),
    /// 采购退货单（一期），对应 `purchase_return_order.purchase_return_no`。
    PurchaseReturn => ("PR", "purchase_return"),
    /// 供应商履约单（二期预声明），对应 `supplier_fulfillment_order`。
    SupplierFulfillment => ("SF", "supplier_fulfillment"),
    /// 供应商结算单（二期预声明），对应 `supplier_settlement_statement`。
    SupplierSettlement => ("SS", "supplier_settlement"),
    /// 公司商品（SPU），对应内部建档未指定编号时的 `product.product_no`。
    Product => ("SPU", "product"),
    /// 商品品牌，对应自动生成的 `product_brand.brand_code`。
    ProductBrand => ("BR", "product_brand"),
    /// 商品分类，对应自动生成的 `product_category.category_code`。
    ProductCategory => ("CAT", "product_category"),
    /// API 供应商批量入库作业，对应 `supplier_catalog_adoption_batches.batch_no`。
    SupplierCatalogAdoption => ("SCA", "supplier_catalog_adoption"),
    /// 供应商预存款充值申请。
    SupplierDepositRecharge => ("DR", "supplier_deposit_recharge"),
}

impl Display for DocumentNumberKind {
    // 格式化单据种类时使用与 MongoDB `_id` 一致的计数器标识，便于从取号错误定位计数器，
    // 而不是输出供业务编号展示的大写前缀。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.counter_id_str())
    }
}

/// 将已分配的序号组装为可展示的业务编号，例如 `SO20260701-000123`。
/// 日期使用业务方传入的 `YYYYMMDD`，只用于展示；序号补足 6 位，超过 6 位不截断。
/// 此函数只格式化，不会访问数据库或消费序号。
pub fn format_number(kind: DocumentNumberKind, date: NaiveDate, seq: i64) -> String {
    format!("{}{}-{:0width$}", kind.prefix(), date.format("%Y%m%d"), seq, width = SEQ_WIDTH)
}

/// 计数器集合中的文档形态。
#[derive(Debug, Deserialize)]
struct NumberCounter {
    /// 已消费的最大序号。
    seq: i64,
}

/// 业务编号原子取号器。
///
/// 底层使用 MongoDB `find_one_and_update` + `$inc` + upsert 实现原子计数器
/// （findAndModify 模式）：并发取号在 MongoDB 侧串行化，取号成功即消费序号。
///
/// 计数器集合与调用方业务事务解耦：即使传入事务执行器，计数器自增也始终以
/// 自动提交方式独立执行，不随调用方事务回滚（行为与原因见 crate README 第 5 节）。
#[derive(Debug, Clone)]
pub struct DocumentNumberGenerator {
    db: Database,
}

impl DocumentNumberGenerator {
    /// 在装配业务用例时，绑定保存 `document_number_counters` 的业务数据库；构造时不会取号。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 在单据提交、过账或登记等正式化时分配业务编号（例如 `SO20260701-000123`），草稿不预占正式号。
    /// API 供应商批量入库使用作业编号，在创建选择快照时取号，后续沿用同一编号。
    /// 序号按种类跨日递增，业务日期只用于编号展示。即使传入事务执行器，计数器也独立提交；
    /// 业务回滚后不得回收已消费的序号，调用方须允许跳号。
    ///
    /// # Errors
    /// MongoDB 写入失败或计数器 upsert 后没有返回文档时返回错误。
    pub async fn next_number(
        &self,
        kind: DocumentNumberKind,
        date: NaiveDate,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let in_transaction = executor.boundary().is_some_and(|boundary| boundary.is_snapshot());
        tracing::debug!(kind = %kind.counter_id_str(), in_transaction, "taking next document number");

        let seq = self.next_seq(kind, date).await?;
        Ok(format_number(kind, date, seq))
    }

    // 正式取号时按单据种类原子自增并取回新序号，首次取号由 upsert 建档并从 1 开始。
    // 业务日期写入计数器仅用于追溯；独立提交使已消费序号不随业务事务回滚，宁可跳号也不重号。
    async fn next_seq(&self, kind: DocumentNumberKind, date: NaiveDate) -> Result<i64> {
        let counter_id = kind.counter_id_str();
        let counter = self
            .db
            .collection::<NumberCounter>(COUNTER_COLLECTION)
            .find_one_and_update(
                doc! { "_id": counter_id },
                doc! {
                    "$inc": { "seq": 1_i64 },
                    "$set": { "date": date.format("%Y-%m-%d").to_string() },
                    "$currentDate": { "updated_at": true },
                },
            )
            .upsert(true)
            .return_document(ReturnDocument::After)
            .await
            .map_err(DatabaseError::from)?;

        let Some(counter) = counter else {
            return Err(Error::CounterMissing { kind });
        };
        Ok(counter.seq)
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use mongodb::bson::{Bson, deserialize_from_bson, serialize_to_bson};

    use super::{COUNTER_COLLECTION, DocumentNumberKind, SEQ_WIDTH, format_number};

    #[test]
    fn number_prefixes_and_persisted_names_remain_stable() {
        use DocumentNumberKind::*;
        for (kind, prefix, name) in [
            (SalesOrder, "SO", "sales_order"),
            (PurchaseOrder, "PO", "purchase_order"),
            (PurchaseReceipt, "GRN", "purchase_receipt"),
            (Delivery, "DN", "delivery"),
            (CustomerAcceptance, "CA", "customer_acceptance"),
            (StockAdjustment, "SA", "stock_adjustment"),
            (CustomerReceipt, "CR", "customer_receipt"),
            (SupplierPayment, "PM", "supplier_payment"),
            (Invoice, "INV", "invoice"),
            (SalesReturn, "SR", "sales_return"),
            (PurchaseReturn, "PR", "purchase_return"),
            (SupplierFulfillment, "SF", "supplier_fulfillment"),
            (SupplierSettlement, "SS", "supplier_settlement"),
            (Product, "SPU", "product"),
            (ProductBrand, "BR", "product_brand"),
            (ProductCategory, "CAT", "product_category"),
            (SupplierCatalogAdoption, "SCA", "supplier_catalog_adoption"),
            (SupplierDepositRecharge, "DR", "supplier_deposit_recharge"),
        ] {
            assert_eq!(kind.prefix(), prefix);
            assert_eq!(kind.counter_id_str(), name);
            assert_eq!(kind.to_string(), name);

            let stored = Bson::String(name.to_owned());
            assert_eq!(serialize_to_bson(&kind).unwrap(), stored);
            assert_eq!(deserialize_from_bson::<DocumentNumberKind>(stored).unwrap(), kind);
        }
    }

    #[test]
    fn format_number_uses_prefix_date_and_zero_padded_seq() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 1).expect("valid date");

        assert_eq!(format_number(DocumentNumberKind::SalesOrder, date, 123), "SO20260701-000123");
        assert_eq!(format_number(DocumentNumberKind::Invoice, date, 1), "INV20260701-000001");
        assert_eq!(format_number(DocumentNumberKind::Product, date, 7), "SPU20260701-000007");
        assert_eq!(format_number(DocumentNumberKind::ProductBrand, date, 1), "BR20260701-000001");
        assert_eq!(format_number(DocumentNumberKind::ProductCategory, date, 1), "CAT20260701-000001");
        assert_eq!(format_number(DocumentNumberKind::SupplierCatalogAdoption, date, 8), "SCA20260701-000008");
    }

    #[test]
    fn format_number_expands_seq_beyond_width_without_truncation() {
        let date = NaiveDate::from_ymd_opt(2026, 12, 31).expect("valid date");
        assert_eq!(format_number(DocumentNumberKind::SalesOrder, date, 1_000_000), "SO20261231-1000000");
    }

    #[test]
    fn counter_collection_and_seq_width_constants_are_fixed() {
        assert_eq!(COUNTER_COLLECTION, "document_number_counters");
        assert_eq!(SEQ_WIDTH, 6);
    }
}
