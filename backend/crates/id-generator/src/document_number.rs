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

/// 可展示业务编号（`*_no`）的单据种类。
///
/// 每个变体包含业务前缀（见 `prefix`），serde 序列化为 snake_case 字符串
/// （如 `sales_order`），同时作为计数器集合 [`COUNTER_COLLECTION`] 的 `_id`，
/// 保证持久化标识与应用层一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentNumberKind {
    /// 销售单（一期），对应 `sales_order.order_no`。
    SalesOrder,

    /// 采购单（一期），对应 `purchase_order.purchase_no`。
    PurchaseOrder,

    /// 采购入库单（一期），对应 `purchase_receipt.receipt_no`。
    PurchaseReceipt,

    /// 履约发货单（一期），对应 `delivery.delivery_no`。
    Delivery,

    /// 客户验收单（一期），对应 `customer_acceptance.acceptance_no`。
    CustomerAcceptance,

    /// 库存调整单（一期），对应 `stock_adjustment.adjustment_no`。
    StockAdjustment,

    /// 客户回款单（一期），对应 `customer_receipt.receipt_no`。
    CustomerReceipt,

    /// 供应商付款单（一期），对应 `supplier_payment.payment_no`。
    SupplierPayment,

    /// 发票（一期），对应 `invoice`，销项与进项共用同一序号空间。
    Invoice,

    /// 销售退货/拒收单（一期），对应 `sales_return_case.return_no`。
    SalesReturn,

    /// 采购退货单（一期），对应 `purchase_return_order.purchase_return_no`。
    PurchaseReturn,

    /// 供应商履约单（二期预声明），对应 `supplier_fulfillment_order`。
    SupplierFulfillment,

    /// 供应商结算单（二期预声明），对应 `supplier_settlement_statement`。
    SupplierSettlement,
}

impl Display for DocumentNumberKind {
    /// 格式化计数器标识。
    ///
    /// # 参数
    /// * `formatter` - 格式化器
    ///
    /// # 返回
    /// 写入静态计数器名。
    ///
    /// # 错误
    /// 写入失败时返回格式化错误。
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str(self.counter_id_str())
    }
}

impl DocumentNumberKind {
    /// 返回单据种类的业务前缀。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 前缀为固定大写业务缩写（如 `SO`），编号格式串的一部分，一经启用不得变更。
    ///
    /// # 错误
    /// 无。
    pub fn prefix(self) -> &'static str {
        self.metadata().0
    }

    /// 返回计数器文档 `_id` 静态名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回与 serde snake_case 一致的静态计数器名。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn counter_id_str(self) -> &'static str {
        self.metadata().1
    }

    /// 单据种类的前缀与计数器标识，保持一一对应。
    fn metadata(self) -> (&'static str, &'static str) {
        match self {
            Self::SalesOrder => ("SO", "sales_order"),
            Self::PurchaseOrder => ("PO", "purchase_order"),
            Self::PurchaseReceipt => ("GRN", "purchase_receipt"),
            Self::Delivery => ("DN", "delivery"),
            Self::CustomerAcceptance => ("CA", "customer_acceptance"),
            Self::StockAdjustment => ("SA", "stock_adjustment"),
            Self::CustomerReceipt => ("CR", "customer_receipt"),
            Self::SupplierPayment => ("PM", "supplier_payment"),
            Self::Invoice => ("INV", "invoice"),
            Self::SalesReturn => ("SR", "sales_return"),
            Self::PurchaseReturn => ("PR", "purchase_return"),
            Self::SupplierFulfillment => ("SF", "supplier_fulfillment"),
            Self::SupplierSettlement => ("SS", "supplier_settlement"),
        }
    }
}

/// 组装业务编号：前缀 + 业务日期（YYYYMMDD）+ 连字符 + 6 位补零序号段。
///
/// 例如 `SO20260701-000123`。序号段超过 6 位时直接展开位数，不截断。
///
/// # 参数
/// * `kind` - 单据种类，决定前缀
/// * `date` - 业务日期，决定日期段
/// * `seq` - 已消费的序号
///
/// # 返回
/// 返回完整业务编号字符串。
///
/// # 错误
/// 无。
pub fn format_number(kind: DocumentNumberKind, date: NaiveDate, seq: i64) -> String {
    format!("{}{}-{:0width$}", kind.prefix(), display_date(date), seq, width = SEQ_WIDTH)
}

