//! 供应商履约订单跟进人交接：范围重验、CAS 与责任字段。

use application_core::AuditActor;
use erp_permissions::supplier_fulfillment;
use mongodb::Database;
use persistence_core::Executor;
use validator::Validate;

use super::{FulfillmentOrderAccess, SupplierFulfillmentService};
use crate::dto::supplier_fulfillment::{HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView};
use crate::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use crate::error::{Error, Result};
use crate::repository::SupplierFulfillmentExt;

impl SupplierFulfillmentService {
    /// 在调用方事务内交接跟进人；供组合层与幂等收据共用执行器。
    ///
    /// # 参数
    /// * `id` - 订单稳定 ID
    /// * `req` - 已校验交接请求
    /// * `actor` - 已认证操作人
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回交接后视图。
    ///
    /// # 错误
    /// 版本、范围或目标非法时拒绝。
    pub async fn apply_fulfillment_handover(
        &self,
        id: &str,
        req: &HandoverFulfillmentOrderRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<HandoverFulfillmentOrderView> {
        req.validate()?;
        persist_handover(&self.db, &self.access(), id, req, actor, executor).await
    }
}

/// 在调用方事务内重验范围、CAS 并写入跟进人。
async fn persist_handover(
    db: &Database,
    access: &FulfillmentOrderAccess,
    id: &str,
    req: &HandoverFulfillmentOrderRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<HandoverFulfillmentOrderView> {
    let mut order = access.require_order(actor, &supplier_fulfillment::ORDER_HANDOVER, id, executor).await?;
    if order.base.version != req.expected_version {
        return Err(Error::ConflictError("供应商履约订单责任或版本已变化，请刷新后重试".into()));
    }

    authorize_handover(access, &mut order, req, actor, executor).await?;
    db.supplier_fulfillment_orders().update(&mut order, executor).await?;
    Ok(handover_view(&order, Vec::new()))
}

/// 先应用领域交接规则，再用实际拟保存归属重验原记录与目标。
async fn authorize_handover(
    access: &FulfillmentOrderAccess,
    order: &mut SupplierFulfillmentOrder,
    req: &HandoverFulfillmentOrderRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    order.handover(req.target_user_id.clone(), req.target_org_unit_id.clone()).map_err(Error::from)?;
    access.require_owner_change(actor, order, executor).await
}

/// 构造交接响应。
///
/// # 参数
/// * `order` - 已写回跟进人的履约订单。
/// * `transferred_work_item_ids` - 已移交的工作项身份。
///
/// # 返回
/// 返回订单、跟进人、业务组织、版本和工作项身份。
///
/// # 错误
/// 不返回错误。
pub(crate) fn handover_view(
    order: &SupplierFulfillmentOrder,
    transferred_work_item_ids: Vec<String>,
) -> HandoverFulfillmentOrderView {
    HandoverFulfillmentOrderView {
        order_id: order.base.id.clone(),
        follow_up_user_id: order.follow_up_user_id.clone(),
        business_org_unit_id: order.business_org_unit_id.clone(),
        version: order.base.version,
        transferred_work_item_ids,
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::authorization::{FactValue, ObjectFacts};
    use erp_identity::testing::fixed_authorization;
    use mongodb::Client;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::entity::supplier_fulfillment::SupplierFulfillmentOrderData;

    fn order() -> SupplierFulfillmentOrder {
        let data = SupplierFulfillmentOrderData::submitting(
            "order-number",
            SupplierAccountId::new("supplier"),
            SupplierApiConnectionId::new("connection"),
            1,
            Instant::from_unix_secs(1),
            "encrypted-address",
            "address-fingerprint",
        )
        .with_follow_up("old-owner", "allowed-org");
        let mut order =
            SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new("order"), data).unwrap();
        order.base = BaseModel { id: "order".into(), ..BaseModel::fake() };
        order
    }

    fn request(org: Option<&str>) -> HandoverFulfillmentOrderRequest {
        HandoverFulfillmentOrderRequest {
            target_user_id: " new-owner ".into(),
            target_org_unit_id: org.map(str::to_string),
            reason: "岗位交接".into(),
            expected_version: 1,
            idempotency_key: "key".into(),
            transfer_open_exception_tasks: false,
        }
    }

    fn original(org: &str) -> ObjectFacts {
        ObjectFacts {
            resource: "supplier_fulfillment_order".into(),
            id: "order".into(),
            version: "v1".into(),
            owner_id: FactValue::Known(Some("old-owner".into())),
            org_unit_id: FactValue::Known(Some(org.into())),
            ..Default::default()
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "operator".into(), AccountKind::Admin)
    }

    async fn access(original: ObjectFacts, clause: ScopeClause) -> FulfillmentOrderAccess {
        let service = fixed_authorization(
            &[supplier_fulfillment::ORDER_HANDOVER],
            ResolvedScope { role_clauses: vec![clause], user_limit: None },
            vec![original],
            Vec::new(),
        )
        .unwrap();
        let client = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        FulfillmentOrderAccess::new(client.database("handover_unit_test"), service)
    }

    #[tokio::test]
    async fn handover_uses_normalized_entity_and_preserves_company_policy() {
        let access =
            access(original("allowed-org"), ScopeClause { company: true, ..Default::default() }).await;
        let mut changed = order();
        authorize_handover(
            &access,
            &mut changed,
            &request(Some(" new-org ")),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();
        assert_eq!(changed.follow_up_user_id, "new-owner");
        assert_eq!(changed.business_org_unit_id, "new-org");

        let mut retained = order();
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
            &mut order(),
            &request(None),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap();

        let destination_denied = authorize_handover(
            &allowed,
            &mut order(),
            &request(Some("outside-org")),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(destination_denied, Err(Error::Forbidden(_))));

        let denied = access(original("outside-org"), restricted()).await;
        let original_denied = authorize_handover(
            &denied,
            &mut order(),
            &request(None),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(original_denied, Err(Error::Forbidden(_))));

        let no_snapshot =
            authorize_handover(&allowed, &mut order(), &request(None), &actor(), &mut NoTransaction).await;
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
            let mut order = order();
            order.follow_up_user_id = original_owner.into();
            let mut req = request(Some("new-org"));
            req.target_user_id = target_owner.into();
            let result =
                authorize_handover(&access, &mut order, &req, &actor(), &mut SnapshotExecutor::default())
                    .await;
            if permitted {
                result.unwrap();
            } else {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            }
        }
    }
}
