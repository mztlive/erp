//! 结算列表按动作解析范围，并收窄三角色筛选。

use application_core::{AuditActor, normalized_text};
use async_trait::async_trait;
use erp_core::ids::SupplierAccountId;
use erp_party::PartyExt;
use erp_permissions::supplier_settlement;
use erp_supplier::SupplierExt;
use erp_supplier::repository::prelude::*;
use erp_supply::SettlementAccess;
use erp_supply::authorization::CompiledScope;
use erp_supply::dto::supplier_settlement::{
    SupplierSettlementStatementListParams, SupplierSettlementStatementListView,
};
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemType;
use erp_workflow::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierSettlementReadService;
use crate::Result;

impl SupplierSettlementReadService {
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
        let db = self.db.clone();
        let data_scope = SettlementAccess::new(self.db.clone(), self.identity.clone());
        let actor = actor.clone();
        let params = params.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let data_scope = data_scope.clone();
                let actor = actor.clone();
                let params = params.clone();
                let db = db.clone();
                Box::pin(async move {
                    load_list(
                        &SettlementList { db: &db, data_scope, actor: &actor, params: &params },
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

#[async_trait]
trait ListReader: Sync {
    type Scope: Send;
    type Output;
    async fn scope(&self, executor: &mut dyn Executor) -> Result<Self::Scope>;
    async fn filters(&self, executor: &mut dyn Executor) -> Result<(Vec<SupplierAccountId>, Vec<String>)>;
    async fn page(
        &self,
        scope: Self::Scope,
        filters: (Vec<SupplierAccountId>, Vec<String>),
        executor: &mut dyn Executor,
    ) -> Result<Self::Output>;
}

/// 授权通过后才读取跨域筛选事实，三个阶段复用原执行器。
async fn load_list<R: ListReader>(reader: &R, executor: &mut dyn Executor) -> Result<R::Output> {
    let scope = reader.scope(executor).await?;
    let filters = reader.filters(executor).await?;
    reader.page(scope, filters, executor).await
}

struct SettlementList<'a> {
    db: &'a Database,
    data_scope: SettlementAccess,
    actor: &'a AuditActor,
    params: &'a SupplierSettlementStatementListParams,
}

#[async_trait]
impl ListReader for SettlementList<'_> {
    type Scope = CompiledScope;
    type Output = SupplierSettlementStatementListView;

    async fn scope(&self, executor: &mut dyn Executor) -> Result<Self::Scope> {
        Ok(self.data_scope.resolve(self.actor, &supplier_settlement::STATEMENT_LIST, executor).await?)
    }

    async fn filters(&self, executor: &mut dyn Executor) -> Result<(Vec<SupplierAccountId>, Vec<String>)> {
        let suppliers = keyword_supplier_ids(self.db, self.params.q.as_deref(), executor).await?;
        let open_ids = open_review_statement_ids(self.db, self.params, executor).await?;
        Ok((suppliers, open_ids))
    }

    async fn page(
        &self,
        scope: Self::Scope,
        (suppliers, open_ids): (Vec<SupplierAccountId>, Vec<String>),
        executor: &mut dyn Executor,
    ) -> Result<Self::Output> {
        Ok(SupplierSettlementService::new(self.db.clone())
            .with_data_scope(self.data_scope.clone())
            .statement_list_scoped(self.params, suppliers, &scope, open_ids, executor)
            .await?)
    }
}

async fn keyword_supplier_ids(
    db: &Database,
    q: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierAccountId>> {
    let Some(q) = normalized_text(q) else {
        return Ok(Vec::new());
    };
    let parties = db.party().matching_current_party_ids_by_name(&q, executor).await?;
    Ok(db.supplier_accounts().matching_ids_by_parties(&parties, executor).await?)
}

async fn open_review_statement_ids(
    db: &Database,
    params: &SupplierSettlementStatementListParams,
    executor: &mut dyn Executor,
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
            executor,
        )
        .await?;
    Ok(items.into_iter().map(|item| item.business_object_id).collect())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    struct Reader {
        reject: Option<&'static str>,
        events: Mutex<Vec<(&'static str, usize)>>,
    }

    impl Reader {
        fn record(&self, stage: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.events.lock().unwrap().push((stage, executor as *mut dyn Executor as *mut () as usize));
            if self.reject == Some(stage) {
                return Err(Error::Forbidden(stage.into()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl ListReader for Reader {
        type Scope = &'static str;
        type Output = (Self::Scope, Vec<SupplierAccountId>, Vec<String>);

        async fn scope(&self, executor: &mut dyn Executor) -> Result<Self::Scope> {
            self.record("authorization", executor)?;
            Ok("authorized-scope")
        }
        async fn filters(
            &self,
            executor: &mut dyn Executor,
        ) -> Result<(Vec<SupplierAccountId>, Vec<String>)> {
            self.record("filters", executor)?;
            Ok((vec![SupplierAccountId::new("supplier-1")], vec!["statement-1".into()]))
        }
        async fn page(
            &self,
            scope: Self::Scope,
            (suppliers, statements): (Vec<SupplierAccountId>, Vec<String>),
            executor: &mut dyn Executor,
        ) -> Result<Self::Output> {
            self.record("page", executor)?;
            Ok((scope, suppliers, statements))
        }
    }

    #[tokio::test]
    async fn list_authorizes_before_filters_and_preserves_original_executor() {
        let reader = Reader { reject: None, events: Mutex::new(Vec::new()) };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;
        assert_eq!(
            load_list(&reader, &mut executor).await.unwrap(),
            ("authorized-scope", vec![SupplierAccountId::new("supplier-1")], vec!["statement-1".into()])
        );
        assert_eq!(
            *reader.events.lock().unwrap(),
            vec![("authorization", expected), ("filters", expected), ("page", expected)]
        );
    }

    #[tokio::test]
    async fn rejected_authorization_reads_no_filters_and_filter_failure_reads_no_page() {
        for (stage, expected) in [("authorization", 1), ("filters", 2)] {
            let reader = Reader { reject: Some(stage), events: Mutex::new(Vec::new()) };
            let error = load_list(&reader, &mut SnapshotExecutor::default()).await.unwrap_err();
            assert!(matches!(error, Error::Forbidden(message) if message == stage));
            assert_eq!(reader.events.lock().unwrap().len(), expected);
        }
    }
}
