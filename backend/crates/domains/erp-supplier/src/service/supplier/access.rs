//! 供应商对象读取与写入范围；维护人与业务组织分别解释。

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::{AccessTarget, AuthorizationService, OrganizationDirectoryService};
use erp_permissions::{Permission, supplier};
use mongodb::Database;
use persistence_core::Executor;

use crate::authorization::CompiledScope;
use crate::entity::supplier::SupplierAccount;
use crate::error::{Error, Result};

/// 供应商对象访问范围；列表、详情、导出和写命令复用同一解析。
#[derive(Clone)]
pub struct SupplierAccess {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl SupplierAccess {
    /// 接入供应商数据库与共享身份服务，供列表授权和对象判定复用；构造时不读取账号权限。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 用原查询执行器请求身份域编译固定供应商动作的查询条件。组织范围使用对象业务组织，能力负责人不扩大
    /// 范围；账号、动作、模型或阶段无效时拒绝。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<CompiledScope> {
        ensure_permission(permission)?;
        let query = self.authorization.query(actor, permission, executor).await?;
        let compiled = Arc::new(query);
        Ok(CompiledScope {
            scope_version: compiled.version(),
            compiled,
            authorization: self.authorization.clone(),
        })
    }

    /// 详情或写入前以业务入口指定的供应商动作，在原事务中验证真实对象；拒绝跨资源权限、失效账号或范围外
    /// 对象。
    pub async fn require_with(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        supplier_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_permission(permission)?;
        Ok(self
            .authorization
            .require(actor, permission, AccessTarget::record(supplier_id)?, executor)
            .await?)
    }

    /// 交接落库前使用 `HANDOVER` 同时授权持久化供应商和拟保存的维护人、业务组织；两端任一不可访问或事实
    /// 读不到均拒绝。
    pub async fn require_owner_change(
        &self,
        actor: &AuditActor,
        candidate: &SupplierAccount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .authorization
            .require(
                actor,
                &supplier::HANDOVER,
                AccessTarget::OwnershipChange {
                    id: candidate.base.id.clone(),
                    candidate: ObjectFacts {
                        resource: "supplier".into(),
                        owner_id: FactValue::Known(Some(candidate.maintainer_user_id.clone())),
                        org_unit_id: FactValue::Known(Some(candidate.business_org_unit_id.clone())),
                        ..Default::default()
                    },
                },
                executor,
            )
            .await?)
    }

    /// 在原事务读取拟指定维护人的有效主属组织，再按导入创建权限授权该候选。缺主属组织或无创建范围均拒绝，
    /// 不以操作人兜底。
    pub async fn require_create(
        &self,
        actor: &AuditActor,
        maintainer_user_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let owner_org = OrganizationDirectoryService::new(self.db.clone())
            .own_org(maintainer_user_id, Instant::now(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("请先维护负责人的有效主属组织".into()))?;

        self.authorization
            .require(
                actor,
                &supplier::IMPORT_SUPPLIER_IMPORT,
                AccessTarget::Candidate(ObjectFacts {
                    resource: "supplier".into(),
                    owner_id: FactValue::Known(Some(maintainer_user_id.into())),
                    org_unit_id: FactValue::Known(Some(owner_org.clone())),
                    ..Default::default()
                }),
                executor,
            )
            .await?;
        Ok(owner_org)
    }
}

// 内置权限直接接入时仍校验拥有资源，禁止跨域凭证替换。
fn ensure_permission(permission: &Permission) -> Result<()> {
    if permission.resource() != "supplier" {
        return Err(Error::ValidationError("权限资源与对象访问器不符".into()));
    }
    Ok(())
}

#[cfg(test)]
mod permission_tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::sales_order;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::authorization::bindings;

    async fn access(granted: bool) -> SupplierAccess {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("supplier_read_authorization");

        let permissions = if granted { vec![supplier::LIST, supplier::DETAIL] } else { vec![] };
        let mapping = bindings(db.clone()).unwrap().1;
        let authorization = fixed_authorization(
            &permissions,
            ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            [("visible-supplier", "user"), ("outside-supplier", "other")]
                .into_iter()
                .map(|(id, owner)| ObjectFacts {
                    resource: "supplier".into(),
                    id: id.into(),
                    version: "v1".into(),
                    owner_id: FactValue::Known(Some(owner.into())),
                    org_unit_id: FactValue::Known(None),
                    ..Default::default()
                })
                .collect(),
            vec![(supplier::LIST, mapping)],
        )
        .unwrap();
        SupplierAccess::new(db, authorization)
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn reads_allow_real_owner_with_production_mapping_and_original_snapshot() {
        let access = access(true).await;
        let mut executor = SnapshotExecutor::default();
        let scope = access.resolve(&actor(), &supplier::LIST, &mut executor).await.unwrap();
        assert!(!scope.stages().is_empty());
        scope.ensure_current(&mut executor).unwrap();
        scope.validate(&mut executor).await.unwrap();

        access.require_with(&actor(), &supplier::DETAIL, "visible-supplier", &mut executor).await.unwrap();

        let outside = access
            .require_with(&actor(), &supplier::DETAIL, "outside-supplier", &mut executor)
            .await
            .unwrap_err();
        assert!(matches!(outside, Error::Forbidden(_)));

        let absent = access
            .require_with(&actor(), &supplier::DETAIL, "absent-supplier", &mut executor)
            .await
            .unwrap_err();
        assert!(matches!(absent, Error::NotFound(_)));
    }

    #[tokio::test]
    async fn reads_reject_missing_permissions_before_object_loading() {
        let access = access(false).await;
        let mut executor = SnapshotExecutor::default();
        assert!(matches!(
            access.resolve(&actor(), &supplier::LIST, &mut executor).await,
            Err(Error::Forbidden(_))
        ));
        assert!(matches!(
            access.require_with(&actor(), &supplier::DETAIL, "unread-supplier", &mut executor).await,
            Err(Error::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn reads_reject_no_transaction_despite_valid_permissions() {
        let access = access(true).await;
        assert!(
            matches!(access.resolve(&actor(), &supplier::LIST, &mut NoTransaction).await, Err(Error::ValidationError(message)) if message == "授权要求可核验的事务或读取阶段")
        );
        assert!(
            matches!(access.require_with(&actor(), &supplier::DETAIL, "visible-supplier", &mut NoTransaction).await, Err(Error::ValidationError(message)) if message == "授权要求可核验的事务或读取阶段")
        );
    }

    #[test]
    fn accessor_accepts_its_constant_and_rejects_foreign_resource() {
        assert!(ensure_permission(&supplier::DETAIL).is_ok());
        assert!(matches!(ensure_permission(&sales_order::DETAIL), Err(Error::ValidationError(_))));
    }
}
