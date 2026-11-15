//! 供应商履约订单跟进人交接：目标资格、可选转交开放 W26 任务。

use std::sync::Arc;

use application_core::{AuditActor, CommandFingerprint};
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_identity::repository::prelude::*;
use erp_identity::service::organization_access::ensure_org_enabled as ensure_identity_org_enabled;
use erp_identity::{AccessControlExt, AuthorizationService};
use erp_permissions::supplier_fulfillment;
use erp_supply::command_receipt::SupplyCommandResult;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::dto::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
};
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_fulfillment::{SupplierFulfillmentService, W26_BUSINESS_OBJECT_TYPE};
use erp_workflow::repository::prelude::*;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use super::follow_up::reject_company_org;
use super::receipt::persist_supply_receipt;
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 显式交接内部跟进人；是否转交开放 W26 任务由命令声明，默认不改派。
    ///
    /// # 参数
    /// * `id` - 供应商履约订单 ID。
    /// * `req` - 交接命令。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 返回交接后的订单视图；同一命令重放返回原跟进人与组织，不重复列出已转交任务。
    ///
    /// # 错误
    /// 请求校验失败、幂等键为空、目标组织为公司根或未启用、目标账号不合格、同一幂等键载荷不同，或交接写入失败时返回对应错误。
    pub async fn handover_fulfillment_order(
        &self,
        id: &str,
        req: HandoverFulfillmentOrderRequest,
        actor: &AuditActor,
    ) -> Result<HandoverFulfillmentOrderView> {
        req.validate()?;
        let key = req.idempotency_key.trim().to_string();
        if key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".into()));
        }
        if let Some(org) = req.target_org_unit_id.as_deref().map(str::trim).filter(|value| !value.is_empty())
        {
            reject_company_org(org)?;
        }
        let identity = self.identity.clone();
        let rbac = identity.rbac;
        let service = self.domain_with_scope(identity.data_scope);
        let audit_id = format!(
            "fulfillment-handover-{}",
            CommandFingerprint::from_parts([actor.id().to_string(), id.to_string(), key.clone()])
                .digest_hex()
        );
        let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
        let db = self.db.clone();
        let write = HandoverWrite {
            service,
            rbac: rbac.clone(),
            actor: actor.clone(),
            request: req.clone(),
            id: id.to_string(),
            audit_id,
            fingerprint,
        };
        db.client()
            .clone()
            .with_transaction(move |executor| Box::pin(async move { write.apply(&db, executor).await }))
            .await
    }

    /// 查询当前订单可交接的合格目标候选。
    ///
    /// # 参数
    /// * `id` - 供应商履约订单 ID。
    /// * `actor` - 当前操作人。
    ///
    /// # 返回
    /// 返回按姓名、账号和用户 ID 排序的后台候选。当前跟进人以及停用或无交接权限的账号被排除。
    ///
    /// # 错误
    /// 订单不在交接范围内，或账号、权限查询失败时返回对应错误。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<FulfillmentHandoverCandidateView>> {
        let identity = self.identity.clone();
        let rbac = identity.rbac;
        let service = self.domain_with_scope(identity.data_scope);
        let db = self.db.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = service
                        .access()
                        .require_order(&actor, &supplier_fulfillment::ORDER_HANDOVER, &id, executor)
                        .await?;
                    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, executor).await?;
                    let mut candidates = Vec::new();
                    for account in accounts {
                        if account.base.id == order.follow_up_user_id {
                            continue;
                        }
                        if !account_can_follow(&db, &rbac, &account.base.id, executor).await? {
                            continue;
                        }
                        candidates.push(FulfillmentHandoverCandidateView {
                            user_id: account.base.id.clone(),
                            display_name: account.name.clone(),
                            account: account.secret.account().to_string(),
                        });
                    }
                    candidates.sort_by(|left, right| {
                        left.display_name
                            .cmp(&right.display_name)
                            .then_with(|| left.account.cmp(&right.account))
                            .then_with(|| left.user_id.cmp(&right.user_id))
                    });
                    service
                        .access()
                        .require_order(&actor, &supplier_fulfillment::ORDER_HANDOVER, &id, executor)
                        .await?;
                    Ok(candidates)
                })
            })
            .await
    }
}

/// 在根事务前冻结的交接身份和领域能力；不重建授权来源。
struct HandoverWrite {
    service: SupplierFulfillmentService,
    rbac: Arc<AuthorizationService>,
    actor: AuditActor,
    request: HandoverFulfillmentOrderRequest,
    id: String,
    audit_id: String,
    fingerprint: String,
}

