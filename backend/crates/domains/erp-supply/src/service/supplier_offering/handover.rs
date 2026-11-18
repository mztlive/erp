//! 供给维护人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;
use erp_permissions::supplier_offering;
use mongodb::Database;
use persistence_core::Executor;

use super::{OfferingAccess, SupplierOfferingService};
use crate::dto::{HandoverSupplierOfferingRequest, HandoverSupplierOfferingView};
use crate::entity::supplier_offering::SupplierOffering;
use crate::error::{Error, Result};
use crate::repository::SupplierOfferingExt;

impl SupplierOfferingService {
    /// 在调用方事务内交接；供组合层与幂等收据共用执行器。
    ///
    /// # 参数
    /// * `id` - 供给稳定 ID
    /// * `req` - 已校验交接请求
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回交接前实体与交接后视图，供调用方记录审计事实。
    ///
    /// # 错误
    /// 版本、范围或目标非法时拒绝。
    pub async fn apply_offering_handover(
        &self,
        id: &str,
        req: &HandoverSupplierOfferingRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierOffering, HandoverSupplierOfferingView)> {
        persist_handover(&self.db, &self.access(), id, req, actor, executor).await
    }
}

/// 在调用方事务内重验范围、CAS 并写入维护人。
///
/// # 参数
/// * `db` - 供给数据库
/// * `access` - 供给范围访问器
/// * `id` - 供给 ID
/// * `req` - 已校验交接请求
/// * `actor` - 操作人
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回交接前实体与交接后视图，供调用方记录审计事实。
///
/// # 错误
/// 版本、范围或目标非法时整事务回滚。
async fn persist_handover(
    db: &Database,
    access: &OfferingAccess,
    id: &str,
    req: &HandoverSupplierOfferingRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<(SupplierOffering, HandoverSupplierOfferingView)> {
    let mut offering = access.require_offering(actor, &supplier_offering::REVISE, id, executor).await?;
    if offering.base.version != req.expected_version {
        return Err(Error::ConflictError("供给责任或版本已变化，请刷新后重试".into()));
    }

    let before = offering.clone();
    authorize_handover(access, &mut offering, req, actor, executor).await?;
    db.supplier_offerings().update(&mut offering, executor).await?;
    Ok((before, handover_view(&offering)))
}

/// 先应用领域交接规则，再用实际拟保存归属重验原记录与目标。
async fn authorize_handover(
    access: &OfferingAccess,
    offering: &mut SupplierOffering,
    req: &HandoverSupplierOfferingRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    offering
        .handover(req.target_user_id.clone(), req.target_org_unit_id.clone(), actor.id())
        .map_err(Error::from)?;
    access.require_owner_change(actor, offering, executor).await
}

/// 构造交接响应。
fn handover_view(offering: &SupplierOffering) -> HandoverSupplierOfferingView {
    HandoverSupplierOfferingView {
        offering_id: offering.base.id.clone(),
        maintainer_user_id: offering.maintainer_user_id.clone(),
        business_org_unit_id: offering.business_org_unit_id.clone(),
        version: offering.base.version,
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::ids::{SkuId, SupplierAccountId, SupplierOfferingId};
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{FactValue, ObjectFacts};
    use erp_identity::testing::fixed_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::supplier_offering::{OfferingSourceType, SupplierOfferingData};

    fn offering() -> SupplierOffering {
        let mut offering = SupplierOffering::new(
            SupplierOfferingId::new("offering"),
            SupplierOfferingData {
                sku_id: SkuId::new("sku"),
                supplier_id: SupplierAccountId::new("supplier"),
                supplier_product_code: None,
                supplier_sku_code: "supplier-sku".into(),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
                maintainer_user_id: "old-owner".into(),
                business_org_unit_id: "allowed-org".into(),
            },
            "creator",
        )
        .unwrap();
        offering.base = BaseModel { id: "offering".into(), ..BaseModel::fake() };
        offering
    }

    fn request(org: Option<&str>) -> HandoverSupplierOfferingRequest {
        HandoverSupplierOfferingRequest {
            target_user_id: " new-owner ".into(),
            target_org_unit_id: org.map(str::to_string),
            reason: "岗位交接".into(),
            expected_version: 1,
            idempotency_key: "key".into(),
        }
    }

    fn original(org: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "supplier_offering".into(),
            id: "offering".into(),
            version: "v1".into(),
            owner_id: FactValue::Known(Some("old-owner".into())),
            org_unit_id: FactValue::Known(Some(org.into())),
            ..Default::default()
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "operator".into(), AccountKind::Admin)
    }

    async fn access(original: ObjectFacts, clause: ScopeClause) -> OfferingAccess {
        let service = fixed_authorization(
            &[supplier_offering::REVISE],
            ResolvedScope { role_clauses: vec![clause], user_limit: None },
            vec![original],
            Vec::new(),
        )
        .unwrap();
        let client = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        OfferingAccess::new(client.database("handover_unit_test"), service)
    }

    #[tokio::test]
    async fn handover_uses_normalized_entity_and_preserves_company_policy() {
        let access =
            access(original("allowed-org"), ScopeClause { company: true, ..Default::default() }).await;
        let mut changed = offering();
        authorize_handover(
            &access,
            &mut changed,
            &request(Some(" new-org ")),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
        assert_eq!(changed.maintainer_user_id, "new-owner");
        assert_eq!(changed.business_org_unit_id, "new-org");

        let mut retained = offering();
        authorize_handover(
            &access,
            &mut retained,
            &request(None),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
        assert_eq!(retained.business_org_unit_id, "allowed-org");
    }

    #[tokio::test]
    async fn handover_checks_original_and_destination_in_the_same_snapshot() {
        let restricted = || ScopeClause { org_unit_ids: ["allowed-org".into()].into(), ..Default::default() };
        let allowed = access(original("allowed-org"), restricted()).await;
        authorize_handover(
            &allowed,
            &mut offering(),
            &request(None),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

        let destination_denied = authorize_handover(
            &allowed,
            &mut offering(),
            &request(Some("outside-org")),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(destination_denied, Err(Error::Forbidden(_))));

        let denied = access(original("outside-org"), restricted()).await;
        let original_denied = authorize_handover(
            &denied,
            &mut offering(),
            &request(None),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(original_denied, Err(Error::Forbidden(_))));

        let no_snapshot =
            authorize_handover(&allowed, &mut offering(), &request(None), &actor(), &mut NoTransaction).await;
        assert!(matches!(no_snapshot, Err(Error::ValidationError(_))));
    }

    #[tokio::test]
    async fn handover_checks_original_and_destination_owners() {
        for (original_owner, target_owner, permitted) in
            [("user", "user", true), ("user", "other", false), ("other", "user", false)]
        {
            let mut facts = original("allowed-org");
            facts.owner_id = FactValue::Known(Some(original_owner.into()));
            let access = access(facts, ScopeClause { self_owned: true, ..Default::default() }).await;
            let mut offering = offering();
            offering.maintainer_user_id = original_owner.into();
            let mut req = request(Some("new-org"));
            req.target_user_id = target_owner.into();
            let result =
                authorize_handover(&access, &mut offering, &req, &actor(), &mut SnapshotExecutor::default())
                    .await;
            if permitted {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }
}
