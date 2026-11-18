//! 单条供给资料：先证明供给范围，再装配商品、供应商与当前条款。

use application_core::AuditActor;
use erp_core::ids::SupplierOfferingId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_permissions::supplier_offering as supplier_offering_permissions;
use persistence_core::{Executor, Transactional};

use super::{SupplierOfferingReadService, SupplierOfferingView, page_view, repository_page};
use crate::supplier_center::repository::offering::SupplierOfferingListQuery;
use crate::{Error, Result};

impl SupplierOfferingReadService {
    /// 处理供应商供给时，读取授权范围内一条供给的完整当前资料。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    ///
    /// # 返回
    /// 返回与列表同口径的当前资料，成本脱敏由 HTTP 边界执行。
    ///
    /// # 错误
    /// 缺少详情资格、授权阶段失效或仓储失败时拒绝；对象不存在或不在范围内返回 NotFound。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<SupplierOfferingView> {
        let service = Self::new(self.db.clone(), self.access.clone(), self.procurement.clone());
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.detail_snapshot(&id, &actor, executor).await })
            })
            .await
    }

    // 处理供应商供给时，详情资格、根读取、展示补齐与原凭证交付检查共享同一快照。
    async fn detail_snapshot(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOfferingView> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("暂时无法查看供给详情，请从供给列表重新打开".into()));
        }

        let context = self.access.resolve(actor, &supplier_offering_permissions::DETAIL, executor).await?;
        let query = SupplierOfferingListQuery {
            offering_ids: Some(vec![SupplierOfferingId::new(id)]),
            page_size: 1,
            scope: Some(context.clone()),
            ..Default::default()
        };
        let bundle = repository_page(&self.db, &query, executor).await?;
        let mut view = page_view(bundle, &query)
            .items
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotFound("供给不存在或无权查看".into()))?;

        let names = self.db.accounts().names_by_ids(&[view.maintainer_user_id.clone()], executor).await?;
        view.maintainer_user_name = names.get(&view.maintainer_user_id).cloned();

        context.validate(executor).await?;
        Ok(view)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::{unavailable_authorization, unbound_authorization};
    use erp_supply::OfferingAccess;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::supplier_center::offering::MongoOfferingProcurementOwners;

    #[tokio::test]
    async fn offering_detail_rejects_non_snapshot_before_authorization_or_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("offering_detail");
        let service = SupplierOfferingReadService::new(
            db.clone(),
            OfferingAccess::new(db.clone(), unavailable_authorization()),
            MongoOfferingProcurementOwners::shared(db),
        );
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        let error =
            service.detail_snapshot("offering", &actor, &mut ReadExecutor::default()).await.unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(reason) if reason == "暂时无法查看供给详情，请从供给列表重新打开")
        );
    }

    #[tokio::test]
    async fn offering_detail_rejects_missing_query_binding_or_qualification_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("offering_detail");
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for (authorization, missing_binding) in
            [(unbound_authorization(), true), (unavailable_authorization(), false)]
        {
            let service = SupplierOfferingReadService::new(
                db.clone(),
                OfferingAccess::new(db.clone(), authorization),
                MongoOfferingProcurementOwners::shared(db.clone()),
            );
            let error = service
                .detail_snapshot("offering", &actor, &mut SnapshotExecutor::default())
                .await
                .unwrap_err();
            assert!(match (error, missing_binding) {
                (Error::ValidationError(reason), true) => reason == "缺少查询 Mapping 绑定",
                (Error::Internal(reason), false) => reason == "测试授权设施不可用",
                _ => false,
            });
        }
    }
}
