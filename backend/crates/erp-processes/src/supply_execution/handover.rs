//! 供应商履约订单跟进人交接：目标资格、可选转交开放 W26 任务。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{AccessControlExt, Permission, SharedRbacService};
use erp_supply::dto::{
    FulfillmentHandoverCandidateView, HandoverFulfillmentOrderRequest, HandoverFulfillmentOrderView,
};
use erp_supply::service::supplier_fulfillment::W26_BUSINESS_OBJECT_TYPE;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use super::follow_up::reject_company_org;
use crate::adapters::identity::shared_rbac_service;
use crate::adapters::{fulfillment_order_access, scoped_fulfillment_service};
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 显式交接内部跟进人；是否转交开放 W26 任务由命令声明，默认不改派。
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
        let rbac = shared_rbac_service(self.db.clone());
        let audit_id = format!("fulfillment-handover-{}-{}-{key}", actor.id(), id);
        let fingerprint = handover_fingerprint(actor.id(), id, &req, &key)?;
        if let Some(existing) = replay(&self.db, &rbac, &audit_id, &fingerprint, id, actor).await? {
            return Ok(existing);
        }
        ensure_target_qualified(&self.db, &rbac, req.target_user_id.trim(), &mut NoTransaction).await?;
        ensure_org_enabled(&self.db, req.target_org_unit_id.as_deref(), &mut NoTransaction).await?;
        let db = self.db.clone();
        let rbac_tx = rbac.clone();
        let actor_tx = actor.clone();
        let req_tx = req.clone();
        let id_tx = id.to_string();
        let audit_id_tx = audit_id;
        let fingerprint_tx = fingerprint;
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    persist_handover(
                        &db,
                        &rbac_tx,
                        &id_tx,
                        &req_tx,
                        &actor_tx,
                        &audit_id_tx,
                        &fingerprint_tx,
                        executor,
                    )
                    .await
                })
            })
            .await
    }

    /// 查询当前订单可交接的合格目标候选。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<FulfillmentHandoverCandidateView>> {
        let rbac = shared_rbac_service(self.db.clone());
        let handler = current_handler(&self.db, id, &mut NoTransaction).await?;
        let order = fulfillment_order_access(self.db.clone(), rbac.clone())
            .require_order(actor, "handover", id, handler.as_deref(), &mut NoTransaction)
            .await?;
        let accounts =
            self.db.accounts().list_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if account.base.id == order.follow_up_user_id || !account.is_active_backoffice() {
                continue;
            }
            if !account_can_follow(&rbac, &account.base.id).await? {
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
        Ok(candidates)
    }
}

#[allow(clippy::too_many_arguments)]
async fn persist_handover(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    id: &str,
    req: &HandoverFulfillmentOrderRequest,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<HandoverFulfillmentOrderView> {
    ensure_target_qualified(db, rbac, req.target_user_id.trim(), executor).await?;
    ensure_org_enabled(db, req.target_org_unit_id.as_deref(), executor).await?;
    let handler = current_handler(db, id, executor).await?;
    let mut view = scoped_fulfillment_service(db.clone(), rbac.clone())
        .apply_fulfillment_handover(id, req, handler.as_deref(), actor, executor)
        .await?;
    let transferred = if req.transfer_open_exception_tasks {
        transfer_open_w26(db, id, req.target_user_id.trim(), executor).await?
    } else {
        Vec::new()
    };
    view.transferred_work_item_ids = transferred.clone();
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        "supplier_fulfillment.handover",
        "supplier_fulfillment_order",
        view.order_id.clone(),
        Some(format!(
            "command_sha256={fingerprint};target={};transfer={};reason={}",
            req.target_user_id.trim(),
            req.transfer_open_exception_tasks,
            req.reason.trim()
        )),
    )?;
    db.audit_logs().create(&audit, executor).await?;
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

async fn current_handler(
    db: &mongodb::Database,
    order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<String>> {
    Ok(open_w26(db, order_id, executor)
        .await?
        .into_iter()
        .find_map(|item| item.owner_user_id.filter(|id| !id.is_empty())))
}

async fn replay(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    audit_id: &str,
    expected_fingerprint: &str,
    order_id: &str,
    actor: &AuditActor,
) -> Result<Option<HandoverFulfillmentOrderView>> {
    let Some(audit) = db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
        return Ok(None);
    };
    let expected = format!("command_sha256={expected_fingerprint}");
    let message = audit.message.as_deref().unwrap_or("");
    if message != expected && !message.starts_with(&format!("{expected};")) {
        return Err(Error::ConflictError("同一幂等键已用于不同的供应商订单交接".into()));
    }
    let handler = current_handler(db, order_id, &mut NoTransaction).await?;
    let order = fulfillment_order_access(db.clone(), rbac.clone())
        .require_order(actor, "handover", order_id, handler.as_deref(), &mut NoTransaction)
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
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备供应商订单跟进资格".into()));
    };
    if !account.is_active_backoffice() || !account_can_follow(rbac, target).await? {
        return Err(Error::Forbidden("目标账号不存在、已失效或不具备供应商订单跟进资格".into()));
    }
    Ok(())
}

async fn ensure_org_enabled(
    db: &mongodb::Database,
    target_org: Option<&str>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    reject_company_org(org)?;
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}

async fn account_can_follow(rbac: &SharedRbacService, user_id: &str) -> Result<bool> {
    let permission = Permission::parse("supplier_fulfillment_order:handover").map_err(Error::from)?;
    Ok(rbac.enforce(user_id, &permission).await?)
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
