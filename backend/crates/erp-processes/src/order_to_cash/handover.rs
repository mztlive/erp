//! 销售责任显式交接与验收联动（S3-07）。
//!
//! 开放验收任务随交接原子转交；开放审批任务保持不变；已完成验收与历史
//! 归属快照不改写。业务组织不随接收人部门隐式变化，必须显式传入。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_sales::dto::sales_order::{HandoverCandidateView, HandoverSalesOrderRequest, HandoverSalesOrderView};
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::command::identity::{
    sales_handover_audit_id, sales_handover_audit_message, sales_handover_fingerprint,
    sales_handover_fingerprint_matches,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use erp_workflow::service::approval::business_adapter::ensure_separation_of_duties;
use erp_workflow::service::approval::policy::SeparationOfDutiesPolicy;
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
    /// 账号失效、动作权限不足、对象不可见、版本冲突、目标不合格
    /// （含与开放审批人岗位分离冲突）、异载荷同幂等键或组织停用时拒绝；
    /// 失败零部分变更。
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
        self.ensure_handover_target(
            &target,
            &current.sales_owner_user_id,
            &current.base.id,
            &mut NoTransaction,
        )
        .await?;
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
            .with_transaction(move |executor| {
                Box::pin(async move {
                    access_for_tx.revalidate(&id_owned, expected_version, executor).await?;
                    apply_handover(
                        &db,
                        &id_owned,
                        &req_owned,
                        &target_owned,
                        &reason,
                        &actor_owned,
                        &audit_id_owned,
                        &fingerprint_owned,
                        executor,
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
    /// 不按部门相同与否过滤；同部门无资格不得接收，跨部门有资格正常接收；
    /// 该单开放审批任务的审批人不得列为候选（提交与审批分离）。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let access = self.command_access(actor, "update")?;
        let order = access.current(id, &mut NoTransaction).await?;
        let rbac = self.require_rbac()?.clone();
        let approvers = open_approval_assignees(&self.db, &order.base.id, &mut NoTransaction).await?;
        let accounts =
            self.db.accounts().list_by_kind(erp_core::AccountKind::Admin, &mut NoTransaction).await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if account.base.id == order.sales_owner_user_id
                || !account.is_active_backoffice()
                || approvers.iter().any(|approver| approver == &account.base.id)
            {
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
        if !sales_handover_fingerprint_matches(audit.message.as_deref(), expected_fingerprint) {
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
        let order = access.current(sales_order_id, &mut NoTransaction).await?;
        let acceptance_ids = open_acceptance_task_ids(&self.db, &order.base.id, &mut NoTransaction).await?;
        let approval_assignees =
            open_approval_assignees(&self.db, &order.base.id, &mut NoTransaction).await?;
        Ok(Some(HandoverSalesOrderView {
            sales_order_id: order.base.id.clone(),
            sales_owner_user_id: order.sales_owner_user_id.clone(),
            business_org_unit_id: order.business_org_unit_id.clone(),
            version: order.base.version,
            transferred_acceptance_task_ids: acceptance_ids,
            kept_approval_task_count: approval_assignees.len(),
        }))
    }

    /// 在事务外预检目标账号有效性、完整执行资格与审批岗位分离。
    async fn ensure_handover_target(
        &self,
        target: &str,
        current_owner: &str,
        sales_order_id: &str,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        if target == current_owner {
            return Err(Error::BusinessLogicError("目标已是当前负责人，无需交接".to_string()));
        }
        let rbac = self.require_rbac()?.clone();
        ensure_target_qualified(&self.db, &rbac, target, executor).await?;
        ensure_handover_separation(&self.db, sales_order_id, target, executor).await
    }

    /// 在事务外预检显式目标业务组织启用状态。
    async fn ensure_handover_org(
        &self,
        target_org: Option<&str>,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        ensure_org_enabled(&self.db, target_org, executor).await
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
#[allow(clippy::too_many_arguments)]
async fn apply_handover(
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
    let rbac = crate::adapters::identity::shared_rbac_service(db.clone());
    ensure_target_qualified(db, &rbac, target, executor).await?;
    ensure_org_enabled(db, req.target_business_org_unit_id.as_deref(), executor).await?;
    ensure_handover_separation(db, id, target, executor).await?;
    let next_org = req
        .target_business_org_unit_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    order.handover(target.to_string(), next_org, actor.id())?;
    let mut acceptance_tasks = open_acceptance_tasks(db, id, executor).await?;
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
        Some(sales_handover_audit_message(fingerprint, target, reason, &transferred)),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(transferred)
}

/// 校验目标账号有效且具备验收完整执行资格。
///
/// # 参数
/// * `db` - 目标数据库
/// * `rbac` - 共享授权服务
/// * `target` - 目标负责销售
/// * `executor` - 调用方执行器（事务外预检与原事务重验共用）
///
/// # 返回
/// 目标有效且具备验收读取与执行资格时成功。
///
/// # 错误
/// 账号缺失、失效或缺少验收完整执行权限时拒绝。
///
/// # 关键业务约束
/// 只判定账号与权限事实；提交—审批分离另由 `ensure_handover_separation` 执行。
async fn ensure_target_qualified(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    if account_qualified_for_acceptance(db, rbac, target, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden("目标账号不存在、已失效或不具备销售验收读取与执行资格".to_string()))
}

/// 校验显式目标业务组织启用状态；留空表示保留原组织。
///
/// # 参数
/// * `db` - 目标数据库
/// * `target_org` - 显式目标业务组织；`None` 或空白保留原组织
/// * `executor` - 调用方执行器（事务外预检与原事务重验共用）
///
/// # 返回
/// 留空或目标组织链路全部启用时成功。
///
/// # 错误
/// 目标组织未知或链路存在停用节点时拒绝。
///
/// # 关键业务约束
/// 组织不随接收人部门隐式变化；本检查不提供任何范围授权。
async fn ensure_org_enabled(
    db: &mongodb::Database,
    target_org: Option<&str>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".to_string()));
    }
    Ok(())
}

/// 强制交接目标与开放审批任务的提交—审批岗位分离。
///
/// # 参数
/// * `db` - 目标数据库
/// * `sales_order_id` - 销售单 ID
/// * `target` - 目标负责销售（交接后承担提交侧责任）
/// * `executor` - 调用方执行器（事务外预检与原事务重验共用）
///
/// # 返回
/// 目标未兼任该单开放审批人时成功。
///
/// # 错误
/// 目标是该单开放审批任务的当前审批人时拒绝；失败不产生任何写入。
///
/// # 关键业务约束
/// 复用审批域 `ForbidSubmitterAsApprover` 纯规则；开放审批任务本身保持不变。
async fn ensure_handover_separation(
    db: &mongodb::Database,
    sales_order_id: &str,
    target: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    let approvers = open_approval_assignees(db, sales_order_id, executor).await?;
    ensure_separation_of_duties(SeparationOfDutiesPolicy::ForbidSubmitterAsApprover, target, &approvers)
        .map_err(|error| Error::Forbidden(format!("目标交接人违反销售审批岗位分离约束: {error}")))?;
    Ok(())
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
        .filter(|item| is_handover_transferable_type(item.work_item_type))
        .collect())
}

/// 判定任务类型是否随销售交接转交。
///
/// 仅客户验收任务随交接转交；审批任务、交付与其他任务类型保持不变，
/// 由各自领域命令处理。调用方传入的已是开放任务，存续状态由
/// `list_active_by_object` 保证，本判定只做类型分流。
///
/// # 参数
/// * `work_item_type` - 待判定任务的固定类型
///
/// # 返回
/// 客户验收任务返回 `true`，其他类型返回 `false`。
///
/// # 错误
/// 无。
fn is_handover_transferable_type(work_item_type: WorkItemType) -> bool {
    work_item_type == WorkItemType::CustomerAcceptanceRegistration
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

/// 列出该销售单全部开放审批任务的当前审批人。
///
/// # 参数
/// * `db` - 目标数据库
/// * `sales_order_id` - 销售单 ID
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回开放 `DocumentApproval` 任务的 `owner_user_id` 清单。
///
/// # 错误
/// 任务读取失败时返回错误。
///
/// # 关键业务约束
/// 只读审批任务用于岗位分离校验、候选过滤与随转计数，不得改写。
async fn open_approval_assignees(
    db: &mongodb::Database,
    sales_order_id: &str,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    Ok(db
        .work_items()
        .list_active_by_object(ACCEPTANCE_OBJECT_TYPE, sales_order_id, executor)
        .await?
        .into_iter()
        .filter(|item| item.work_item_type == WorkItemType::DocumentApproval)
        .filter_map(|item| item.owner_user_id)
        .collect())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId};
    use erp_sales::entity::sales_order::{
        BusinessType, CommercialStatus, OriginSystem, SalesOrder, SalesOrderData,
    };
    use erp_sales::service::sales_order::command::identity::{
        sales_handover_audit_message, sales_handover_fingerprint_matches,
    };

    use super::{SeparationOfDutiesPolicy, ensure_separation_of_duties, is_handover_transferable_type};

    fn sales_order_owned_by(owner: &str, org: &str) -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                sales_owner_user_id: owner.into(),
                business_org_unit_id: org.into(),
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
        .expect("销售单应合法")
    }

    /// 交接审计收据必须绑定本次载荷指纹：同载荷重放放行，异载荷同键拒绝。
    ///
    /// 直接驱动生产收据构造与回放判定；若回放改回精确字符串相等，
    /// 本用例失败（写入的扩展格式含目标／原因／随转清单，精确相等永不命中）。
    #[test]
    fn handover_receipt_binds_fingerprint_and_rejects_alien_payload() {
        let transferred = vec!["accept-1".to_string(), "accept-2".to_string()];
        let stored = sales_handover_audit_message("fp-same", "sales-b", "轮换", &transferred);
        assert!(stored.contains("command_sha256=fp-same"));
        assert!(stored.contains("sales-b"));
        assert!(stored.contains("accept-1"));
        assert!(sales_handover_fingerprint_matches(Some(&stored), "fp-same"));
        assert!(!sales_handover_fingerprint_matches(Some(&stored), "fp-other"));
        assert!(!sales_handover_fingerprint_matches(None, "fp-same"));
        assert!(!sales_handover_fingerprint_matches(Some(""), "fp-same"));
    }

    /// 前缀伪造不得蒙混：指纹 `abc` 的收据不能冒充 `abc-def` 的回放。
    #[test]
    fn handover_receipt_rejects_fingerprint_prefix_forgery() {
        let stored = sales_handover_audit_message("abc-def", "sales-b", "轮换", &[]);
        assert!(!sales_handover_fingerprint_matches(Some(&stored), "abc"));
        assert!(sales_handover_fingerprint_matches(Some(&stored), "abc-def"));
    }

    /// 仅指纹的旧格式收据仍可回放，保证已写入收据的兼容性。
    #[test]
    fn handover_replay_accepts_legacy_fingerprint_only_receipt() {
        assert!(sales_handover_fingerprint_matches(Some("command_sha256=fp-old"), "fp-old"));
        assert!(!sales_handover_fingerprint_matches(Some("command_sha256=fp-old"), "fp-new"));
    }

    /// 仅开放验收任务随交接转交；审批与其他任务类型保持不变。
    #[test]
    fn handover_transfers_only_acceptance_task_type() {
        use erp_workflow::entity::work_item::WorkItemType;

        assert!(is_handover_transferable_type(WorkItemType::CustomerAcceptanceRegistration));
        assert!(!is_handover_transferable_type(WorkItemType::DocumentApproval));
    }

    /// 交接实体只改负责人与组织，不改归属快照与状态；显式组织留空保留。
    #[test]
    fn handover_entity_keeps_attribution_and_status() {
        let mut order = sales_order_owned_by("sales-a", "org-a");
        order.handover("sales-b".into(), None, "manager").unwrap();
        assert_eq!(order.sales_owner_user_id, "sales-b");
        assert_eq!(order.business_org_unit_id, "org-a");
        assert!(order.attribution.is_none());
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
        order.handover("sales-c".into(), Some("org-b".into()), "manager").unwrap();
        assert_eq!(order.business_org_unit_id, "org-b");
        assert!(order.attribution.is_none());
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
    }

    /// 失败交接零部分变更：责任、组织与版本保持原值（原子性）。
    #[test]
    fn failed_handover_leaves_order_unchanged() {
        let mut order = sales_order_owned_by("sales-b", "org-a");
        let version = order.base.version;
        assert!(order.handover("sales-b".into(), Some("org-a".into()), "manager").is_err());
        assert!(order.handover("".into(), None, "manager").is_err());
        assert_eq!(order.sales_owner_user_id, "sales-b");
        assert_eq!(order.business_org_unit_id, "org-a");
        assert_eq!(order.base.version, version);
        assert_eq!(order.commercial_status, CommercialStatus::Draft);
    }

    /// 交接目标不得兼任该单开放审批人；无审批或非审批人时放行。
    #[test]
    fn handover_target_separated_from_open_approvers() {
        let policy = SeparationOfDutiesPolicy::ForbidSubmitterAsApprover;
        assert!(ensure_separation_of_duties(policy, "sales-b", &[]).is_ok());
        assert!(ensure_separation_of_duties(policy, "sales-b", &["approver-1".to_string()]).is_ok());
        assert!(
            ensure_separation_of_duties(
                policy,
                "approver-1",
                &["approver-1".to_string(), "approver-2".to_string()]
            )
            .is_err()
        );
    }
}
