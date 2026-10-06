//! 仅沿正式选源与采购当前指针读取供给影响；范围由消费方证明后下推。

use std::collections::BTreeMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{PurchaseOrderId, SupplierOfferingId};
use futures_util::TryStreamExt;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
use serde::Deserialize;

use super::scope::PurchaseReadScope;
use super::{
    PURCHASE_ORDER_REVISION_LINES, PURCHASE_ORDER_REVISIONS, PURCHASE_ORDER_SUBMISSION_LINES,
    PURCHASE_ORDER_SUBMISSIONS, PURCHASE_ORDERS, PurchaseOrderDomainRepository,
};
use crate::entity::purchase_order::{PurchaseOfferingSource, PurchaseOrder, PurchaseOrderStatus};
use crate::{Error, Result};

/// 当前采购行的精确选源，未关联的历史行不会出现在此结果中。
#[derive(Debug, Clone, Deserialize)]
pub struct PurchaseOfferingUsageLine {
    pub line_id: String,
    pub source: PurchaseOfferingSource,
    pub purchase_order_revision_id: Option<String>,
    pub purchase_order_submission_id: Option<String>,
}

/// 内部授权所需真实采购对象与有限选源行；HTTP不得直接透传该领域对象。
#[derive(Debug, Clone)]
pub struct PurchaseOfferingUsageFact {
    pub order: PurchaseOrder,
    pub lines: Vec<PurchaseOfferingUsageLine>,
}

#[derive(Deserialize)]
struct UsageRow {
    order: PurchaseOrder,
    #[serde(flatten)]
    line: PurchaseOfferingUsageLine,
}

