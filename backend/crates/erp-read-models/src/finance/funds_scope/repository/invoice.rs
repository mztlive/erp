//! 发票最终授权份额、稳定页面与完整轻量版本投影。

use std::hash::{Hash, Hasher};

use erp_core::money::Amount;
use erp_finance::dto::payable::PurchaseInvoiceAllocationView;
use erp_finance::dto::receivable::{InvoiceListQuery, SortDir};
use erp_finance::entity::payable::{AllocationAction as PurchaseAction, PurchaseInvoiceAllocation};
use erp_finance::entity::receivable::{AllocationAction as SalesAction, SalesInvoiceAllocation};
use erp_finance::repository::{InvoiceFilter, InvoiceRow, PayableExt, ReceivableExt};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, QueryFilter};
use serde::Deserialize;

use super::super::allocation::PurchaseInvoiceLink;
use super::super::invoice::sales_invoice_allocation_view;
use super::super::rows::{FundsSummaryView, SalesInvoiceLink, zero_amount};
use super::super::{FundsAuthorization, FundsLinkedCondition};
use super::invoice_sources::{allocation_lookup, purchase_stages, sales_stages};
use super::invoice_summary::{InvoiceSummaryHeader, InvoiceSummaryRow, invoice_summary, summary_rows};
use super::{aggregate, page_only_facet, sort_document};
use crate::{Error, Result};

/// 一拍数据库结果；完整实体只在当前页，完整金额流只用于汇总。
#[derive(Debug, Default, Deserialize)]
pub(in crate::finance::funds_scope) struct InvoiceSnapshot {
    #[serde(default)]
    pub items: Vec<InvoicePageRow>,
    #[serde(default)]
    total: Vec<CountRow>,
    #[serde(default)]
    pub versions: Vec<InvoiceVersion>,
    #[serde(default)]
    summary: Vec<InvoiceSummaryRow>,
}

/// 当前页票面和实际分配；来源匹配键已在数据库内求交。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct InvoicePageRow {
    #[serde(flatten)]
    pub row: InvoiceRow,
    sales: Vec<SaleRow>,
    purchase: Vec<PurchaseRow>,
    pub sales_matched: Vec<String>,
    pub purchase_matched: Vec<String>,
    pub whole: bool,
}

/// 当前页销项分配，使用原实体解码约束和原响应转换。
#[derive(Debug, Deserialize)]
struct SaleRow {
    #[serde(flatten)]
    item: SalesInvoiceAllocation,
    order: Option<String>,
}

/// 当前页进项分配，原分配实体继续校验必要字段和枚举。
#[derive(Debug, Deserialize)]
struct PurchaseRow {
    #[serde(flatten)]
    item: PurchaseInvoiceAllocation,
    order: Option<String>,
}

/// 同口径计数，来源资格过滤必须在此分支之前完成。
#[derive(Debug, Deserialize)]
struct CountRow {
    count: u64,
}

/// 哈希沿旧行与来源键顺序投影；首拍附带汇总资格，复核拍省略金额及负责人。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct InvoiceVersion {
    pub id: String,
    pub version: u64,
    pub sales_versions: Vec<u64>,
    pub purchase_versions: Vec<u64>,
    /// 仅首拍读取汇总资格；复核拍不投影金额或负责人。
    #[serde(default)]
    pub(super) summary: Option<InvoiceSummaryHeader>,
}

impl InvoiceVersion {
    /// 保留旧协议逐票哈希主键、版本及按来源键排序的责任版本。
    ///
    /// # 参数
    /// 原指纹 hasher。
    /// # 返回
    /// 按既有协议追加当前记录与来源版本。
    /// # 错误
    /// 无。
    pub fn hash_into(&self, fingerprint: &mut impl Hasher) {
        self.id.hash(fingerprint);
        self.version.hash(fingerprint);
        for version in self.sales_versions.iter().chain(&self.purchase_versions) {
            version.hash(fingerprint);
        }
    }
}

