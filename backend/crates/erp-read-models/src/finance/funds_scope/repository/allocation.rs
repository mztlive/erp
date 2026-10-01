//! 进项分配先按真实来源授权，页面与计数同口径，版本和金额独立游标读取。

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_finance::dto::payable::PurchaseInvoiceAllocationListQuery;
use erp_finance::entity::payable::{PayableSourceType, PurchaseInvoiceAllocation};
use erp_finance::repository::{PayableExt, PurchaseInvoiceAllocationFilter, ReceivableExt};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, QueryFilter};
use serde::Deserialize;

use super::super::rows::{FundsSummaryView, build_summary, zero_amount};
use super::super::{FundsAuthorization, FundsLinkedCondition};
use super::invoice_sources::purchase_stages;
use super::invoice_summary::InvoiceShare;
use super::{aggregate, page_only_facet};
use crate::{Error, Result};

/// 一拍最终授权分配；第二拍只装载相同口径的完整轻量版本。
#[derive(Debug, Default, Deserialize)]
pub(in crate::finance::funds_scope) struct AllocationSnapshot {
    #[serde(default)]
    pub items: Vec<AllocationPageRow>,
    #[serde(default)]
    total: Vec<CountRow>,
    #[serde(default)]
    pub versions: Vec<AllocationVersion>,
    #[serde(default)]
    summary: Vec<InvoiceShare>,
}

/// 当前页原分配实体和已登记票号，不装载其它分配。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct AllocationPageRow {
    pub item: PurchaseInvoiceAllocation,
    pub invoice_no: Option<String>,
}

/// 最终授权口径的计数，空匹配保持零。
#[derive(Debug, Deserialize)]
struct CountRow {
    count: u64,
}

/// 当前分配与真实来源的完整身份版本，用于原分页指纹协议。
#[derive(Debug, Deserialize)]
pub(in crate::finance::funds_scope) struct AllocationVersion {
    pub id: String,
    pub version: u64,
    pub order: String,
    pub source_version: u64,
}

impl AllocationVersion {
    /// 保留原分配主键、分配版本、带类型来源关联键及来源版本哈希。
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
        Some(self.order.as_str()).hash(fingerprint);
        self.source_version.hash(fingerprint);
    }
}

impl AllocationSnapshot {
    /// 最终已授权匹配超过原 9999 条保护时整体拒绝，禁止截断。
    ///
    /// # 参数
    /// 最终授权快照。
    /// # 返回
    /// 未达到原查询上限时返回成功。
    /// # 错误
    /// 最终匹配达到 10000 条时返回原查询保护错误。
    pub fn ensure_bounded(&self) -> Result<()> {
        if self.versions.len() >= 10_000 {
            return Err(Error::ValidationError("收票查询超过上限，请收窄组织或负责人条件".into()));
        }
        Ok(())
    }

    /// 读取同一最终条件的数据库总数，空集合为零。
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

    /// 全部已授权分配沿原正反事实归并，结算来源无负责人时进入未分配分区。
    ///
    /// # 参数
    /// 首拍金额事实及完整范围版本。
    /// # 返回
    /// 原定点算法归并的授权份额与整单金额资格。
    /// # 错误
    /// 必要事实损坏时拒绝；金额越界保留原金额类型的溢出行为。
    pub fn summary(&self, version: &str) -> Result<FundsSummaryView> {
        let mut triples = Vec::new();
        let mut owners = HashMap::new();
        let mut total = zero_amount();
        for share in &self.summary {
            let signed = share.signed();
            triples.push((share.id.clone(), signed, Some(share.order.clone())));
            total = total.checked_add(signed);
            if let Some(owner) = &share.owner
                && (share.source_type != Some(PayableSourceType::PurchaseOrder) || !owner.trim().is_empty())
            {
                owners.insert(share.order.clone(), owner.clone());
            }
        }
        build_summary(&triples, &owners, Some(total), version, false)
    }
}

/// 来源授权与人员筛选在页面、完整版本与汇总分支之前完成。
///
/// # 参数
/// 数据库、规范化查询、已解析授权、业务条件、首拍标记和原执行器。
/// # 返回
/// 最终授权分配页、计数和完整版本；首拍同时读取必要金额流。
/// # 错误
/// 持久化、解码或最终匹配超限时拒绝。
pub(in crate::finance::funds_scope) async fn allocation_snapshot(
    db: &Database,
    query: &PurchaseInvoiceAllocationListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
    materialize: bool,
    executor: &mut dyn Executor,
) -> Result<AllocationSnapshot> {
    let base = allocation_pipeline(query, authorization, condition);
    let mut pipeline = base.clone();
    let sort = doc! { "created_at": -1, "id": -1 };
    let projection = doc! { "_id": 0, "id": 1, "version": 1, "order": 1,
    "source_version": "$_source.version" };
    pipeline.extend([
        doc! { "$sort": sort.clone() },
        doc! { "$limit": 10000 },
        doc! { "$project": projection },
    ]);
    let collection = db.collection::<Document>(<Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS);
    let versions = aggregate(collection.clone(), pipeline, executor).await?;
    let mut snapshot = AllocationSnapshot { versions, ..Default::default() };
    snapshot.ensure_bounded()?;
    if materialize {
        let page = allocation_page(db, base.clone(), query, sort.clone(), executor).await?;
        snapshot.items = page.items;
        snapshot.total = page.total;
        let mut summary = base;
        summary.extend([
            doc! { "$sort": sort },
            doc! { "$limit": 10000 },
            doc! { "$project": summary_projection() },
        ]);
        snapshot.summary = aggregate(collection, summary, executor).await?;
    }
    Ok(snapshot)
}