impl HandoverWrite {
    /// 在入口事务内按原顺序持久化责任交接、开放任务、回执及审计。
    async fn apply(self, db: &Database, executor: &mut dyn Executor) -> Result<HandoverFulfillmentOrderView> {
        persist_handover(
            db,
            &self.service,
            &self.rbac,
            &self.id,
            &self.request,
            &self.actor,
            &self.audit_id,
            &self.fingerprint,
            executor,
        )
        .await
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_handover(
    db: &mongodb::Database,
    service: &SupplierFulfillmentService,
    rbac: &Arc<AuthorizationService>,
    id: &str,
    req: &HandoverFulfillmentOrderRequest,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<HandoverFulfillmentOrderView> {
    service.access().require_order(actor, &supplier_fulfillment::ORDER_HANDOVER, id, executor).await?;
    if let Some(existing) = replay(
        db,
        service,
        HandoverReceipt { audit_id, fingerprint, key: &req.idempotency_key },
        id,
        actor,
        executor,
    )
    .await?
    {
        return Ok(existing);
    }
    ensure_target_qualified(db, rbac, req.target_user_id.trim(), executor).await?;
    ensure_org_enabled(db, req.target_org_unit_id.as_deref(), executor).await?;
    let mut view = service.apply_fulfillment_handover(id, req, actor, executor).await?;
    let transferred = if req.transfer_open_exception_tasks {
        transfer_open_w26(db, id, req.target_user_id.trim(), executor).await?
    } else {
        Vec::new()
    };
    view.transferred_work_item_ids = transferred.clone();
    let audit = actor
        .clone()
        .resource_log_with_id(
            audit_id.to_string(),
            "supplier_fulfillment.handover",
            "supplier_fulfillment_order",
            view.order_id.clone(),
            Some("供应商履约订单跟进责任已交接".to_string()),
        )?
        .with_command_id(Some(audit_id.to_string()))?;
    persist_supply_receipt(
        db,
        &audit,
        fingerprint,
        &req.idempotency_key,
        id,
        SupplyCommandResult::FulfillmentHandover,
        executor,
    )
    .await?;
    persist_log(db, &audit, executor).await?;
    Ok(view)
}

async fn transfer_open_w26(
    db: &mongodb::Database,
    order_id: &str,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    let mut tasks = open_w26(db, order_id, executor).await?;
    let at = Instant::now();
    let mut transferred = Vec::new();
    for task in &mut tasks {
        task.reassign(target.to_string(), at)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        db.work_items().update(task, executor).await?;
        transferred.push(task.base.id.clone());
    }
    Ok(transferred)
}

async fn open_w26(
    db: &mongodb::Database,
    order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<WorkItem>> {
    Ok(db
        .work_items()
        .list_active_by_object(W26_BUSINESS_OBJECT_TYPE, order_id, executor)
        .await?
        .into_iter()
        .filter(|item| {
            matches!(
                item.work_item_type,
                WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException
            )
        })
        .collect())
}

/// 原交接命令的不可变回执身份及请求指纹。
struct HandoverReceipt<'a> {
    audit_id: &'a str,
    fingerprint: &'a str,
    key: &'a str,
}

async fn replay(
    db: &mongodb::Database,
    service: &SupplierFulfillmentService,
    receipt: HandoverReceipt<'_>,
    order_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<HandoverFulfillmentOrderView>> {
    let HandoverReceipt { audit_id, fingerprint, key } = receipt;
    let Some(stored) = db.supply_command_receipts().find_command(audit_id, executor).await? else {
        return Ok(None);
    };
    stored.verify_identity(
        audit_id,
        actor.id(),
        "supplier_fulfillment.handover",
        order_id,
        &stable_digest(key.trim()),
    )?;
    stored.verify(fingerprint, Some(order_id), "同一幂等键已用于不同的供应商订单交接")?;
    if !matches!(stored.result, SupplyCommandResult::FulfillmentHandover) {
        return Err(Error::Internal("供应商订单交接回执类型非法".to_string()));
    }
    let order = service
        .access()
        .require_order(actor, &supplier_fulfillment::ORDER_HANDOVER, order_id, executor)
        .await?;
    Ok(Some(HandoverFulfillmentOrderView {
        order_id: order.base.id,
        follow_up_user_id: order.follow_up_user_id,
        business_org_unit_id: order.business_org_unit_id,
        version: order.base.version,
        transferred_work_item_ids: Vec::new(),
    }))
}

async fn ensure_target_qualified(
    db: &mongodb::Database,
    rbac: &Arc<AuthorizationService>,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    if !account_can_follow(db, rbac, target, executor).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备供应商订单跟进资格".into()));
    }
    Ok(())
}

async fn ensure_org_enabled(
    db: &mongodb::Database,
    target_org: Option<&str>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    if let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) {
        reject_company_org(org)?;
    }
    Ok(ensure_identity_org_enabled(db, target_org, executor).await?)
}

async fn account_can_follow(
    _db: &mongodb::Database,
    rbac: &Arc<AuthorizationService>,
    user_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<bool> {
    let permission = supplier_fulfillment::ORDER_HANDOVER;
    let actor =
        application_core::AuditActor::new(user_id.into(), user_id.into(), erp_core::AccountKind::Admin);
    Ok(rbac.has_permission(&actor, &permission, executor).await?)
}

fn handover_fingerprint(
    actor_id: &str,
    order_id: &str,
    req: &HandoverFulfillmentOrderRequest,
    key: &str,
) -> Result<String> {
    serde_json::to_string(&(
        actor_id,
        order_id,
        req.target_user_id.trim(),
        req.target_org_unit_id.as_deref().map(str::trim),
        req.transfer_open_exception_tasks,
        req.reason.trim(),
        req.expected_version,
        key,
    ))
    .map_err(|error| Error::Internal(format!("供应商订单交接命令序列化失败: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_command_does_not_transfer_w26() {
        let req: HandoverFulfillmentOrderRequest = serde_json::from_value(serde_json::json!({
            "target_user_id": "buyer-2",
            "reason": "岗位调整",
            "expected_version": 3,
            "idempotency_key": "k1"
        }))
        .unwrap();
        assert!(!req.transfer_open_exception_tasks);
        let with_transfer: HandoverFulfillmentOrderRequest = serde_json::from_value(serde_json::json!({
            "target_user_id": "buyer-2",
            "reason": "岗位调整",
            "expected_version": 3,
            "idempotency_key": "k1",
            "transfer_open_exception_tasks": true
        }))
        .unwrap();
        assert!(with_transfer.transfer_open_exception_tasks);
    }
}