impl InvoiceSnapshot {
    /// 用最终授权集合执行原查询保护，超限整体拒绝而不截断结果。
    ///
    /// # 参数
    /// 最终授权快照。
    /// # 返回
    /// 未达到原查询上限时返回成功。
    /// # 错误
    /// 最终匹配达到 10000 条时返回原查询保护错误。
    pub fn ensure_bounded(&self) -> Result<()> {
        if self.versions.len() >= 10_000 {
            return Err(Error::ValidationError("发票查询超过上限，请收窄组织或负责人条件".into()));
        }
        Ok(())
    }

    /// 返回与页面同一最终条件的数据库计数，空匹配为零。
    ///
    /// # 参数
    /// 当前快照。
    /// # 返回
    /// 数据库最终授权条件的计数。
    /// # 错误
    /// 无。
    pub fn total(&self) -> u64 {
        self.total.first().map_or(0, |row| row.count)
    }

    /// 完整轻量金额流继续使用原 Rust 定点算法归并，部分整单合计为 null。
    ///
    /// # 参数
    /// 首拍金额事实及完整范围版本。
    /// # 返回
    /// 原定点算法归并的授权份额与整单金额资格。
    /// # 错误
    /// 必要事实损坏时拒绝；金额越界保留原金额类型的溢出行为。
    pub fn summary(&self, version: &str) -> Result<FundsSummaryView> {
        invoice_summary(&self.summary, version)
    }
}

impl InvoicePageRow {
    /// 当前页分配恢复原裁剪单元，金额正反不由票面差额推导。
    ///
    /// # 参数
    /// 当前页已登记分配。
    /// # 返回
    /// 保持原顺序和金额方向的销项、进项裁剪单元。
    /// # 错误
    /// 无；金额越界保留原金额类型行为。
    pub fn links(&self) -> (Vec<SalesInvoiceLink>, Vec<PurchaseInvoiceLink>) {
        let sales = self
            .sales
            .iter()
            .map(|row| SalesInvoiceLink {
                order: row.order.clone(),
                signed: signed_sale(&row.item),
                view: sales_invoice_allocation_view(&row.item),
            })
            .collect();
        let purchase = self
            .purchase
            .iter()
            .map(|row| PurchaseInvoiceLink {
                order: row.order.clone(),
                signed: signed_purchase(&row.item),
                view: purchase_view(&row.item),
            })
            .collect();
        (sales, purchase)
    }
}

/// 当前页进项分配字段保持既有对外 DTO 口径。
fn purchase_view(item: &PurchaseInvoiceAllocation) -> PurchaseInvoiceAllocationView {
    PurchaseInvoiceAllocationView {
        id: item.base.id.clone(),
        invoice_id: item.invoice_id.to_string(),
        allocation_seq: item.allocation_seq,
        allocation_action: item.allocation_action,
        payable_account_id: item.payable_account_id.to_string(),
        allocated_gross_amount: item.allocated_gross_amount,
        allocated_net_amount: item.allocated_net_amount,
        allocated_tax_amount: item.allocated_tax_amount,
        reverses_allocation_id: item.reverses_allocation_id.as_ref().map(ToString::to_string),
    }
}

/// 销项原正反分配的方向金额。
fn signed_sale(item: &SalesInvoiceAllocation) -> Amount {
    match item.allocation_action {
        SalesAction::Apply => item.allocated_gross_amount,
        SalesAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
    }
}

/// 进项原正反分配的方向金额。
fn signed_purchase(item: &PurchaseInvoiceAllocation) -> Amount {
    match item.allocation_action {
        PurchaseAction::Apply => item.allocated_gross_amount,
        PurchaseAction::Reverse => zero_amount().checked_sub(item.allocated_gross_amount),
    }
}

