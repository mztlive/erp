//! 下单幂等读取、候选授权和意图写入共用创建入口的原快照。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_identity::AccessTarget;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_integration::entity::integration_ops::{InboxMessage, InboxMessageStatus};
use erp_integration::repository::IntegrationOpsExt;
use erp_permissions::supplier_fulfillment;
use erp_supply::dto::supplier_fulfillment::PlaceFulfillmentOrderRequest;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::repository::prelude::*;
use erp_supply::service::supplier_fulfillment::place::persist_place_facts;
use persistence_core::Executor;

use super::super::follow_up::resolve_follow_up;
use super::{SupplierFulfillmentProcess, build_action_message};
use crate::Result;

/// 原订单只回读；新意图只有提交成功后才允许向供应商派发。
pub(super) enum PlaceIntent {
    Existing(Box<SupplierFulfillmentOrder>),
    Prepared(Box<PlaceDispatch>),
}

/// 已通过授权并在创建事务中完整落库的供应商请求。
pub(super) struct PlaceDispatch {
    pub(super) order: SupplierFulfillmentOrder,
    pub(super) action: SupplierOrderAction,
    pub(super) message: InboxMessage,
    pub(super) connection: SupplierApiConnection,
}

impl SupplierFulfillmentProcess {
    /// 在原快照中检查幂等结果或授权新候选，再写入完整下单意图。
    /// # 参数
    /// req、actor 为当前请求和真实操作人，executor 为创建入口的事务执行器。
    /// # 返回
    /// 已授权的原订单或等待事务提交后派发的新意图。
    /// # 错误
    /// 当前资格不足、目标越权、来源无效、版本冲突或写入失败时拒绝。
    pub(super) async fn prepare_place_intent(
        &self,
        req: &PlaceFulfillmentOrderRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<PlaceIntent> {
        if let Some(order) = self
            .db
            .supplier_fulfillment_orders()
            .find_by_fulfillment_order_no(&req.fulfillment_order_no, executor)
            .await?
        {
            self.identity
                .rbac
                .require(
                    actor,
                    &supplier_fulfillment::ORDER_SUBMIT,
                    AccessTarget::record(&order.base.id)?,
                    executor,
                )
                .await?;
            self.ensure_legacy_order(&order.base.id, executor).await?;
            return Ok(PlaceIntent::Existing(Box::new(order)));
        }
        let (connection, offerings) = self.domain().ensure_placeable(req, executor).await?;
        let follow_up = resolve_follow_up(&self.db, &req.supplier_id, executor).await?;
        let (order, items, action) =
            self.domain().build_place_facts(req, &offerings, follow_up.user_id, follow_up.org_unit_id)?;
        self.identity
            .rbac
            .require(actor, &supplier_fulfillment::ORDER_SUBMIT, place_candidate(&order), executor)
            .await?;
        let message = build_action_message(&action, &connection, InboxMessageStatus::Received)?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.submit",
            "supplier_fulfillment_order",
            order.base.id.clone(),
        )?;
        persist_place_facts(&self.db, &order, &items, &action, executor).await?;
        self.db.inbox_messages().create(&message, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(PlaceIntent::Prepared(Box::new(PlaceDispatch { order, action, message, connection })))
    }
}

/// 候选责任只取服务端解析的正式跟进人与主属组织，不携带已有记录版本。
fn place_candidate(order: &SupplierFulfillmentOrder) -> AccessTarget {
    AccessTarget::Candidate(ObjectFacts {
        resource: supplier_fulfillment::ORDER_SUBMIT.resource().into(),
        id: order.base.id.clone(),
        owner_id: FactValue::Known(Some(order.follow_up_user_id.clone())),
        org_unit_id: FactValue::Known(Some(order.business_org_unit_id.clone())),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentOrderId};
    use erp_identity::Error as IdentityError;
    use erp_identity::testing::unavailable_authorization;
    use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrderData;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 创建候选必须先满足快照边界，再保留真实授权设施的错误，不能降级为功能资格查询。
    #[tokio::test]
    async fn place_candidate_requires_snapshot_and_propagates_authorization_failure() {
        let order = SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierFulfillmentOrderData::submitting(
                "order-no-1",
                SupplierAccountId::new("supplier-1"),
                SupplierApiConnectionId::new("connection-1"),
                1,
                Instant::now(),
                "encrypted-address",
                "fingerprint",
            )
            .with_follow_up("follow-up-1", "org-1"),
        )
        .unwrap();
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let authorization = unavailable_authorization();
        let error = authorization
            .require(
                &actor,
                &supplier_fulfillment::ORDER_SUBMIT,
                place_candidate(&order),
                &mut ReadExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(
            matches!(error, IdentityError::ValidationError(message) if message == "写入授权必须使用快照事务")
        );
        let error = authorization
            .require(
                &actor,
                &supplier_fulfillment::ORDER_SUBMIT,
                place_candidate(&order),
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, IdentityError::Internal(message) if message == "测试授权设施不可用"));
    }
}