/// 格式化编号展示段日期（YYYYMMDD）。
fn display_date(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

/// 格式化计数器追溯段日期（YYYY-MM-DD）。
fn trace_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
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
    /// 创建业务编号取号器。
    ///
    /// # 参数
    /// * `db` - 目标数据库（调用方业务库）
    ///
    /// # 返回
    /// 返回取号器实例。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 取下一个业务编号。
    ///
    /// # 参数
    /// * `kind` - 单据种类，决定编号前缀与计数器标识
    /// * `date` - 业务日期，决定编号日期段（YYYYMMDD）
    /// * `executor` - 数据访问执行器；计数器自增始终独立提交，不加入调用方事务
    ///
    /// # 返回
    /// 返回形如 `SO20260701-000123` 的业务编号字符串。
    ///
    /// # 错误
    /// 底层 MongoDB 写入失败或计数器 upsert 后未返回文档时返回错误。
    pub async fn next_number(
        &self,
        kind: DocumentNumberKind,
        date: NaiveDate,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let in_transaction = executor.session().is_some();
        tracing::debug!(kind = %kind.counter_id_str(), in_transaction, "taking next document number");
        let seq = self.next_seq(kind, date).await?;
        Ok(format_number(kind, date, seq))
    }

    /// 原子自增计数器并返回新序号。
    ///
    /// 计数器自增不使用执行器会话，按自动提交方式执行，保证序号一经消费
    /// 不随调用方事务回滚而回收（防重复优先于防跳号）。
    ///
    /// # 参数
    /// * `kind` - 单据种类
    /// * `date` - 业务日期，写入计数器文档用于追溯
    ///
    /// # 返回
    /// 返回本次消费的序号（首次取号为 1）。
    ///
    /// # 错误
    /// 底层 MongoDB 写入失败或计数器 upsert 后未返回文档时返回错误。
    async fn next_seq(&self, kind: DocumentNumberKind, date: NaiveDate) -> Result<i64> {
        let counter_id = kind.counter_id_str();
        let counter = self
            .db
            .collection::<NumberCounter>(COUNTER_COLLECTION)
            .find_one_and_update(
                doc! { "_id": counter_id },
                doc! {
                    "$inc": { "seq": 1_i64 },
                    "$set": { "date": trace_date(date) },
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

    const KIND_TABLE: [(DocumentNumberKind, &str); 13] = [
        (DocumentNumberKind::SalesOrder, "SO"),
        (DocumentNumberKind::PurchaseOrder, "PO"),
        (DocumentNumberKind::PurchaseReceipt, "GRN"),
        (DocumentNumberKind::Delivery, "DN"),
        (DocumentNumberKind::CustomerAcceptance, "CA"),
        (DocumentNumberKind::StockAdjustment, "SA"),
        (DocumentNumberKind::CustomerReceipt, "CR"),
        (DocumentNumberKind::SupplierPayment, "PM"),
        (DocumentNumberKind::Invoice, "INV"),
        (DocumentNumberKind::SalesReturn, "SR"),
        (DocumentNumberKind::PurchaseReturn, "PR"),
        (DocumentNumberKind::SupplierFulfillment, "SF"),
        (DocumentNumberKind::SupplierSettlement, "SS"),
    ];

    #[test]
    fn kind_metadata_matches_design_table() {
        for (kind, prefix) in KIND_TABLE {
            assert_eq!(kind.prefix(), prefix);
        }
    }

    #[test]
    fn kind_serializes_to_stable_snake_case_names() {
        let cases = [
            (DocumentNumberKind::SalesOrder, "sales_order"),
            (DocumentNumberKind::CustomerReceipt, "customer_receipt"),
            (DocumentNumberKind::SupplierSettlement, "supplier_settlement"),
            (DocumentNumberKind::Invoice, "invoice"),
        ];
        for (kind, expected) in cases {
            let serialized = serialize_to_bson(&kind).expect("kind should serialize");
            assert_eq!(serialized, Bson::String(expected.to_string()));
            let deserialized =
                deserialize_from_bson::<DocumentNumberKind>(serialized).expect("kind should deserialize");
            assert_eq!(deserialized, kind);
        }
    }

    #[test]
    fn counter_id_matches_serialized_kind_name() {
        for (kind, _) in KIND_TABLE {
            let Bson::String(name) = serialize_to_bson(&kind).expect("kind should serialize") else {
                panic!("kind must serialize to a string");
            };
            assert_eq!(kind.counter_id_str(), name.as_str());
            assert_eq!(kind.to_string(), name.as_str());
        }
    }

    #[test]
    fn format_number_uses_prefix_date_and_zero_padded_seq() {
        let date = NaiveDate::from_ymd_opt(2026, 7, 1).expect("valid date");

        assert_eq!(format_number(DocumentNumberKind::SalesOrder, date, 123), "SO20260701-000123");
        assert_eq!(format_number(DocumentNumberKind::Invoice, date, 1), "INV20260701-000001");
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
