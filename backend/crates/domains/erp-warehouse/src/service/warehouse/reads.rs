//! 仓库经办候选读取的授权事务步骤。
use super::*;

impl WarehouseService {
    // 在原读取快照校验筛选并检查仓库修订读取资格，供仓储随后分页；非法输入、缺资格或非快照时拒绝。
    pub(super) async fn revision_filter(
        &self,
        actor: &AuditActor,
        params: &WarehouseRevisionListParams,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseRevisionFilter> {
        params.validate()?;
        let filter = params.normalized()?.into_filter();
        self.authorization
            .require(actor, &warehouse::REVISION_LIST, AccessTarget::Operation, executor)
            .await?;
        Ok(filter)
    }

    // 在同一快照校验预警策略筛选并确认读取资格，再交给仓储；筛选非法、缺资格或非快照时拒绝。
    pub(super) async fn sku_policy_filter(
        &self,
        actor: &AuditActor,
        params: &WarehouseSkuPolicyListParams,
        executor: &mut dyn Executor,
    ) -> Result<WarehouseSkuPolicyFilter> {
        params.validate()?;
        let filter = params.normalized()?.into_filter();
        self.authorization
            .require(actor, &warehouse::SKU_POLICY_LIST, AccessTarget::Operation, executor)
            .await?;
        Ok(filter)
    }

    // 在同一快照检查调用入口的仓库操作资格，再读取具备履约资格的候选。
    pub(super) async fn handler_options_snapshot(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WarehouseFulfillmentHandlerOptionView>> {
        self.authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
        Ok(handler_option_views(self.identity.admin_handler_identities(executor).await?))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::fixed_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedFingerprintPort};

    #[derive(Default)]
    struct Identities(AtomicUsize);

    #[async_trait]
    impl IdentityFactPort for Identities {
        async fn handler_identity(
            &self,
            _: &str,
            _: &mut dyn Executor,
        ) -> Result<Option<HandlerIdentityFact>> {
            panic!("候选读取不得逐一扫描账号")
        }

        async fn admin_handler_identities(
            &self,
            executor: &mut dyn Executor,
        ) -> Result<Vec<HandlerIdentityFact>> {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(vec![HandlerIdentityFact {
                user_id: "handler".into(),
                display_name: "仓库经办人".into(),
                account: "handler-login".into(),
                can_login: true,
                inbound_eligible: true,
                outbound_eligible: false,
            }])
        }
    }

    async fn service(permissions: &[Permission], identities: Arc<Identities>) -> WarehouseService {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("warehouse_read_authorization");
        let authorization = fixed_authorization(
            permissions,
            ResolvedScope { role_clauses: vec![], user_limit: None },
            vec![],
            vec![],
        )
        .unwrap();
        WarehouseService::new(
            db,
            identities,
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedFingerprintPort),
            authorization,
        )
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn handler_read_requires_the_permission_for_its_operation() {
        for (granted, denied) in
            [(warehouse::CREATE, warehouse::UPDATE), (warehouse::UPDATE, warehouse::CREATE)]
        {
            let identities = Arc::new(Identities::default());
            let service = service(std::slice::from_ref(&granted), identities.clone()).await;
            let mut executor = SnapshotExecutor::default();
            let rows = service.handler_options_snapshot(&actor(), &granted, &mut executor).await.unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].user_id, "handler");
            assert!(rows[0].inbound_eligible);
            assert!(!rows[0].outbound_eligible);
            let denied = service.handler_options_snapshot(&actor(), &denied, &mut executor).await;
            assert!(matches!(denied, Err(Error::Forbidden(_))));
            assert_eq!(identities.0.load(Ordering::SeqCst), 1);
        }
    }

    #[tokio::test]
    async fn revision_and_policy_reads_allow_permissions_and_prepare_production_filters() {
        let service =
            service(&[warehouse::REVISION_LIST, warehouse::SKU_POLICY_LIST], Arc::new(Identities::default()))
                .await;
        let mut executor = SnapshotExecutor::default();

        let params =
            serde_json::from_value(serde_json::json!({"name":"  仓库名称  ","page":2,"page_size":7}))
                .unwrap();
        let revisions = service.revision_filter(&actor(), &params, &mut executor).await.unwrap();
        assert_eq!(revisions.name.as_deref(), Some("仓库名称"));
        assert_eq!(revisions.page, 2);
        assert_eq!(revisions.page_size, 7);

        let params = serde_json::from_value(
            serde_json::json!({"warehouse_id":"warehouse-1","sku_id":"sku-1","page":3,"page_size":5}),
        )
        .unwrap();
        let policies = service.sku_policy_filter(&actor(), &params, &mut executor).await.unwrap();
        assert_eq!(policies.warehouse_id.as_ref().map(|id| id.as_ref()), Some("warehouse-1"));
        assert_eq!(policies.sku_id.as_ref().map(|id| id.as_ref()), Some("sku-1"));
        assert_eq!(policies.page, 3);
        assert_eq!(policies.page_size, 5);
    }

    #[tokio::test]
    async fn reads_reject_missing_permission_before_candidate_or_business_reads() {
        let identities = Arc::new(Identities::default());
        let service = service(&[], identities.clone()).await;
        let revisions = serde_json::from_value(serde_json::json!({})).unwrap();
        let policies = serde_json::from_value(serde_json::json!({})).unwrap();
        let mut executor = SnapshotExecutor::default();

        let errors = [
            service
                .handler_options_snapshot(&actor(), &warehouse::UPDATE, &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .warehouse_revision_list_snapshot(&actor(), &revisions, &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .warehouse_sku_policy_list_snapshot(&actor(), &policies, &mut executor)
                .await
                .map(|_| ())
                .unwrap_err(),
        ];

        for error in errors {
            assert!(matches!(error, Error::Forbidden(_)));
        }
        assert_eq!(identities.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn reads_reject_no_transaction_despite_valid_permissions() {
        let identities = Arc::new(Identities::default());
        let service = service(
            &[warehouse::CREATE, warehouse::UPDATE, warehouse::REVISION_LIST, warehouse::SKU_POLICY_LIST],
            identities.clone(),
        )
        .await;
        let revisions = serde_json::from_value(serde_json::json!({})).unwrap();
        let policies = serde_json::from_value(serde_json::json!({})).unwrap();

        let errors = [
            service
                .handler_options_snapshot(&actor(), &warehouse::CREATE, &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .handler_options_snapshot(&actor(), &warehouse::UPDATE, &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .warehouse_revision_list_snapshot(&actor(), &revisions, &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
            service
                .warehouse_sku_policy_list_snapshot(&actor(), &policies, &mut NoTransaction)
                .await
                .map(|_| ())
                .unwrap_err(),
        ];

        for error in errors {
            assert!(matches!(error, Error::ValidationError(message) if message.contains("必须使用快照事务")));
        }
        assert_eq!(identities.0.load(Ordering::SeqCst), 0);
    }
}
