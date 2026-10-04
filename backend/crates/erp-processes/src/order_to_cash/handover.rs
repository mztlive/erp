//! 销售责任显式交接与验收联动（S3-07）。
//!
//! 开放验收任务随交接原子转交；开放审批任务保持不变；已完成验收与历史
//! 归属快照不改写。业务组织不随接收人部门隐式变化，必须显式传入。

use application_core::{AuditActor, CommandFingerprint};
use erp_core::ids::SalesOrderId;
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, Permission, PermissionSet, SharedRbacService};
use erp_sales::dto::sales_order::{HandoverCandidateView, HandoverSalesOrderRequest, HandoverSalesOrderView};
use erp_sales::entity::command_receipt::SalesCommandResult;
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::command::identity::{
    sales_handover_audit_id, sales_handover_fingerprint,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::business_adapter::ensure_separation_of_duties;
use erp_workflow::service::approval::policy::SeparationOfDutiesPolicy;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SalesOrderCommandProcess;
use super::authorization::SalesCommandAccess;
use super::command_event::{SalesCommandEvent, finish_receipt_recovery, load_command_receipt};
use crate::handover_common::ensure_org_enabled;
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
        let expected_key_hash = CommandFingerprint::from_parts([idempotency_key.clone()]);
        if let Some(existing) =
            self.replay_sales_handover(&audit_id, &fingerprint, &expected_key_hash, id, actor).await?
        {
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

        let transaction_result = handover_transaction(
            self.db.clone(),
            SalesHandoverWrite {
                actor: actor.clone(),
                request: req.clone(),
                order_id: id.to_string(),
                fingerprint: fingerprint.clone(),
                command_id: audit_id.clone(),
                target: target.clone(),
                access: access.clone(),
                expected_version: req.expected_version,
                expected_key_hash: expected_key_hash.clone(),
                reason,
            },
        )
        .await;
        match transaction_result {
            Ok(Some(view)) => Ok(view),
            Ok(None) => self
                .replay_sales_handover(&audit_id, &fingerprint, &expected_key_hash, id, actor)
                .await?
                .ok_or_else(|| Error::Internal("销售交接幂等收据缺失".to_string())),
            Err(error) => finish_receipt_recovery(
                error,
                self.replay_sales_handover(&audit_id, &fingerprint, &expected_key_hash, id, actor).await,
            ),
        }
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
            let mut planned = order.clone();
            planned.sales_owner_user_id = account.base.id.clone();
            if !target_can_read_order(&self.db, &rbac, &account.base.id, &planned, &mut NoTransaction).await?
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

    /// 查证独立回执并返回当前责任与开放任务，不执行第二次交接。
    async fn replay_sales_handover(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        expected_key_hash: &CommandFingerprint,
        sales_order_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<HandoverSalesOrderView>> {
        let db = self.db.clone();
        let command_id = audit_id.to_string();
        let fingerprint = expected_fingerprint.to_string();
        let expected_key_hash = expected_key_hash.clone();
        let order_id = sales_order_id.to_string();
        let actor_id = actor.id().to_string();
        let access = self.command_access(actor, "update")?;
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_handover_with_executor(
                        &db,
                        &command_id,
                        (&fingerprint, &expected_key_hash),
                        &order_id,
                        &actor_id,
                        &access,
                        executor,
                    )
                    .await
                })
            })
            .await
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
/// 收拢原销售交接事务已有的请求、身份与授权上下文。
struct SalesHandoverWrite {
    actor: AuditActor,
    request: HandoverSalesOrderRequest,
    order_id: String,
    fingerprint: String,
    command_id: String,
    target: String,
    access: SalesCommandAccess,
    expected_version: u64,
    expected_key_hash: CommandFingerprint,
    reason: String,
}

/// 保持原事务先查证回执，再重验版本、交接与保存结果的顺序。
async fn handover_transaction(
    db: Database,
    input: SalesHandoverWrite,
) -> Result<Option<HandoverSalesOrderView>> {
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move {
                if let Some(view) = replay_handover_with_executor(
                    &db,
                    &input.command_id,
                    (&input.fingerprint, &input.expected_key_hash),
                    &input.order_id,
                    input.actor.id(),
                    &input.access,
                    executor,
                )
                .await?
                {
                    return Ok::<Option<HandoverSalesOrderView>, crate::Error>(Some(view));
                }
                input.access.revalidate(&input.order_id, input.expected_version, executor).await?;
                apply_handover(
                    &db,
                    &input.order_id,
                    &input.request,
                    &input.target,
                    &input.reason,
                    &input.actor,
                    &input.command_id,
                    &input.fingerprint,
                    executor,
                )
                .await?;
                Ok::<Option<HandoverSalesOrderView>, crate::Error>(None)
            })
        })
        .await
}

/// 只更新销售单负责人／组织与开放验收任务；审批、已完成与快照不变。
#[allow(clippy::too_many_arguments)]
async fn apply_handover(
    db: &mongodb::Database,
    id: &str,
    req: &HandoverSalesOrderRequest,
    target: &str,
    _reason: &str,
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
    if !target_can_read_order(db, &rbac, target, &order, executor).await? {
        return Err(Error::Forbidden("接收人无权读取交接后的销售单，请核对数据范围和个人上限".into()));
    }
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
    let audit = SalesCommandEvent::new(
        audit_id.to_string(),
        actor,
        &req.idempotency_key,
        fingerprint.to_string(),
        SalesCommandResult::HandedOver { sales_order_id: SalesOrderId::new(order.base.id.clone()) },
        order.order_no.clone(),
    )?;
    audit.persist(db, executor).await?;
    Ok(transferred)
}

