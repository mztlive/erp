//! 销售责任显式交接与验收联动（S3-07）。
//!
//! 开放验收任务随交接原子转交；开放审批任务保持不变；已完成验收与历史
//! 归属快照不改写。业务组织不随接收人部门隐式变化，必须显式传入。

use std::sync::Arc;

use application_core::{AuditActor, CommandFingerprint};
use erp_audit::execution::recover_command;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use erp_identity::entity::authorization::{FactValue, ObjectFacts};
use erp_identity::repository::prelude::*;
use erp_identity::service::organization_access::ensure_org_enabled;
use erp_identity::{
    AccessControlExt, AccessTarget, AccountSummary, AuthorizationService, Error as IdentityError,
};
use erp_permissions::{internal as internal_permissions, sales_order as sales_order_permissions};
use erp_read_models::Error as ReadModelError;
use erp_read_models::sales_center::access::SalesAccess;
use erp_sales::dto::sales_order::{HandoverCandidateView, HandoverSalesOrderRequest, HandoverSalesOrderView};
use erp_sales::entity::command_receipt::SalesCommandResult;
use erp_sales::entity::sales_order::SalesOrder;
use erp_sales::service::sales_order::SalesOrderService;
use erp_sales::service::sales_order::command::identity::{
    sales_handover_audit_id, sales_handover_fingerprint,
};
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::business_adapter::ensure_separation_of_duties;
use erp_workflow::service::approval::policy::require_process_required;
use erp_workflow::{DocumentType, WorkItemExt};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SalesOrderCommandProcess;
use super::authorization::SalesCommandAccess;
use super::command_event::{SalesCommandEvent, load_command_receipt};
use crate::{Error, Result};

/// 客户验收完整执行权限的固定对象类型。
const ACCEPTANCE_OBJECT_TYPE: &str = "sales_order";

impl SalesOrderCommandProcess {
    /// 显式交接销售单当前负责人与业务组织，并原子转交开放验收任务。
    ///
    /// `req` 为交接请求（含期望版本、目标、原因与幂等键）。返回交接后责任与转交任务清单。
    ///
    /// 账号失效、动作权限不足、对象不可见、版本冲突、目标不合格（含与开放审批人岗位分离冲突）、异载荷同幂等键或组织停用时拒绝；失败零部分变更。
    ///
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
            return Err(Error::ValidationError("缺少本次操作编号，请重新打开当前页面后再办理".to_string()));
        }
        let reason = req.reason.trim().to_string();
        if reason.is_empty() {
            return Err(Error::ValidationError("交接原因不能为空".to_string()));
        }

        let target = req.target_owner_user_id.trim().to_string();
        let audit_id = sales_handover_audit_id(actor.id(), id, &idempotency_key);
        let fingerprint = sales_handover_fingerprint(actor.id(), id, &req)?;
        let expected_key_hash = CommandFingerprint::from_parts([idempotency_key.clone()]);

        if let Some(existing) =
            self.replay_sales_handover(&audit_id, &fingerprint, &expected_key_hash, id, actor).await?
        {
            return Ok(existing);
        }

        let access = self.command_access(actor, sales_order_permissions::HANDOVER);
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
                target_identity: self.authorization.clone(),
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
            Err(error) => recover_command(
                error,
                self.replay_sales_handover(&audit_id, &fingerprint, &expected_key_hash, id, actor).await,
            ),
        }
    }

    /// 查询当前销售单可交接的合格目标候选。返回有效且具备验收执行资格的账号；只作交互提示，提交仍重验。对象不可见或授权读取失败时拒绝。
    ///
    /// 不按部门相同与否过滤；同部门无资格不得接收，跨部门有资格正常接收；销售单允许负责人兼任已配置的审批人，审批任务保持原指派。
    pub async fn handover_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.handover_candidates_stage(&id, &actor, executor).await })
            })
            .await
    }

    async fn handover_candidates_stage(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<HandoverCandidateView>> {
        let access = self.command_access(actor, sales_order_permissions::HANDOVER);
        let order = access.current(id, executor).await?;

        let authorization = self.authorization.clone();
        let approvers = open_approval_assignees(&self.db, &order.base.id, executor).await?;
        let accounts = self.db.accounts().list_by_kind(AccountKind::Admin, executor).await?;
        let policy = require_process_required(DocumentType::SalesOrder)?;

        let mut candidates = Vec::new();
        for account in accounts {
            if account.base.id == order.sales_owner_user_id
                || ensure_separation_of_duties(
                    policy.separation_of_duties_policy,
                    &account.base.id,
                    &approvers,
                )
                .is_err()
            {
                continue;
            }

            let Some(qualified) =
                acceptance_qualification(&self.db, &authorization, &account.base.id, executor).await?
            else {
                continue;
            };

            let mut planned = order.clone();
            planned.sales_owner_user_id = account.base.id.clone();
            if !target_can_read_order(&self.db, &authorization, &account.base.id, &planned, executor).await? {
                continue;
            }
            candidates.push(HandoverCandidateView {
                user_id: qualified.id,
                display_name: qualified.display_name,
                account: qualified.login_account,
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

    // 查证独立回执并返回当前责任与开放任务，不执行第二次交接。
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
        let access = self.command_access(actor, sales_order_permissions::HANDOVER);
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
    target_identity: Arc<AuthorizationService>,
    expected_key_hash: CommandFingerprint,
    reason: String,
}

// 保持原事务先查证回执，再重验版本、交接与保存结果的顺序。
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
                    return Ok::<Option<HandoverSalesOrderView>, Error>(Some(view));
                }

                apply_handover(
                    &db,
                    &input.target_identity,
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
                Ok::<Option<HandoverSalesOrderView>, Error>(None)
            })
        })
        .await
}

