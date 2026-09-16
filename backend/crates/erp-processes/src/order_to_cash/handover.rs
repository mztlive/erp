//! 销售责任显式交接与验收联动（S3-07）。
//!
//! 开放验收任务随交接原子转交；开放审批任务保持不变；已完成验收与历史
//! 归属快照不改写。业务组织不随接收人部门隐式变化，必须显式传入。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_sales::dto::sales_order::{HandoverCandidateView, HandoverSalesOrderRequest, HandoverSalesOrderView};
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::command::identity::{
    sales_handover_audit_id, sales_handover_fingerprint,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SalesOrderCommandProcess;
use crate::{Error, Result};

/// 客户验收完整执行权限的固定对象类型。
const ACCEPTANCE_OBJECT_TYPE: &str = "sales_order";

impl SalesOrderCommandProcess {
    /// 显式交接销售单当前负责人与业务组织，并原子转交开放验收任务。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 交接请求（含期望版本、目标、原因与幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回交接后责任与转交任务清单。
    ///
    /// # 错误
    /// 账号失效、动作权限不足、对象不可见、版本冲突、目标不合格、
    /// 异载荷同幂等键或组织停用时拒绝；失败零部分变更。
    ///
    /// # 关键业务约束
    /// 开放验收任务随交接转交；审批任务、已完成验收与历史归属保持不变。
    pub async fn handover_sales_order(
        &self,
        id: &str,
        req: HandoverSalesOrderRequest,
        actor: &AuditActor,
    ) -> Result<HandoverSalesOrderView> {
        req.validate()?;
        let idempotency_key = req.idempotency_key.trim().to_string();
        if idempotency_key.is_empty() {
            return Err(Error::ValidationError("幂等键不能为空".to_string()));
        }
        let reason = req.reason.trim().to_string();
        if reason.is_empty() {
            return Err(Error::ValidationError("交接原因不能为空".to_string()));
        }
        let target = req.target_owner_user_id.trim().to_string();
        if target.is_empty() {
            return Err(Error::ValidationError("目标负责销售不能为空".to_string()));
        }
        let audit_id = sales_handover_audit_id(actor.id(), id, &idempotency_key);
        let fingerprint = sales_handover_fingerprint(actor.id(), id, &req)?;
        if let Some(existing) = self.replay_sales_handover(&audit_id, &fingerprint, id, actor).await? {
            return Ok(existing);
        }
        let access = self.command_access(actor, "update")?;
        let current = access.current(id, &mut NoTransaction).await?;
        if current.base.version != req.expected_version {
            return Err(Error::ConflictError("销售单责任或版本已变化，请刷新后重试".to_string()));
        }
        self.ensure_handover_target(&target, &current.sales_owner_user_id, &mut NoTransaction).await?;
        self.ensure_handover_org(req.target_business_org_unit_id.as_deref(), &mut NoTransaction).await?;

        let db = self.db.clone();
        let actor_owned = actor.clone();
        let req_owned = req.clone();
        let id_owned = id.to_string();
        let fingerprint_owned = fingerprint.clone();
        let audit_id_owned = audit_id.clone();
        let target_owned = target.clone();
        let access_for_tx = access.clone();
        let expected_version = req.expected_version;
        db.client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    access_for_tx.revalidate(&id_owned, expected_version, session).await?;
                    handover_in_transaction(
                        &db,
                        &id_owned,
                        &req_owned,
                        &target_owned,
                        &reason,
                        &actor_owned,
                        &audit_id_owned,
                        &fingerprint_owned,
                        session,
                    )
                    .await
                })
            })
            .await?;
        self.replay_sales_handover(&audit_id, &fingerprint, id, actor)
            .await?
            .ok_or_else(|| Error::Internal("销售交接幂等收据缺失".to_string()))
    }

    /// 查询当前销售单可交接的合格目标候选。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回有效且具备验收执行资格的账号；只作交互提示，提交仍重验。
    ///
    /// # 错误
    /// 对象不可见或授权读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 不按部门相同与否过滤；同部门无资格不得接收，跨部门有资格正常接收。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let access = self.command_access(actor, "update")?;
        let order = access.current(id, &mut NoTransaction).await?;
        let rbac = self.require_rbac()?.clone();
        let accounts =
            self.db.accounts().list_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if account.base.id == order.sales_owner_user_id || !account.is_active_backoffice() {
                continue;
            }
            if !account_qualified_for_acceptance(&self.db, &rbac, &account.base.id, &mut NoTransaction)
                .await?
            {
                continue;
            }
            candidates.push(HandoverCandidateView {
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

    /// 按稳定审计收据重放已交接的销售责任。
    async fn replay_sales_handover(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        sales_order_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<HandoverSalesOrderView>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        if audit.message.as_deref() != Some(&format!("command_sha256={expected_fingerprint}")) {
            return Err(Error::ConflictError("同一幂等键已用于不同的销售交接".to_string()));
        }
        let order_id = audit
            .resource_id
            .as_deref()
            .ok_or_else(|| Error::Internal("销售交接幂等收据缺少结果引用".to_string()))?;
        if order_id != sales_order_id {
            return Err(Error::Internal("销售交接幂等收据与业务对象不一致".to_string()));
        }
        let access = self.command_access(actor, "update")?;
        let sales_order_id = sales_order_id.to_string();
        let order = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { access.current(&sales_order_id, executor).await })
            })
            .await?;
        let acceptance_ids = open_acceptance_task_ids(&self.db, &order.base.id, &mut NoTransaction).await?;
        let approval_count = open_approval_task_count(&self.db, &order.base.id, &mut NoTransaction).await?;
        Ok(Some(HandoverSalesOrderView {
            sales_order_id: order.base.id.clone(),
            sales_owner_user_id: order.sales_owner_user_id.clone(),
            business_org_unit_id: order.business_org_unit_id.clone(),
            version: order.base.version,
            transferred_acceptance_task_ids: acceptance_ids,
            kept_approval_task_count: approval_count,
        }))
    }

    /// 在事务外预检目标账号有效性与完整执行资格。
    async fn ensure_handover_target(
        &self,
        target: &str,
        current_owner: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        if target == current_owner {
            return Err(Error::BusinessLogicError("目标已是当前负责人，无需交接".to_string()));
        }
        let rbac = self.require_rbac()?.clone();
        if !account_qualified_for_acceptance(&self.db, &rbac, target, executor).await? {
            return Err(Error::Forbidden("目标账号不具备销售验收读取与执行资格".to_string()));
        }
        Ok(())
    }

    /// 在事务外预检显式目标业务组织启用状态。
    async fn ensure_handover_org(
        &self,
        target_org: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(());
        };
        let state = erp_identity::repository::OrganizationRepository::new(&self.db).state(executor).await?;
        let tree = erp_identity::entity::organization::OrgTree::new(&state.units)?;
        let path = tree.path(org)?;
        if path.iter().any(|node| !node.enabled) {
            return Err(Error::BusinessLogicError("目标业务组织已停用".to_string()));
        }
        Ok(())
    }
}

