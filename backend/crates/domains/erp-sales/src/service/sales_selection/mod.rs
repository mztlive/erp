//! 销售选品用例编排。

pub mod access;
mod copy;
mod create;
mod lifecycle;
mod mapper;
mod prepare;
mod prepare_queue;
mod pricing;
mod public_access;
mod publish;
mod query;
mod session;

use std::sync::Arc;

pub use access::SelectionAccess;
use application_core::AuditActor;
use erp_core::ids::SalesSelectionBookletId;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::Permission;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::entity::sales_selection::IdempotencyOperation;

/// 幂等写入上下文对象。
#[derive(Debug)]
pub struct IdempotencyStoreInput<'a, T> {
    /// 操作域。
    pub operation: IdempotencyOperation,
    /// 作用域。
    pub scope_id: &'a str,
    /// 幂等键。
    pub key: &'a str,
    /// 请求哈希。
    pub hash: &'a str,
    /// 待持久化的结果视图。
    pub result: &'a T,
    /// 令牌版本。
    pub token_version: Option<u32>,
    /// 关联选品册。
    pub booklet_id: Option<SalesSelectionBookletId>,
}

/// 销售选品服务.
#[derive(Clone)]
pub struct SalesSelectionService {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SalesSelectionService {
    /// 装配选品用例的数据库与共享授权服务，创建服务时不读取业务数据。
    ///
    /// 输入约定：
    /// - `authorization`：已完整装配的进程共享授权服务。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    async fn authorize_booklet(
        &self,
        actor: &AuditActor,
        id: &str,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.require(actor, permission, AccessTarget::record(id)?, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{FactValue, ObjectFacts, RelatedObjects, RelationKey};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::{customer, sales_selection};
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    fn object(resource: &str, id: &str) -> ObjectFacts {
        ObjectFacts {
            resource: resource.into(),
            id: id.into(),
            version: "1".into(),
            owner_id: FactValue::Known(Some("owner".into())),
            org_unit_id: FactValue::Known(Some("org".into())),
            collaborators: FactValue::Known(BTreeSet::new()),
            history_participants: FactValue::Known(BTreeSet::new()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn shared_selection_service_uses_each_commands_actor() {
        let mut book = object("sales_selection_booklet", "book");
        book.relations.insert(
            RelationKey::parse("customer").unwrap(),
            RelatedObjects {
                expected_ids: ["customer".into()].into(),
                objects: vec![object("customer", "customer")],
            },
        );
        let authorization = fixed_authorization(
            &[sales_selection::BOOKLET_PUBLISH, sales_selection::ADMIN_IMAGE, customer::DETAIL],
            ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            vec![book],
            vec![],
        )
        .unwrap();
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("selection_actor");
        let service = SalesSelectionService::new(db, authorization);
        let owner = AuditActor::new("owner".into(), "owner".into(), AccountKind::Admin);
        let outsider = AuditActor::new("outsider".into(), "outsider".into(), AccountKind::Admin);

        service
            .authorize_booklet(
                &owner,
                "book",
                &sales_selection::BOOKLET_PUBLISH,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();

        let error = service
            .clone()
            .authorize_booklet(
                &outsider,
                "book",
                &sales_selection::BOOKLET_PUBLISH,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, Error::Forbidden(message) if message == "操作包含不可访问的对象或目标归属"));

        service
            .authorize_booklet(
                &owner,
                "book",
                &sales_selection::BOOKLET_PUBLISH,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap();
    }
}