/// 同执行器验证交接命令的完整身份，再读取当前责任及开放任务。
async fn replay_handover_with_executor(
    db: &mongodb::Database,
    command_id: &str,
    fingerprint: (&str, &CommandFingerprint),
    sales_order_id: &str,
    actor_id: &str,
    access: &super::authorization::SalesCommandAccess,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Option<HandoverSalesOrderView>> {
    let Some(receipt) = load_command_receipt(
        db,
        command_id,
        actor_id,
        "sales_order.handover",
        Some(sales_order_id),
        fingerprint,
        executor,
    )
    .await?
    else {
        return Ok(None);
    };
    if !matches!(receipt.result, SalesCommandResult::HandedOver { .. }) {
        return Err(Error::Internal("销售交接回执结果种类无效".to_string()));
    }
    let order = access.current(sales_order_id, executor).await?;
    let acceptance_ids = open_acceptance_task_ids(db, &order.base.id, executor).await?;
    let approval_assignees = open_approval_assignees(db, &order.base.id, executor).await?;
    Ok(Some(handover_view(&order, acceptance_ids, approval_assignees.len())))
}

/// 回放视图沿当前销售责任与开放任务构建，不冻结首次交接目标。
fn handover_view(
    order: &erp_sales::entity::sales_order::SalesOrder,
    acceptance_ids: Vec<String>,
    approval_task_count: usize,
) -> HandoverSalesOrderView {
    HandoverSalesOrderView {
        sales_order_id: order.base.id.clone(),
        sales_owner_user_id: order.sales_owner_user_id.clone(),
        business_org_unit_id: order.business_org_unit_id.clone(),
        version: order.base.version,
        transferred_acceptance_task_ids: acceptance_ids,
        kept_approval_task_count: approval_task_count,
    }
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

/// 用拟交接后的责任事实判断接收人的详情范围，不能使用旧负责人或旧组织。
async fn target_can_read_order(
    db: &mongodb::Database,
    rbac: &SharedRbacService,
    target: &str,
    planned: &erp_sales::entity::sales_order::SalesOrder,
    executor: &mut dyn persistence_core::Executor,
) -> Result<bool> {
    let Some(account) = db.accounts().find_by_id(target, executor).await? else {
        return Ok(false);
    };
    let actor = AuditActor::new(account.base.id.clone(), account.secret.account().to_string(), account.kind);
    let access = erp_read_models::sales_center::access::SalesAccess::new(db.clone(), rbac.clone());
    let (context, scope) = match access.resolve(&actor, "detail", &[], executor).await {
        Ok(value) => value,
        Err(erp_read_models::Error::Forbidden(_)) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    Ok(erp_read_models::sales_center::access::SalesAccess::allows(&context, &scope, planned)?)
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

    /// 实际回放视图保持当前责任和开放任务，不回写首次交接人。
    #[test]
    fn handover_replay_view_tracks_current_responsibility_and_open_tasks() {
        let mut order = sales_order_owned_by("sales-a", "org-a");
        order.handover("sales-b".into(), Some("org-b".into()), "manager").unwrap();
        let view = super::handover_view(&order, vec!["open-acceptance".into()], 2);
        assert_eq!(view.sales_owner_user_id, "sales-b");
        assert_eq!(view.business_org_unit_id, "org-b");
        assert_eq!(view.transferred_acceptance_task_ids, vec!["open-acceptance"]);
        assert_eq!(view.kept_approval_task_count, 2);
        assert_eq!(order.stable.created_by, "sales-a");
        order.handover("sales-c".into(), None, "manager").unwrap();
        let view = super::handover_view(&order, vec![], 1);
        assert_eq!(view.sales_owner_user_id, "sales-c");
        assert!(view.transferred_acceptance_task_ids.is_empty());
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
    #[test]
    fn recipient_scope_is_checked_on_new_owner_and_explicit_business_org() {
        use erp_core::common::time::Instant;
        use erp_identity::access_control::{ResolvedScope, ScopeClause};
        use erp_identity::service::access_control::resolve::AuthorizedDataScope;
        use erp_read_models::sales_center::access::SalesAccess;
        use erp_sales::repository::sales_order::scope::SalesReadScope;
        let mut access = AuthorizedDataScope {
            user_id: "recipient".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            role_scopes: Default::default(),
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v".into(),
            as_of: Instant::from_unix_secs(1),
        };
        let scope = SalesReadScope::default();
        let mut order = sales_order_owned_by("old-owner", "org-a");
        assert!(!SalesAccess::allows(&access, &scope, &order).unwrap());
        order.handover("recipient".into(), None, "operator").unwrap();
        assert!(SalesAccess::allows(&access, &scope, &order).unwrap());
        access.scope.user_limit =
            Some(ScopeClause { org_unit_ids: ["org-b".into()].into(), ..Default::default() });
        assert!(!SalesAccess::allows(&access, &scope, &order).unwrap());
        order.handover("recipient".into(), Some("org-b".into()), "operator").unwrap();
        assert!(SalesAccess::allows(&access, &scope, &order).unwrap());
    }
}
