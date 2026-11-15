//! 应付关联保持真实来源类型，结算来源不得退化为空责任事实。

use std::collections::{BTreeSet, HashMap};

use erp_finance::entity::payable::PayableSourceType;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use super::super::authorization::{FundsAccess, FundsAuthorization};
use super::super::rows::LinkedPurchaseFact;
use super::aggregate;
use super::source_access::SourceQuery;
use crate::Result;

const SETTLEMENT_PREFIX: &str = "supplier_settlement_statement:";

/// 内部关联键包含真实来源类型，避免相同主键在采购与结算域间串权。
///
/// # 参数
/// * `source` - 应付来源类型。
/// * `id` - 来源主键。
///
/// # 返回
/// 采购单返回原主键；供应商结算返回带 `supplier_settlement_statement:` 前缀的关联键。
///
/// # 错误
/// 不返回错误。
pub(in crate::finance::funds_scope) fn source_key(source: PayableSourceType, id: &str) -> String {
    match source {
        PayableSourceType::PurchaseOrder => id.to_owned(),
        PayableSourceType::SupplierSettlement => format!("{SETTLEMENT_PREFIX}{id}"),
    }
}

impl FundsAccess {
    /// 有界批量装载结算当前责任，加入与采购共用的关联事实集合。
    ///
    /// # 参数
    /// * `ids` - 带结算前缀的来源关联键；其他键被忽略。
    /// * `executor` - 调用方事务。
    ///
    /// # 返回
    /// 返回存在的结算来源事实；缺失来源不补空事实。
    ///
    /// # 错误
    /// 结算单读取失败时返回对应错误。
    pub(in crate::finance::funds_scope) async fn settlement_fact_map(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, LinkedPurchaseFact>> {
        let ids = ids
            .iter()
            .filter_map(|id| id.strip_prefix(SETTLEMENT_PREFIX).map(str::to_owned))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut facts = HashMap::new();
        for chunk in ids.chunks(500) {
            for statement in
                self.db.supplier_settlement_statements().find_statements_by_ids(chunk, executor).await?
            {
                facts.insert(
                    source_key(PayableSourceType::SupplierSettlement, &statement.base.id),
                    LinkedPurchaseFact {
                        owner_user_id: Some(statement.prepared_by),
                        business_org_unit_id: statement.business_org_unit_id,
                        version: statement.base.version,
                        document_no: statement.statement_no,
                    },
                );
            }
        }
        Ok(facts)
    }
}

impl FundsAccess {
    /// 按当前请求的真实采购来源检查指定记录授权，结算来源仍由所属范围判定。
    pub(in crate::finance::funds_scope) async fn payable_targets(
        &self,
        authorization: &FundsAuthorization,
        facts: &HashMap<String, LinkedPurchaseFact>,
        executor: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        let mut allowed = BTreeSet::new();
        for id in facts.keys() {
            if id.starts_with(SETTLEMENT_PREFIX) {
                if let Some(scope) = &authorization.settlement
                    && self.settlement_source_allowed(id, scope, executor).await?
                {
                    allowed.insert(id.clone());
                }
            } else if let Some(scope) = &authorization.purchase_scope
                && !scope.ids_in(&self.db, std::slice::from_ref(id), executor).await?.is_empty()
            {
                allowed.insert(id.clone());
            }
        }
        Ok(allowed)
    }
}

impl FundsAccess {
    async fn settlement_source_allowed(
        &self,
        id: &str,
        scope: &SourceQuery,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let Some(id) = id.strip_prefix(SETTLEMENT_PREFIX) else {
            return Ok(false);
        };
        scope.validate(executor).await?;
        let mut pipeline = vec![doc! { "$match": { "id": id } }];
        pipeline.extend_from_slice(scope.stages());
        pipeline.push(doc! { "$project": { "id": 1 } });
        let rows: Vec<Document> = aggregate(
            self.db.collection(<Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS),
            pipeline,
            executor,
        )
        .await?;
        scope.validate(executor).await?;
        Ok(!rows.is_empty())
    }
}