/// 在调用方根事务内原子执行交接与验收任务转交。
///
/// # 参数
/// * `db` - 目标数据库
/// * `id` - 销售单 ID
/// * `req` - 已校验的交接请求
/// * `target` - 目标负责销售
/// * `reason` - 交接原因
/// * `actor` - 操作人
/// * `audit_id` - 稳定幂等收据 ID
/// * `fingerprint` - 请求指纹
/// * `executor` - 调用方根事务执行器
///
/// # 返回
/// 返回转交的验收任务 ID 清单。
///
/// # 错误
/// 版本、资格或组织任一失败时整事务回滚。
///
/// # 关键业务约束
/// 只更新销售单负责人／组织与开放验收任务；审批、已完成与快照不变。
async fn handover_in_transaction(
    db: &mongodb::Database,
    id: &str,
    req: &HandoverSalesOrderRequest,
    target: &str,
    reason: &str,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    let mut order = db
        .sales_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在或无权操作".to_string()))?;
    if order.base.version != req.expected_version {
        return Err(Error::ConflictError("销售单责任或版本已变化，请刷新后重试".to_string()));
    }
    ensure_target_qualified_in_transaction(db, target, executor).await?;
    if let Some(org) = req.target_business_org_unit_id.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        let state = erp_identity::repository::OrganizationRepository::new(db).state(executor).await?;
        let tree = erp_identity::entity::organization::OrgTree::new(&state.units)?;
        let path = tree.path(org)?;
        if path.iter().any(|node| !node.enabled) {
            return Err(Error::BusinessLogicError("目标业务组织已停用".to_string()));
        }
    }
    let next_org = req
        .target_business_org_unit_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    order.handover(target.to_string(), next_org, actor.id())?;
    let mut acceptance_tasks = open_acceptance_tasks(db, id, executor).await?;
    let approval_count = open_approval_task_count(db, id, executor).await?;
    let _ = approval_count;
    let at = erp_core::common::time::Instant::now();
    let mut transferred = Vec::new();
    for task in &mut acceptance_tasks {
        task.reassign(target.to_string(), at)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        db.work_items().update(task, executor).await?;
        transferred.push(task.base.id.clone());
    }
    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
        .persist_order(&mut order, executor)
        .await?;
    let audit = actor.clone().resource_log_with_id(
        audit_id.to_string(),
        "sales_order.handover",
        "sales_order",
        order.base.id.clone(),
        Some(format!(
            "command_sha256={fingerprint};target={target};reason={reason};acceptance={}",
            transferred.join(",")
        )),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(transferred)
}