// 只更新销售单负责人／组织与开放验收任务；审批、已完成与快照不变。
#[allow(clippy::too_many_arguments)]
async fn apply_handover(
    db: &Database,
    target_identity: &Arc<AuthorizationService>,
    id: &str,
    req: &HandoverSalesOrderRequest,
    target: &str,
    _reason: &str,
    actor: &AuditActor,
    audit_id: &str,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let mut order = prepare_handover_order(db, target_identity, id, req, target, actor, executor).await?;

    let mut acceptance_tasks = open_acceptance_tasks(db, id, executor).await?;
    let at = Instant::now();
    let mut transferred = Vec::new();
    for task in &mut acceptance_tasks {
        task.reassign(target.to_string(), at)
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        db.work_items().update(task, executor).await?;
        transferred.push(task.base.id.clone());
    }

    SalesOrderService::new(db.clone()).persist_order(&mut order, executor).await?;

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

// 在原快照先核验原对象，再检查版本、接收资格及拟交接后的双方范围。
async fn prepare_handover_order(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    id: &str,
    req: &HandoverSalesOrderRequest,
    target: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let access = SalesAccess::new(db.clone(), authorization.clone());
    let mut order = access.require_object(actor, &sales_order_permissions::HANDOVER, id, executor).await?;
    if order.base.version != req.expected_version {
        return Err(Error::ConflictError("销售单责任或版本已变化，请刷新后重试".into()));
    }
    if target == order.sales_owner_user_id {
        return Err(Error::BusinessLogicError("目标已是当前负责人，无需交接".into()));
    }

    ensure_target_qualified(db, authorization, target, executor).await?;
    ensure_org_enabled(db, req.target_business_org_unit_id.as_deref(), executor).await?;
    ensure_handover_separation(db, id, target, executor).await?;

    let next_org = req
        .target_business_org_unit_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    order.handover(target.to_string(), next_org, actor.id())?;

    access.require_loaded(actor, &sales_order_permissions::HANDOVER, &order, executor).await?;
    if !target_can_read_order(db, authorization, target, &order, executor).await? {
        return Err(Error::Forbidden("接收人无权读取交接后的销售单，请核对数据范围和个人上限".into()));
    }
    Ok(order)
}

// 同执行器验证交接命令的完整身份，再读取当前责任及开放任务。
async fn replay_handover_with_executor(
    db: &Database,
    command_id: &str,
    fingerprint: (&str, &CommandFingerprint),
    sales_order_id: &str,
    actor_id: &str,
    access: &super::authorization::SalesCommandAccess,
    executor: &mut dyn Executor,
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

// 回放视图沿当前销售责任与开放任务构建，不冻结首次交接目标。
fn handover_view(
    order: &SalesOrder,
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

// 校验目标账号有效且具备验收完整执行资格。
//
// `db` 为目标数据库；`authorization` 为共享授权服务；`target` 为目标负责销售；`executor` 为原交接事务执行器。
//
// 目标有效且具备验收读取与执行资格时成功。账号缺失、失效或缺少验收完整执行权限时拒绝。
//
// 只判定账号与权限事实；提交—审批分离另由 `ensure_handover_separation` 执行。
async fn ensure_target_qualified(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if acceptance_qualification(db, authorization, target, executor).await?.is_some() {
        return Ok(());
    }
    Err(Error::Forbidden("目标账号不存在、已失效或不具备销售验收读取与执行资格".to_string()))
}

// 按销售审批政策校验交接目标与开放审批任务的人员关系。
//
// `db` 为目标数据库；`sales_order_id` 为销售单 ID；`target` 为目标负责销售（交接后承担提交侧责任）；`executor` 为原交接事务执行器。
//
// 目标满足销售审批政策时成功，允许兼任已配置审批人。政策读取或审批任务查询失败时返回错误；失败不产生任何写入。
//
// 复用销售单类型审批政策；开放审批任务本身保持不变。
async fn ensure_handover_separation(
    db: &Database,
    sales_order_id: &str,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let approvers = open_approval_assignees(db, sales_order_id, executor).await?;
    let policy = require_process_required(DocumentType::SalesOrder)?;
    ensure_separation_of_duties(policy.separation_of_duties_policy, target, &approvers).map_err(|_| {
        Error::Forbidden("拟交接的负责人是当前审批人，不能同时负责提交和审批，请另选负责人".into())
    })?;
    Ok(())
}

// 沿原执行器取得验收完整资格与同次读取的目标账号摘要。
async fn acceptance_qualification(
    _db: &Database,
    authorization: &Arc<AuthorizationService>,
    target: &str,
    executor: &mut dyn Executor,
) -> Result<Option<AccountSummary>> {
    let actor = AuditActor::new(target.into(), target.into(), AccountKind::Admin);
    let candidate = ObjectFacts {
        resource: "work_item".into(),
        subject_type: FactValue::Known(WorkItemType::CustomerAcceptanceRegistration.as_str().into()),
        subject_source_resource: FactValue::Known(ACCEPTANCE_OBJECT_TYPE.into()),
        ..Default::default()
    };

    match authorization
        .require(
            &actor,
            &internal_permissions::WORK_ITEM_EXECUTION_QUALIFICATION,
            AccessTarget::Candidate(candidate),
            executor,
        )
        .await
    {
        Ok(()) => {},
        Err(IdentityError::Forbidden(_)) => return Ok(None),
        Err(error) => return Err(error.into()),
    }

    Ok(authorization.permissions(&actor, &[], executor).await?.map(|qualified| qualified.account().clone()))
}

// 用拟交接后的责任事实判断接收人的详情范围，不能使用旧负责人或旧组织。
async fn target_can_read_order(
    db: &Database,
    authorization: &Arc<AuthorizationService>,
    target: &str,
    planned: &SalesOrder,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let actor = AuditActor::new(target.into(), target.into(), AccountKind::Admin);
    let access = SalesAccess::new(db.clone(), authorization.clone());
    match access
        .require_loaded(&actor, &internal_permissions::SALES_ORDER_HANDOVER_RECIPIENT, planned, executor)
        .await
    {
        Ok(()) => Ok(true),
        Err(ReadModelError::Forbidden(_) | ReadModelError::NotFound(_)) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

// 销售责任交接时，按销售单读取开放待办并筛出可随单移交的验收任务。
async fn open_acceptance_tasks(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<WorkItem>> {
    Ok(db
        .work_items()
        .list_active_by_object(ACCEPTANCE_OBJECT_TYPE, sales_order_id, executor)
        .await?
        .into_iter()
        .filter(|item| is_handover_transferable_type(item.work_item_type))
        .collect())
}

// 判定任务类型是否随销售交接转交。
//
// 仅客户验收任务随交接转交；审批任务、交付与其他任务类型保持不变，由各自领域命令处理。调用方传入的已是开放任务，存续状态由 `list_active_by_object` 保证，本判定只做类型分流。
//
// `work_item_type` 为待判定任务的固定类型。客户验收任务返回 `true`，其他类型返回 `false`。
fn is_handover_transferable_type(work_item_type: WorkItemType) -> bool {
    work_item_type == WorkItemType::CustomerAcceptanceRegistration
}

// 列出开放验收任务 ID（幂等回放用，不校验身份）。
async fn open_acceptance_task_ids(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    Ok(open_acceptance_tasks(db, sales_order_id, executor)
        .await?
        .into_iter()
        .map(|item| item.base.id)
        .collect())
}

// 列出该销售单全部开放审批任务的当前审批人。
//
// `db` 为目标数据库；`sales_order_id` 为销售单 ID；沿用调用方的执行器。
//
// 返回开放 `DocumentApproval` 任务的 `owner_user_id` 清单。任务读取失败时返回错误。
//
// 只读审批任务用于岗位分离校验、候选过滤与随转计数，不得改写。
async fn open_approval_assignees(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
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
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use erp_workflow::entity::work_item::WorkItemType;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use crate::Error;

    #[tokio::test]
    async fn acceptance_candidate_preserves_authorization_failures_before_reading_account_summary() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("unit_test");
        let error = super::acceptance_qualification(
            &db,
            &unavailable_authorization(),
            "sales",
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
    use erp_core::ids::{ContractId, CustomerAccountId, PartyId, SalesOrderId};
    use erp_sales::entity::sales_order::{
        BusinessType, CommercialStatus, OriginSystem, SalesOrder, SalesOrderData,
    };

    use super::{
        DocumentType, ensure_separation_of_duties, is_handover_transferable_type, require_process_required,
    };

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

    // 实际回放视图保持当前责任和开放任务，不回写首次交接人。
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

    // 仅开放验收任务随交接转交；审批与其他任务类型保持不变。
    #[test]
    fn handover_transfers_only_acceptance_task_type() {
        assert!(is_handover_transferable_type(WorkItemType::CustomerAcceptanceRegistration));
        assert!(!is_handover_transferable_type(WorkItemType::DocumentApproval));
    }

    // 交接实体只改负责人与组织，不改归属快照与状态；显式组织留空保留。
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

    // 失败交接零部分变更：责任、组织与版本保持原值（原子性）。
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

    // 销售交接按同一审批政策允许目标兼任该单已配置审批人。
    #[test]
    fn handover_target_may_be_a_configured_approver() {
        let policy = require_process_required(DocumentType::SalesOrder).unwrap().separation_of_duties_policy;
        assert!(ensure_separation_of_duties(policy, "sales-b", &[]).is_ok());
        assert!(ensure_separation_of_duties(policy, "sales-b", &["approver-1".to_string()]).is_ok());
        assert!(
            ensure_separation_of_duties(
                policy,
                "approver-1",
                &["approver-1".to_string(), "approver-2".to_string()]
            )
            .is_ok()
        );
    }

    #[tokio::test]
    async fn handover_stops_at_authorization_before_loading_version_or_target_state() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("handover_authorization");
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let request = super::HandoverSalesOrderRequest {
            expected_version: 0,
            target_owner_user_id: "actor".into(),
            target_business_org_unit_id: None,
            reason: "交接".into(),
            idempotency_key: "handover".into(),
        };

        let error = super::prepare_handover_order(
            &db,
            &unavailable_authorization(),
            "order",
            &request,
            "actor",
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
    }
}
