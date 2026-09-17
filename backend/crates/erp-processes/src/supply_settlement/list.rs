//! 结算列表按动作解析范围，并收窄三角色筛选。

use application_core::AuditActor;
use erp_core::ids::SupplierAccountId;
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use erp_supply::dto::supplier_settlement::{
    SupplierSettlementStatementListParams, SupplierSettlementStatementListView,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use persistence_core::{NoTransaction, Transactional};

use super::SupplierSettlementProcess;
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 按详情动作重验结算单可见性，不返回整单视图。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `id` - 结算单 ID
    ///
    /// # 返回
    /// 对象在详情范围内时返回 `Ok(())`。
    ///
    /// # 错误
    /// 不可见对象返回 NotFound。
    pub async fn require_detail(&self, actor: &AuditActor, id: &str) -> Result<()> {
        self.domain().access().require_statement(actor, "detail", id, &mut NoTransaction).await?;
        Ok(())
    }

    /// 分页查询授权范围内的结算单。
    ///
    /// # 参数
    /// * `params` - 查询参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回带范围版本的列表；空范围返回空集。
    ///
    /// # 错误
    /// 第二页缺版本或范围指纹变化时返回 `DATA_SCOPE_CHANGED`。
    ///
    /// # 关键业务约束
    /// 对账负责人授权对象；差异/复核筛选只收窄。confirm 不经本入口。
    pub async fn statement_list(
        &self,
        params: &SupplierSettlementStatementListParams,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementListView> {
        let suppliers = keyword_supplier_ids(&self.db, params.q.as_deref()).await?;
        let open_ids = open_review_statement_ids(&self.db, params).await?;
        let db = self.db.clone();
        let data_scope = self.data_scope.clone();
        let actor = actor.clone();
        let params = params.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let data_scope = data_scope.clone();
                let actor = actor.clone();
                let params = params.clone();
                let suppliers = suppliers.clone();
                let open_ids = open_ids.clone();
                let db = db.clone();
                Box::pin(async move {
                    erp_supply::service::supplier_settlement::SupplierSettlementService::new(db)
                        .with_data_scope(data_scope)
                        .statement_list_scoped(&params, suppliers, &actor, open_ids, executor)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }
}

async fn keyword_supplier_ids(db: &mongodb::Database, q: Option<&str>) -> Result<Vec<SupplierAccountId>> {
    let Some(q) = application_core::normalized_text(q) else {
        return Ok(Vec::new());
    };
    let parties = db.party().matching_current_party_ids_by_name(&q, &mut NoTransaction).await?;
    Ok(db.supplier_accounts().matching_ids_by_parties(&parties, &mut NoTransaction).await?)
}

async fn open_review_statement_ids(
    db: &mongodb::Database,
    params: &SupplierSettlementStatementListParams,
) -> Result<Vec<String>> {
    let Some(ids) = params.handler_user_ids.as_ref() else {
        return Ok(Vec::new());
    };
    let items = db
        .work_items()
        .list_open_by_type_owners(
            WorkItemType::SupplierSettlementReview,
            "supplier_settlement_statement",
            ids.as_slice(),
            &mut NoTransaction,
        )
        .await?;
    Ok(items.into_iter().map(|item| item.business_object_id).collect())
}