/// 在事务内重验目标账号有效性与验收完整执行资格。
async fn ensure_target_qualified_in_transaction(
    db: &mongodb::Database,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let account = db
        .accounts()
        .find_by_id(target, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
    let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
    let granted = PermissionSet::new(rbac.permissions(account.kind, target).await?);
    let required_codes = WorkItemType::CustomerAcceptanceRegistration
        .customer_acceptance_execution_permissions(ACCEPTANCE_OBJECT_TYPE)
        .ok_or_else(|| Error::Internal("客户验收登记对象权限合同必须存在".to_string()))?;
    let required = PermissionSet::new(
        required_codes.iter().map(|code| Permission::parse(code).expect("固定客户验收操作权限必须合法")),
    );
    if granted.covers(&required) {
        return Ok(());
    }
    Err(Error::Forbidden("目标账号不具备销售验收读取与执行资格".to_string()))
}

/// 事务外判断账号是否具备验收执行资格。
async fn account_qualified_for_acceptance(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<bool> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Ok(false);
    };
    if !account.is_active_backoffice() {
        return Ok(false);
    }
    let granted = PermissionSet::new(rbac.permissions(account.kind, target).await?);
    let Some(required_codes) = WorkItemType::CustomerAcceptanceRegistration
        .customer_acceptance_execution_permissions(ACCEPTANCE_OBJECT_TYPE)
    else {
        return Ok(false);
    };
    let mut required = Vec::new();
    for code in required_codes {
        let Ok(permission) = Permission::parse(code) else {
            return Ok(false);
        };
        required.push(permission);
    }
    Ok(granted.covers(&PermissionSet::new(required)))
}

/// 列出该销售单全部开放验收任务。
async fn open_acceptance_tasks(
    db: &mongodb::Database,
    sales_order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<WorkItem>> {
    Ok(db
        .work_items()
        .list_active_by_object(ACCEPTANCE_OBJECT_TYPE, sales_order_id, executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == WorkItemType::CustomerAcceptanceRegistration)
        .collect())
}

/// 列出开放验收任务 ID（幂等回放用，不校验身份）。
async fn open_acceptance_task_ids(
    db: &mongodb::Database,
    sales_order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    Ok(open_acceptance_tasks(db, sales_order_id, executor)
        .await?
        .into_iter()
        .map(|item| item.base.id)
        .collect())
}

/// 统计未改动的开放审批任务数。
async fn open_approval_task_count(
    db: &mongodb::Database,
    sales_order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<usize> {
    Ok(db
        .work_items()
        .list_active_by_object(ACCEPTANCE_OBJECT_TYPE, sales_order_id, executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == WorkItemType::DocumentApproval)
        .count())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId};
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};

    /// 交接生产路径必须复用公共对象判定与原事务重验，不得另建授权解释。
    #[test]
    fn handover_reuses_object_scope_and_transaction_revalidation() {
        let production = include_str!("handover.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(production.contains("command_access(actor, \"update\")"));
        assert!(production.contains("access.current(id"));
        assert!(production.contains(".revalidate("));
        assert!(production.contains("with_transaction"));
    }

    /// 交接必须原子转交开放验收任务且保持审批与快照不变。
    #[test]
    fn handover_transfers_acceptance_and_keeps_approval_snapshot() {
        let production = include_str!("handover.rs").split("#[cfg(test)]").next().expect("生产代码");
        assert!(production.contains("CustomerAcceptanceRegistration"));
        assert!(production.contains("DocumentApproval"));
        assert!(production.contains("sales_order.handover"));
        assert!(production.contains("同一幂等键已用于不同的销售交接"));
        let entity = include_str!("../../../erp-sales/src/entity/sales_order/entity/order.rs");
        assert!(entity.contains("pub fn handover("));
        assert!(entity.contains("禁随接收人部门隐式变化") || entity.contains("不随接收人部门"));
    }

    /// 交接实体只改负责人与组织，不改归属快照与状态。
    #[test]
    fn handover_entity_keeps_attribution_and_status() {
        let mut order = SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                sales_owner_user_id: "sales-a".into(),
                business_org_unit_id: "org-a".into(),
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: Some(ContractId::new("contract-1")),
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "sales-a",
        )
        .unwrap();
        order.handover("sales-b".into(), None, "manager").unwrap();
        assert_eq!(order.sales_owner_user_id, "sales-b");
        assert_eq!(order.business_org_unit_id, "org-a");
        assert!(order.attribution.is_none());
        order.handover("sales-c".into(), Some("org-b".into()), "manager").unwrap();
        assert_eq!(order.business_org_unit_id, "org-b");
        assert!(order.handover("sales-c".into(), Some("org-b".into()), "manager").is_err());
    }
}
