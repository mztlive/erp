use entities::payable::{PayableAccountStatus, PayableSourceType};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::stable::StableBase;
use erp_core::ids::{PayableAccountId, SupplierAccountId};
use erp_core::money::Amount;
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};

use super::super::{Pagination, QueryFilter};

mod invoicing;
mod query;
mod settlement;
mod write;

#[cfg(test)]
mod tests;

/// 批量条件核销结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementBatchResult {
    /// 条件命中并完成核销的账户（输入顺序）。
    pub applied: Vec<PayableAccountId>,
    /// 条件未命中（超过剩余开放余额）被拒绝的账户（输入顺序）。
    pub rejected: Vec<PayableAccountId>,
}

/// 批量条件收票结果：按账户逐个报告命中情况，由 Service 转译业务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvoicingBatchResult {
    /// 条件命中并完成收票的账户（输入顺序）。
    pub applied: Vec<PayableAccountId>,
    /// 条件未命中（超过剩余可收票额度）被拒绝的账户（输入顺序）。
    pub rejected: Vec<PayableAccountId>,
}

/// 应付往来子账列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PayableAccountRow {
    /// 实体主键。
    pub id: String,
    /// 稳定公共字段（状态/版本归属/审计人）。
    #[serde(flatten)]
    pub stable: StableBase<PayableAccountStatus>,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 往来供应商。
    pub supplier_id: String,
    /// 来源类型。
    pub source_type: PayableSourceType,
    /// 含税应付总额。
    pub gross_total: Amount,
    /// 已核销含税总额。
    pub settled_total: Amount,
    /// 剩余开放含税余额。
    pub open_total: Amount,
    /// 可收票含税总额。
    pub invoiceable_total: Amount,
    /// 净已收票含税总额。
    pub invoiced_total: Amount,
    /// 剩余可收票含税额度。
    pub open_invoiceable_total: Amount,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 应付往来子账列表筛选条件。
#[derive(Debug, Clone)]
pub struct PayableAccountFilter {
    /// 往来供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源类型；`None` 表示不筛选。
    pub source_type: Option<PayableSourceType>,
    /// 子账状态；`None` 表示不筛选。
    pub status: Option<PayableAccountStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内有效，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for PayableAccountFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for PayableAccountFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}