impl PurchaseOrderDomainRepository<'_> {
    /// 按正式供给身份反查权限范围内采购的当前有效选源。
    ///
    /// # 参数
    /// * `offering_id` - 明确供给主键，禁止供应商及SKU猜测。
    /// * `scope` - 消费方已证明的采购读取范围，下推到采购主表。
    /// * `limit` - 1至10000的结果行上限，超过上限整体拒绝。
    /// * `executor` - 调用方读取执行器。
    /// # 返回
    /// 返回非完成、非作废采购的当前修订行，尚未生效时取当前提交行。
    /// # 错误
    /// 上限非法、结果超限、当前采购责任缺失或仓储失败时拒绝。
    pub async fn find_current_offering_usage(
        &self,
        offering_id: &SupplierOfferingId,
        scope: &PurchaseReadScope,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOfferingUsageFact>> {
        if limit == 0 || limit > 10_000 {
            return Err(Error::ValidationError("采购供给影响查询上限必须在1至10000之间".into()));
        }
        if scope.is_empty() {
            return Ok(Vec::new());
        }
        let mut rows = self.offering_usage_rows(offering_id, scope, true, limit, executor).await?;
        rows.extend(self.offering_usage_rows(offering_id, scope, false, limit, executor).await?);
        if rows.len() > usize::try_from(limit).unwrap_or(10_000) {
            return Err(Error::ValidationError("供给关联采购超过查询上限，请收窄采购范围".into()));
        }
        let mut grouped = BTreeMap::<String, PurchaseOfferingUsageFact>::new();
        for row in rows {
            row.order.current_owner_user_id()?;
            grouped
                .entry(row.order.base.id.clone())
                .or_insert_with(|| PurchaseOfferingUsageFact { order: row.order, lines: Vec::new() })
                .lines
                .push(row.line);
        }
        Ok(grouped.into_values().collect())
    }

    /// 读取已授权采购当前指针中的明确供给选源，缺失的历史关联保持未知。
    ///
    /// # 参数
    /// * `order_id` - 消费方已独立授权的采购身份。
    /// * `executor` - 同一读取执行器。
    /// # 返回
    /// 返回当前有效版本或未生效当前提交中已记录的精确选源。
    /// # 错误
    /// 仓储读取失败时返回错误；未知采购或已终止采购返回空集合。
    pub async fn current_offering_sources_for_order(
        &self,
        order_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseOfferingSource>> {
        let Some(order) =
            self.find_orders_by_ids(&[order_id.to_string()], executor).await?.into_iter().next()
        else {
            return Ok(Vec::new());
        };
        if matches!(order.stable.status, PurchaseOrderStatus::Completed | PurchaseOrderStatus::Voided) {
            return Ok(Vec::new());
        }
        if let Some(id) = order.stable.current_revision_id {
            return Ok(self
                .list_revision_lines(&id.into(), executor)
                .await?
                .into_iter()
                .filter_map(|line| line.supplier_offering_source)
                .collect());
        }
        if let Some(id) = order.current_submission_id {
            return Ok(self
                .list_submission_lines(&id.into(), executor)
                .await?
                .into_iter()
                .filter_map(|line| line.supplier_offering_source)
                .collect());
        }
        Ok(Vec::new())
    }

    /// 聚合仅返回已匹配当前指针和授权范围的有限行。
    async fn offering_usage_rows(
        &self,
        id: &SupplierOfferingId,
        scope: &PurchaseReadScope,
        revision: bool,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<UsageRow>> {
        let name = if revision { PURCHASE_ORDER_REVISION_LINES } else { PURCHASE_ORDER_SUBMISSION_LINES };
        let collection = self.db.collection::<Document>(name);
        let pipeline = usage_pipeline(id, scope, revision, limit);
        match executor.session() {
            Some(session) => collection
                .aggregate(pipeline)
                .with_type::<UsageRow>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect()
                .await
                .map_err(Into::into),
            None => collection
                .aggregate(pipeline)
                .with_type::<UsageRow>()
                .await?
                .try_collect()
                .await
                .map_err(Into::into),
        }
    }
}

/// 明确来源先使用索引定位，再在采购主表内合并范围与当前指针条件。
fn usage_pipeline(
    id: &SupplierOfferingId,
    scope: &PurchaseReadScope,
    revision: bool,
    limit: u32,
) -> Vec<Document> {
    let (headers, line_pointer, order_pointer) = if revision {
        (PURCHASE_ORDER_REVISIONS, "purchase_order_revision_id", "$order.current_revision_id")
    } else {
        (PURCHASE_ORDER_SUBMISSIONS, "purchase_order_submission_id", "$order.current_submission_id")
    };
    let mut pointer_checks = vec![doc! { "$eq": [order_pointer, "$header.id"] }];
    if !revision {
        pointer_checks.push(doc! { "$eq": [{ "$ifNull": ["$order.current_revision_id", null] }, null] });
    }
    vec![
        doc! { "$match": { "supplier_offering_source.supplier_offering_id": id.to_string(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON } },
        doc! { "$lookup": { "from": headers, "localField": line_pointer, "foreignField": "id", "as": "header" } },
        doc! { "$unwind": "$header" },
        order_lookup(scope),
        doc! { "$unwind": "$order" },
        doc! { "$match": { "$expr": { "$and": pointer_checks } } },
        doc! { "$project": { "_id": 0, "order": 1, "line_id": "$id", "source": "$supplier_offering_source", "purchase_order_revision_id": 1, "purchase_order_submission_id": 1 } },
        doc! { "$sort": { "order.id": 1, "line_id": 1 } },
        doc! { "$limit": i64::from(limit) + 1 },
    ]
}

/// 授权条件留在采购对象字段内，不能被供给业务筛选覆盖。
fn order_lookup(scope: &PurchaseReadScope) -> Document {
    doc! { "$lookup": { "from": PURCHASE_ORDERS, "let": { "order_id": "$header.purchase_order_id" }, "pipeline": [
        { "$match": { "$and": [
            { "$expr": { "$eq": ["$id", "$$order_id"] } },
            { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "status": { "$nin": ["COMPLETED", "VOIDED"] } },
            scope.document(),
        ] } },
    ], "as": "order" } }
}

#[cfg(test)]
mod tests {
    use super::super::scope::PurchaseScopeClause;
    use super::*;

    #[test]
    fn usage_query_keeps_explicit_offering_current_pointer_scope_and_sentinel_limit() {
        let scope = PurchaseReadScope {
            roles: vec![PurchaseScopeClause { owner_user_id: Some("buyer".into()), ..Default::default() }],
            ..Default::default()
        };
        let revision = usage_pipeline(&SupplierOfferingId::new("offering-exact"), &scope, true, 100);
        assert_eq!(
            revision[0]
                .get_document("$match")
                .unwrap()
                .get_str("supplier_offering_source.supplier_offering_id")
                .unwrap(),
            "offering-exact"
        );
        assert_eq!(revision.last().unwrap().get_i64("$limit").unwrap(), 101);
        assert!(revision[3].to_string().contains("owner_user_id"));
        assert!(revision[5].to_string().contains("current_revision_id"));
        let submission = usage_pipeline(&SupplierOfferingId::new("offering-exact"), &scope, false, 100);
        assert!(submission[5].to_string().contains("current_submission_id"));
        assert!(submission[5].to_string().contains("current_revision_id"));
    }
}