/// 在同一执行器查询最终授权页；复核拍只执行轻量版本分支。
///
/// # 参数
/// 数据库、规范化查询、已解析授权、业务条件、首拍标记和原执行器。
/// # 返回
/// 最终授权页面、计数和完整轻量版本；首拍另包含原流汇总事实。
/// # 错误
/// 持久化、解码或最终匹配超限时拒绝。
pub(in crate::finance::funds_scope) async fn invoice_snapshot(
    db: &Database,
    filter: &InvoiceFilter,
    query: &InvoiceListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    materialize: bool,
    executor: &mut dyn Executor,
) -> Result<InvoiceSnapshot> {
    let sort = sort_document(
        query.paging.sort_by,
        query.paging.sort_dir == SortDir::Asc,
        &["invoice_date", "gross_amount", "net_amount", "created_at"],
    );
    let base = invoice_pipeline(filter, query, authorization, condition);
    let mut pipeline = base.clone();
    pipeline.extend([
        doc! { "$sort": sort.clone() },
        doc! { "$limit": 10000 },
        doc! { "$project": version_projection(materialize) },
    ]);
    let versions =
        aggregate(db.collection::<Document>(<Database as ReceivableExt>::INVOICES), pipeline, executor)
            .await?;
    let mut snapshot = InvoiceSnapshot { versions, ..Default::default() };
    snapshot.ensure_bounded()?;
    if materialize {
        let page = invoice_page(db, base, query, sort, executor).await?;
        snapshot.items = page.items;
        snapshot.total = page.total;
        snapshot.summary = summary_rows(db, &snapshot.versions, executor).await?;
    }
    Ok(snapshot)
}

/// 页面聚合只有有界实体页和计数，完整版本与金额流独立游标读取。
async fn invoice_page(
    db: &Database,
    mut pipeline: Vec<Document>,
    query: &InvoiceListQuery,
    sort: Document,
    executor: &mut dyn Executor,
) -> Result<InvoiceSnapshot> {
    pipeline.push(page_only_facet(query.paging.page, query.paging.page_size, sort, item_projection()));
    let mut result =
        aggregate(db.collection::<Document>(<Database as ReceivableExt>::INVOICES), pipeline, executor)
            .await?;
    result.pop().ok_or_else(|| Error::Internal("发票分页投影缺失".into()))
}

/// 所有来源存在性、授权和用户条件均先于页、计数和汇总。
fn invoice_pipeline(
    filter: &InvoiceFilter,
    query: &InvoiceListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
) -> Vec<Document> {
    let mut pipeline = vec![doc! { "$match": header_filter(filter, condition) }];
    pipeline.push(allocation_lookup(
        <Database as ReceivableExt>::SALES_INVOICE_ALLOCATIONS,
        sales_stages(authorization, condition),
        "_sales",
    ));
    pipeline.push(allocation_lookup(
        <Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS,
        purchase_stages(
            authorization,
            condition.secondary_operator_user_ids.as_deref(),
            condition.org_unit_ids.as_deref(),
        ),
        "_purchase",
    ));
    pipeline.push(doc! { "$match": { "$expr": { "$allElementsTrue": [{ "$map": {
        "input": { "$concatArrays": ["$_sales", "$_purchase"] }, "as": "link", "in": "$$link.source_exists",
    } }] } } });
    pipeline.push(doc! { "$set": {
        "_sales_sources": matched_sources("$_sales"), "_purchase_sources": matched_sources("$_purchase"),
        "_whole": { "$and": [authorization.ledger_read, { "$allElementsTrue": [{ "$map": {
            "input": { "$concatArrays": ["$_sales", "$_purchase"] }, "as": "link", "in": "$$link.matched",
        } }] }] },
    } });
    pipeline.push(doc! { "$match": final_match(query, condition) });
    pipeline
}

/// 登记人直接收窄发票主表，未命中的发票不读取分配与真实来源。
fn header_filter(filter: &InvoiceFilter, condition: &FundsLinkedCondition) -> Document {
    let mut document = filter.to_doc();
    if let Some(ids) = &condition.operator_user_ids {
        document.insert("created_by", doc! { "$in": ids });
    }
    document
}