/// 最终授权后的页面和计数共用一个分支，完整版本与金额独立游标读取。
async fn allocation_page(
    db: &Database,
    mut pipeline: Vec<Document>,
    query: &PurchaseInvoiceAllocationListQuery,
    sort: Document,
    executor: &mut dyn Executor,
) -> Result<AllocationSnapshot> {
    pipeline.push(page_only_facet(query.paging.page, query.paging.page_size, sort, page_projection()));
    let mut result = aggregate(
        db.collection::<Document>(<Database as PayableExt>::PURCHASE_INVOICE_ALLOCATIONS),
        pipeline,
        executor,
    )
    .await?;
    result.pop().ok_or_else(|| Error::Internal("进项分配分页投影缺失".into()))
}

/// 先定位子账再核对来源存在与实际范围，登记人仅是额外业务筛选。
fn allocation_pipeline(
    query: &PurchaseInvoiceAllocationListQuery,
    authorization: &FundsAuthorization,
    condition: &FundsLinkedCondition,
) -> Vec<Document> {
    let filter = PurchaseInvoiceAllocationFilter {
        payable_account_id: query.payable_account_id.clone(),
        ..Default::default()
    };
    let mut pipeline = vec![doc! { "$match": filter.to_doc() }];
    pipeline.extend(purchase_stages(
        authorization,
        condition.owner_user_ids.as_deref(),
        condition.org_unit_ids.as_deref(),
    ));
    pipeline.push(doc! { "$match": { "source_exists": true, "matched": true } });
    pipeline.extend(invoice_header_stages());
    if let Some(ids) = &condition.operator_user_ids {
        pipeline.push(doc! { "$match": { "_invoice.created_by": { "$in": ids } } });
    }
    pipeline
}

/// 票号与收票经办人取当前登记发票，缺失发票保留原空票号语义。
fn invoice_header_stages() -> Vec<Document> {
    vec![
        doc! { "$lookup": { "from": <Database as ReceivableExt>::INVOICES,
        "let": { "invoice_id": "$invoice_id" }, "pipeline": [
            { "$match": { "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$expr": { "$eq": ["$id", "$$invoice_id"] } } },
            { "$project": { "_id": 0, "id": 1, "created_by": 1, "invoice_no": 1 } },
        ], "as": "_invoice" } },
        doc! { "$set": { "_invoice": { "$arrayElemAt": ["$_invoice", 0] } } },
    ]
}

/// 页面只携带当前分配实体与票号，保留原解码校验。
fn page_projection() -> Document {
    doc! { "_id": 0, "invoice_no": { "$ifNull": ["$_invoice.invoice_no", null] }, "item": {
        "id": "$id", "version": "$version", "created_at": "$created_at", "updated_at": "$updated_at",
        "deleted_at": "$deleted_at", "invoice_id": "$invoice_id", "payable_account_id": "$payable_account_id",
        "allocation_seq": "$allocation_seq", "allocation_action": "$allocation_action",
        "allocated_gross_amount": "$allocated_gross_amount", "allocated_net_amount": "$allocated_net_amount",
        "allocated_tax_amount": "$allocated_tax_amount", "reverses_allocation_id": "$reverses_allocation_id",
    } }
}

/// 完整金额流只保留原求和必须字段，不传输全量分配实体。
fn summary_projection() -> Document {
    doc! { "_id": 0, "id": 1, "order": 1, "owner": { "$ifNull": ["$_source.owner_user_id", null] },
    "allocated_gross_amount": 1, "allocation_action": 1, "source_type": "$_account.source_type" }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;

    use super::*;

    /// 轻量材料保留旧 Option 来源键的哈希形态，并能检测非当前页责任交接。
    #[test]
    fn allocation_versions_keep_typed_source_and_detect_changes() {
        let mut row = AllocationVersion {
            id: "allocation".into(),
            version: 2,
            order: "supplier_settlement_statement:source".into(),
            source_version: 3,
        };
        let mut legacy = DefaultHasher::new();
        row.id.hash(&mut legacy);
        row.version.hash(&mut legacy);
        Some(row.order.clone()).hash(&mut legacy);
        row.source_version.hash(&mut legacy);
        let mut actual = DefaultHasher::new();
        row.hash_into(&mut actual);
        assert_eq!(actual.finish(), legacy.finish());
        row.source_version += 1;
        let mut changed = DefaultHasher::new();
        row.hash_into(&mut changed);
        assert_ne!(changed.finish(), actual.finish());
    }

    /// 空页面仍保留数据库总数和完整金额流，尾页不能把汇总归零。
    #[test]
    fn empty_page_retains_total_and_signed_summary() {
        let snapshot: AllocationSnapshot = serde_json::from_value(serde_json::json!({
            "items":[], "total":[{"count":2}], "versions":[], "summary":[
                {"id":"one","order":"po","owner":"buyer","allocated_gross_amount":"80","allocation_action":"apply"},
                {"id":"two","order":"po","owner":"buyer","allocated_gross_amount":"20","allocation_action":"reverse"}
            ]
        })).unwrap();
        assert_eq!(snapshot.total(), 2);
        let summary = snapshot.summary("version").unwrap();
        assert_eq!(summary.whole_total, Some("60".parse().unwrap()));
        assert_eq!(summary.grouped[0].visible_share, "60".parse().unwrap());
    }
}