/// 来源去重和稳定键排序对应原 BTreeSet，不能只绑定当前页来源。
fn matched_sources(links: &str) -> Document {
    doc! { "$sortArray": { "input": { "$setUnion": [{ "$map": {
        "input": { "$filter": { "input": links, "as": "link", "cond": "$$link.matched" } },
        "as": "link", "in": { "key": "$$link.order", "version": "$$link._source.version",
            "owner": { "$ifNull": ["$$link._source.owner_user_id", null] } },
    } }, []] }, "sortBy": { "key": 1 } } }
}

/// 双方向字段按原合同匹配，来源销售单不扩展到未登记关系。
fn final_match(query: &InvoiceListQuery, condition: &FundsLinkedCondition) -> Document {
    let visible = doc! { "$or": [{ "_whole": true }, { "_sales_sources.0": { "$exists": true } },
    { "_purchase_sources.0": { "$exists": true } }] };
    let mut conditions = vec![visible];
    if condition.owner_user_ids.is_some() {
        conditions.push(doc! { "_sales_sources.0": { "$exists": true } });
    }
    if condition.secondary_operator_user_ids.is_some() {
        conditions.push(doc! { "_purchase_sources.0": { "$exists": true } });
    }
    if condition.org_unit_ids.is_some() {
        conditions.push(doc! { "$or": [{ "_sales_sources.0": { "$exists": true } },
        { "_purchase_sources.0": { "$exists": true } }] });
    }
    if let Some(id) = &query.sales_order_id {
        conditions.push(doc! { "$or": [
            { "invoice_direction": "sales", "_sales.order": id },
            { "invoice_direction": "purchase", "_sales": { "$size": 0 } },
        ] });
    }
    if let Some(id) = &query.receivable_account_id {
        conditions.push(doc! { "_sales.receivable_account_id": id.to_string() });
    }
    doc! { "$and": conditions }
}

/// 当前页保留原票面字段；完整分配仅解码当前页，再执行原金额裁剪。
fn item_projection() -> Document {
    let mut projection = doc! { "_id": 0, "sales": "$_sales", "purchase": "$_purchase", "whole": "$_whole",
    "sales_matched": "$_sales_sources.key", "purchase_matched": "$_purchase_sources.key" };
    for field in [
        "id",
        "status",
        "current_revision_id",
        "created_by",
        "updated_by",
        "invoice_direction",
        "invoice_kind",
        "party_id",
        "invoice_code",
        "invoice_no",
        "invoice_date",
        "gross_amount",
        "net_amount",
        "tax_amount",
        "rounding_adjustment_amount",
        "rounding_reason",
        "sales_invoice_request_id",
        "original_invoice_id",
        "version",
        "created_at",
    ] {
        projection.insert(field, 1);
    }
    projection
}

/// 完整结果指纹材料保留原哈希字段和来源顺序。
fn version_projection(materialize: bool) -> Document {
    let mut projection = doc! { "_id": 0, "id": 1, "version": 1, "sales_versions": "$_sales_sources.version",
    "purchase_versions": "$_purchase_sources.version" };
    if materialize {
        projection.insert(
            "summary",
            doc! { "whole": "$_whole", "gross_amount": "$gross_amount",
            "sales_sources": "$_sales_sources", "purchase_sources": "$_purchase_sources",
            "all_sales_sources": all_sources("$_sales"), "all_purchase_sources": all_sources("$_purchase") },
        );
    }
    projection
}

/// 全部存在来源只提供原负责人覆盖口径，不扩大匹配份额或版本材料。
fn all_sources(links: &str) -> Document {
    doc! { "$map": { "input": links, "as": "link", "in": {
        "key": "$$link.order", "owner": { "$ifNull": ["$$link._source.owner_user_id", null] },
    } } }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use super::*;

    /// 主表登记人筛选与领域已有条件求交，空经办人集合仍为零匹配条件。
    #[test]
    fn header_operator_filter_keeps_original_constraints_and_empty_set() {
        let filter = InvoiceFilter { keyword_ids: Some(vec!["invoice".into()]), ..Default::default() };
        let original = filter.to_doc();
        assert_eq!(header_filter(&filter, &FundsLinkedCondition::default()), original);
        let condition = FundsLinkedCondition {
            operator_user_ids: Some(vec!["registrar".into(), "second".into()]),
            ..Default::default()
        };
        let mut expected = original.clone();
        expected.insert("created_by", doc! { "$in": ["registrar", "second"] });
        assert_eq!(header_filter(&filter, &condition), expected);
        let condition = FundsLinkedCondition { operator_user_ids: Some(vec![]), ..Default::default() };
        let mut expected = original;
        expected.insert("created_by", doc! { "$in": [] });
        assert_eq!(header_filter(&filter, &condition), expected);
    }

    /// 轻量完整指纹与旧逐票协议一致，并检测非当前页来源责任版本变化。
    #[test]
    fn invoice_versions_preserve_legacy_hash_and_outside_page_changes() {
        let mut rows: Vec<InvoiceVersion> = serde_json::from_value(serde_json::json!([
            {"id":"second","version":7,"sales_versions":[2,4],"purchase_versions":[3]},
            {"id":"first","version":8,"sales_versions":[],"purchase_versions":[9]}
        ]))
        .unwrap();
        let mut legacy = DefaultHasher::new();
        for row in &rows {
            row.id.hash(&mut legacy);
            row.version.hash(&mut legacy);
            for version in row.sales_versions.iter().chain(&row.purchase_versions) {
                version.hash(&mut legacy);
            }
        }
        let mut actual = DefaultHasher::new();
        for row in &rows {
            row.hash_into(&mut actual);
        }
        assert_eq!(actual.finish(), legacy.finish());
        rows[1].purchase_versions[0] += 1;
        let mut current = DefaultHasher::new();
        for row in &rows {
            row.hash_into(&mut current);
        }
        assert_ne!(actual.finish(), current.finish());
    }

    /// 保护应用于最终已授权集合，9999 完整成功，10000 整体拒绝。
    #[test]
    fn invoice_final_set_rejects_sentinel_without_truncating_success() {
        let mut snapshot = InvoiceSnapshot {
            versions: (0..9999)
                .map(|index| InvoiceVersion {
                    id: format!("{index}"),
                    version: 1,
                    sales_versions: vec![],
                    purchase_versions: vec![],
                    summary: None,
                })
                .collect(),
            ..Default::default()
        };
        snapshot.ensure_bounded().unwrap();
        snapshot.versions.push(InvoiceVersion {
            id: "sentinel".into(),
            version: 1,
            sales_versions: vec![],
            purchase_versions: vec![],
            summary: None,
        });
        assert!(matches!(snapshot.ensure_bounded(), Err(Error::ValidationError(message))
            if message == "发票查询超过上限，请收窄组织或负责人条件"));
    }

    /// 空页仍使用数据库最终计数及全匹配份额，部分授权票面不能返回。
    #[test]
    fn invoice_empty_page_keeps_count_summary_and_partial_null() {
        let snapshot: InvoiceSnapshot = serde_json::from_value(serde_json::json!({
            "items":[],"total":[{"count":3}],"versions":[],"summary":[
                {"whole":false,"gross_amount":"100","shares":[
                    {"id":"allocation","order":"order","owner":"seller","allocated_gross_amount":"60","allocation_action":"apply"}
                ]}
            ]
        })).unwrap();
        assert!(snapshot.items.is_empty());
        assert_eq!(snapshot.total(), 3);
        let summary = snapshot.summary("v").unwrap();
        assert_eq!(summary.grouped[0].visible_share, "60".parse().unwrap());
        assert_eq!(summary.whole_total, None);
        assert!(summary.permission_limited);
    }
}
