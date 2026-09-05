//! 开放任务转交、候选人资格与岗位分离。

use std::collections::HashSet;

use database::{
    AccessControlExt, Executor, InventoryExt, MongoCasbinAdapter, NoTransaction, PurchaseOrderExt,
    ReceivableExt, SalesOrderExt, SalesReviewExt, SupplierSettlementExt, WorkItemExt,
};
use entities::common::time::Instant;
use entities::{
    work_item::{
        AvailableWorkItemAccount, FulfillmentResponsibilityKey, WorkItem, WorkItemAssignmentSeparationPolicy,
        WorkItemType,
    },
    CommandReceipt, Permission,
};
use mongodb::Database;
use validator::Validate;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    iam::SharedRbacService,
};

use super::access::{
    active_role_ids, ensure_generic_work_item_mutation, ensure_item_in_managed_scope, ensure_managed_access,
    has_assignment_candidate_access, ActorAccess, MANAGE_PERMISSION,
};
use super::facts::{object_policy, SYSTEM_OBJECT_OWNER};
use super::write::{expected_task_version, required_text, WorkItemWriteOutcome, IDEMPOTENCY_AUDIT_PREFIX};
use super::{
    ReassignWorkItemRequest, WorkItemConflictKind, WorkItemMutationOutcome, WorkItemReassignCandidateView,
    WorkItemService,
};

const REASSIGN_VERSION_CONFLICT: &str = "任务版本已变化";
const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

struct AssignmentAuthorizationSnapshot {
    policy_revision: u64,
    actor_kind: entities::AccountKind,
    assignee_kind: entities::AccountKind,
    read_permission: Permission,
    actor_read_role_ids: Vec<String>,
    actor_manage_role_ids: Vec<String>,
    assignee_read_role_ids: Vec<String>,
    assignee_permissions: Vec<Permission>,
}

/// 转交命令的任务、目标责任人与审计输入。
///
/// # 用途
/// 将转交事务所需字段打包，供 [`WorkItemService::reassign_with_assignment_policy_audit`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 事务内必须重验管理人、目标责任人与授权快照。
struct AssignmentPolicyAuditInput<'a> {
    /// 待转交任务。
    item: WorkItem,
    /// 期望任务版本。
    expected_task_version: u64,
    /// 目标责任人。
    target_user_id: String,
    /// 操作人。
    actor: &'a AuditActor,
    /// 强类型幂等命令收据。
    receipt: CommandReceipt,
    /// 权限安全的转交说明。
    audit_detail: String,
    /// 事务外冻结的授权快照。
    authorization: AssignmentAuthorizationSnapshot,
}

impl WorkItemService {
    /// 查询当前开放非审批任务可转交的具体账号。
    ///
    /// # 参数
    /// * `id` - 工作项稳定 ID
    /// * `actor` - 已通过鉴权且具有责任管理范围的操作人
    ///
    /// # 返回
    /// 返回当前仍有效、具备完整执行权限且满足任务责任约束的账号。
    ///
    /// # 错误
    /// 任务不存在、不是开放非审批任务、操作人不在管理范围，或授权事实读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 采购单责任任务的候选人必须同时能够执行该采购单全部开放履约任务；列表只作交互提示，
    /// 最终转交命令仍须在写事务内重验全部账号、授权与业务事实。
    pub async fn reassign_candidates(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<WorkItemReassignCandidateView>> {
        let managed_access = self.managed_access(actor).await?;
        let item = self.load(id).await?;
        ensure_generic_work_item_mutation(&item)?;
        ensure_item_in_managed_scope(&item, &managed_access)?;

        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let cascade_tasks = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
            let (_, tasks) =
                load_purchase_order_fulfillment_scope(&self.db, &item, purchase_order_id, &mut NoTransaction)
                    .await?;
            Some(tasks)
        } else {
            None
        };

        let accounts = self
            .db
            .accounts()
            .list_by_kind(entities::AccountKind::Admin, &mut NoTransaction)
            .await?;
        let mut candidates = Vec::new();
        for account in accounts {
            if item.owner_user_id.as_deref() == Some(account.base.id.as_str())
                || AvailableWorkItemAccount::from_account(&account).is_err()
            {
                continue;
            }
            let authorization = match self
                .assignment_authorization_snapshot(actor, &account.base.id, &item, true)
                .await
            {
                Ok(authorization) => authorization,
                Err(Error::Forbidden(_)) => continue,
                Err(error) => return Err(error),
            };
            if let Some(tasks) = cascade_tasks.as_deref() {
                match ensure_fulfillment_tasks_candidate(
                    self,
                    tasks,
                    &account.base.id,
                    &authorization.assignee_permissions,
                    &mut NoTransaction,
                )
                .await
                {
                    Ok(()) => {}
                    Err(Error::Forbidden(_)) => continue,
                    Err(error) => return Err(error),
                }
            }
            let login_account = account.secret.account().to_string();
            candidates.push(WorkItemReassignCandidateView {
                user_id: account.base.id,
                display_name: account.name,
                account: login_account,
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

    /// 受控转交开放任务。
    ///
    /// # 错误
    /// 缺少任务管理权限、目标资格无法证明、审批受阻或任务版本陈旧时返回错误。
    pub async fn reassign(
        &self,
        id: &str,
        req: ReassignWorkItemRequest,
        actor: &AuditActor,
    ) -> Result<WorkItemMutationOutcome> {
        let managed_access = self.managed_access(actor).await?;
        let item = self.load(id).await?;
        ensure_generic_work_item_mutation(&item)?;
        req.validate()?;
        let idempotency_key = required_text(&req.idempotency_key, "幂等键不能为空")?;
        let action = "work_item.reassign";
        let target_user_id = required_text(&req.target_user_id, "目标用户不能为空")?;
        let reason = required_text(&req.reason, "转交原因不能为空")?;
        let expected_task_version = expected_task_version(&req.expected_task_version)?;
        let version = expected_task_version.to_string();
        let receipt = CommandReceipt::from_resource_parts(
            IDEMPOTENCY_AUDIT_PREFIX,
            actor.id(),
            action,
            "work_item",
            id,
            &idempotency_key,
            [version, target_user_id.clone(), reason.clone()],
        )?;
        if let Some(replayed) = self.idempotent_replay(&receipt, id).await? {
            ensure_generic_work_item_mutation(&replayed)?;
            return self.applied_outcome(replayed, actor).await;
        }
        if item.base.version != expected_task_version {
            return self
                .conflict_outcome(id, WorkItemConflictKind::Version, actor)
                .await;
        }
        ensure_item_in_managed_scope(&item, &managed_access)?;
        let authorization = self
            .assignment_authorization_snapshot(actor, &target_user_id, &item, true)
            .await?;
        let updated = self
            .reassign_with_assignment_policy_audit(AssignmentPolicyAuditInput {
                item,
                expected_task_version,
                target_user_id,
                actor,
                receipt,
                audit_detail: reason,
                authorization,
            })
            .await?;
        match updated {
            WorkItemWriteOutcome::Updated(item) => self.applied_outcome(*item, actor).await,
            WorkItemWriteOutcome::VersionConflict => {
                self.conflict_outcome(id, WorkItemConflictKind::Version, actor)
                    .await
            }
        }
    }

    /// 形成事务外的授权版本锚点；事务内仍会重新读取全部资格与业务事实。
    async fn assignment_authorization_snapshot(
        &self,
        actor: &AuditActor,
        assignee_id: &str,
        item: &WorkItem,
        require_manager: bool,
    ) -> Result<AssignmentAuthorizationSnapshot> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let read_permission = Permission::parse(policy.read_permission).expect("责任策略权限必须合法");
        let manage_permission = Permission::parse(MANAGE_PERMISSION).expect("固定权限合法");
        let adapter = MongoCasbinAdapter::new(self.db.clone());
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = adapter.policy_revision(&mut NoTransaction).await?;
            let actor_role_ids =
                active_role_ids(&self.db, actor.kind(), actor.id(), &mut NoTransaction).await?;
            let actor_read_role_ids = self
                .roles_granting_permission(&actor_role_ids, &read_permission, true)
                .await?;
            let actor_manage_role_ids = self
                .roles_granting_permission(&actor_role_ids, &manage_permission, true)
                .await?;
            let assignee = self
                .db
                .accounts()
                .find_work_item_account(assignee_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee = AvailableWorkItemAccount::from_account(&assignee)
                .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
            let assignee_role_ids =
                active_role_ids(&self.db, assignee.kind(), assignee_id, &mut NoTransaction).await?;
            let assignee_permissions = self.rbac.permissions(assignee.kind(), assignee_id).await?;
            let assignee_read_role_ids = self
                .roles_granting_permission(&assignee_role_ids, &read_permission, true)
                .await?;
            let snapshot = AssignmentAuthorizationSnapshot {
                policy_revision: before,
                actor_kind: actor.kind(),
                assignee_kind: assignee.kind(),
                read_permission: read_permission.clone(),
                actor_read_role_ids,
                actor_manage_role_ids,
                assignee_read_role_ids,
                assignee_permissions,
            };
            self.ensure_assignment_actor_access(
                actor.kind(),
                actor.id(),
                item,
                require_manager,
                &snapshot,
                &mut NoTransaction,
            )
            .await?;
            self.ensure_assignment_candidate(
                assignee_id,
                assignee.kind(),
                item,
                &snapshot,
                item.owner_user_id.as_deref() == Some(assignee_id),
                &mut NoTransaction,
            )
            .await?;
            let after = adapter.policy_revision(&mut NoTransaction).await?;
            if before == after {
                return Ok(snapshot);
            }
        }
        Err(Error::Rbac(
            "授权策略持续变化，无法形成稳定的任务分派快照".to_string(),
        ))
    }

    /// 在调用方事务快照中重验操作人权限、管理范围与对象参与权。
    async fn ensure_assignment_actor_access(
        &self,
        actor_kind: entities::AccountKind,
        actor_id: &str,
        item: &WorkItem,
        require_manager: bool,
        authorization: &AssignmentAuthorizationSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(actor_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, actor_kind)
            .map_err(|_| Error::Forbidden("操作账号不存在、已停用或身份已变化".to_string()))?;
        let access = self
            .assignment_access_for_executor(
                actor_kind,
                actor_id,
                &authorization.read_permission,
                &authorization.actor_read_role_ids,
                &authorization.actor_manage_role_ids,
                executor,
            )
            .await?;
        if require_manager {
            ensure_managed_access(&access)?;
            ensure_item_in_managed_scope(item, &access)?;
        }
        self.ensure_item_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("当前账号无权处理该业务对象".to_string()))
    }

    /// 在调用方事务快照中重验目标账号资格、对象访问权与岗位分离。
    ///
    /// # 参数
    /// * `user_id` - 待接收任务的具体账号 ID
    /// * `expected_kind` - 事务外授权快照冻结的账号类型
    /// * `item` - 待转交任务
    /// * `authorization` - 事务外形成的稳定授权快照
    /// * `allow_current_owner` - 是否允许目标账号保持为当前负责人
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 目标账号仍有效、具备任务所需权限且满足岗位分离时返回 `Ok(())`。
    ///
    /// # 错误
    /// 账号失效、权限撤销、对象版本变化或岗位分离不满足时返回错误。
    async fn ensure_assignment_candidate(
        &self,
        user_id: &str,
        expected_kind: entities::AccountKind,
        item: &WorkItem,
        authorization: &AssignmentAuthorizationSnapshot,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let account = self
            .db
            .accounts()
            .find_work_item_account(user_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        AvailableWorkItemAccount::from_account_kind(&account, expected_kind)
            .map_err(|_| Error::Forbidden("目标账号不存在或已失效".to_string()))?;
        let mut access = self
            .assignment_access_for_executor(
                expected_kind,
                user_id,
                &authorization.read_permission,
                &authorization.assignee_read_role_ids,
                &[],
                executor,
            )
            .await?;
        if item.work_item_type.requires_full_execution_permissions() {
            // 执行任务除注册表读取权限外，还要求目标工作面使用的完整操作权限。
            // 该快照由同一 policy revision 形成，外层授权事务会以该 revision
            // 做 CAS 后才允许提交。
            access.permissions = authorization.assignee_permissions.clone();
        }
        self.ensure_assignment_candidate_access_with_executor(item, &access, executor)
            .await
            .map_err(|_| Error::Forbidden("目标账号不具备该业务对象的参与权或读取权".to_string()))?;
        self.ensure_assignment_separation(user_id, item, allow_current_owner, executor)
            .await
    }

    /// 审批任务在形成个人责任前排除启动人、既往责任人、前序和当前决定人。
    async fn ensure_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        match item.work_item_type.assignment_separation_policy() {
            AssignmentSeparationPolicy::ApprovalHistory => {
                self.ensure_approval_assignment_separation(user_id, item, allow_current_owner, executor)
                    .await
            }
            AssignmentSeparationPolicy::DomainActors => {
                let excluded = self.domain_assignment_actors(item, executor).await?;
                if excluded.iter().any(|actor_id| actor_id == user_id) {
                    return Err(Error::Forbidden("目标账号违反业务岗位分离约束".to_string()));
                }
                Ok(())
            }
            AssignmentSeparationPolicy::RoleAndParticipation => Ok(()),
            AssignmentSeparationPolicy::FailClosed => {
                Err(Error::Forbidden("任务类型未注册可证明的岗位分离策略".to_string()))
            }
        }
    }

    /// 审批步骤按实例启动人、业务提交人、责任历史和决定历史排除候选人。
    async fn ensure_approval_assignment_separation(
        &self,
        user_id: &str,
        item: &WorkItem,
        allow_current_owner: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let _ = (user_id, item, allow_current_owner, executor);
        Err(Error::Forbidden(
            "单据审批任务不得通过通用责任入口改派".to_string(),
        ))
    }

    /// 读取非审批正式决定任务的权威提交人、经办人及历史决定人。
    async fn domain_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        let actors = match item.work_item_type {
            WorkItemType::PurchaseOrderReview => {
                self.purchase_review_assignment_actors(item, executor).await?
            }
            WorkItemType::SalesChangeImpactReview | WorkItemType::SalesChangeFinanceReview => {
                self.sales_change_assignment_actors(item, executor).await?
            }
            WorkItemType::CardFundsReview | WorkItemType::CardFundsDeltaReview => {
                self.card_funds_assignment_actors(item, executor).await?
            }
            WorkItemType::InventoryAdjustmentReview => {
                self.inventory_assignment_actors(item, executor).await?
            }
            WorkItemType::SupplierSettlementReview => {
                self.settlement_assignment_actors(item, executor).await?
            }
            _ => {
                return Err(Error::Forbidden("任务类型未注册权威业务岗位分离事实".to_string()));
            }
        };
        non_empty_assignment_actors(actors)
    }

    /// 读取采购审核提交人和既往审核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 采购审核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 提交与任务对象一致时返回提交人和可选审核人。
    ///
    /// # 错误
    /// 提交缺失、对象关系不一致或提交人缺失时返回错误。
    async fn purchase_review_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .purchase_order_submissions()
            .find_work_item_purchase_submission(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("采购提交事实缺失".to_string()))?;
        if !item.matches_business_object("purchase_order", submission.purchase_order_id.as_ref()) {
            return Err(Error::Forbidden("采购提交与任务对象不一致".to_string()));
        }
        let submitted_by = submission
            .submitted_by
            .ok_or_else(|| Error::Forbidden("采购提交人事实缺失".to_string()))?;
        Ok(optional_actors([Some(submitted_by), submission.reviewed_by]))
    }

    /// 读取销售变更提交人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 销售变更复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 提交与任务对象一致时返回提交人账号 ID。
    ///
    /// # 错误
    /// 提交缺失、对象关系不一致或仓储查询失败时返回错误。
    async fn sales_change_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let submission = self
            .db
            .sales_change_submissions()
            .find_work_item_sales_change_submission(&item.subject_version, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("销售变更提交事实缺失".to_string()))?;
        if !item.matches_business_object("sales_change_review", submission.sales_change_order_id.as_ref()) {
            return Err(Error::Forbidden("销售变更提交与任务对象不一致".to_string()));
        }
        Ok(vec![submission.submitted_by])
    }

    /// 读取卡券票款任务的权威经办、登记与复核账号用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 卡券票款复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 返回应收、回款、发票与复核事实中的全部责任账号。
    ///
    /// # 错误
    /// 对象类型、状态、版本或票款正式事实无法证明时返回错误。
    async fn card_funds_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if item.business_object_type != "receivable_account" {
            return Err(Error::Forbidden("卡券票款任务责任事实不合法".to_string()));
        }
        let account = self
            .db
            .receivable_accounts()
            .find_work_item_receivable_account(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账事实缺失".to_string()))?;
        let expected_status = match item.work_item_type {
            WorkItemType::CardFundsReview => entities::receivable::AccountReviewStatus::OpeningPending,
            WorkItemType::CardFundsDeltaReview => entities::receivable::AccountReviewStatus::SyncDeltaPending,
            _ => return Err(Error::Forbidden("任务类型不是卡券票款复核".to_string())),
        };
        if account.review_status != expected_status {
            return Err(Error::Forbidden("应收子账已不在当前票款复核状态".to_string()));
        }
        self.ensure_current_card_funds_subject(item, &account, executor)
            .await?;
        let account_id = entities::ids::ReceivableAccountId::new(account.base.id.clone());
        let entries = self
            .db
            .receivable_entries()
            .find_entries_by_account(&account_id, executor)
            .await?;
        let entry_ids = entries
            .iter()
            .map(|entry| entities::ids::ReceivableEntryId::new(entry.base.id.clone()))
            .collect::<Vec<_>>();
        let receipt_allocations = self
            .db
            .receipt_allocations()
            .find_allocations_by_entries(&entry_ids, executor)
            .await?;
        let invoice_allocations = self
            .db
            .sales_invoice_allocations()
            .find_allocations_by_accounts(&[account_id], executor)
            .await?;
        let receipt_ids = receipt_allocations
            .into_iter()
            .map(|allocation| allocation.customer_receipt_id.to_string())
            .collect::<HashSet<_>>();
        let invoice_ids = invoice_allocations
            .into_iter()
            .map(|allocation| allocation.invoice_id.to_string())
            .collect::<HashSet<_>>();
        let receipts = self
            .card_funds_receipts(&receipt_ids, &account.counterparty_party_id, executor)
            .await?;
        let invoices = self
            .card_funds_invoices(&invoice_ids, &account.counterparty_party_id, executor)
            .await?;
        let reviews = self
            .db
            .receivable_funds_reviews()
            .find_reviews_by_account(
                &entities::ids::ReceivableAccountId::new(account.base.id.clone()),
                executor,
            )
            .await?;
        let mut actors = optional_actors([Some(account.stable.created_by), account.reviewed_by]);
        actors.extend(reviews.into_iter().map(|review| review.reviewed_by));
        actors.extend(
            self.card_funds_audit_actors(
                "customer_receipt",
                &receipts,
                &["customer_receipt.create", "customer_receipt.post:"],
                &["customer_receipt.post:"],
                executor,
            )
            .await?,
        );
        actors.extend(
            self.card_funds_audit_actors(
                "invoice",
                &invoices,
                &["invoice.create", "invoice.post", "invoice.red_issue"],
                &["invoice.post", "invoice.red_issue"],
                executor,
            )
            .await?,
        );
        Ok(actors)
    }

    /// 锁定应收来源销售单的当前正式版本，拒绝把旧版本任务重新形成个人责任。
    async fn ensure_current_card_funds_subject(
        &self,
        item: &WorkItem,
        account: &entities::receivable::ReceivableAccount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let order = self
            .db
            .sales_orders()
            .find_work_item_sales_order(account.sales_order_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单事实缺失".to_string()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单缺少当前正式版本".to_string()))?;
        if !item.matches_subject_version(&revision_id) {
            return Err(Error::Forbidden("票款复核任务已不是当前销售版本".to_string()));
        }
        self.db
            .sales_order_revisions()
            .find_work_item_sales_order_revision(&revision_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("应收子账来源销售单当前正式版本事实缺失".to_string()))?;
        Ok(())
    }

    /// 读取当前票款快照引用的正式回款；缺失、未过账或主体不一致均失败关闭。
    async fn card_funds_receipts(
        &self,
        ids: &HashSet<String>,
        party_id: &entities::ids::PartyId,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let receipts = self
            .db
            .customer_receipts()
            .list_work_item_brief_entities_by_ids(&ids.iter().cloned().collect::<Vec<_>>(), executor)
            .await?;
        if receipts.len() != ids.len()
            || receipts.iter().any(|receipt| {
                !matches!(
                    receipt.status,
                    entities::receivable::CustomerReceiptStatus::Posted
                        | entities::receivable::CustomerReceiptStatus::Reversed
                ) || &receipt.counterparty_party_id != party_id
            })
        {
            return Err(Error::Forbidden(
                "票款复核引用的回款事实缺失、未正式过账或往来主体不一致".to_string(),
            ));
        }
        Ok(receipts.into_iter().map(|receipt| receipt.base.id).collect())
    }

    /// 读取当前票款快照引用的正式销项发票；缺失、未登记或主体不一致均失败关闭。
    async fn card_funds_invoices(
        &self,
        ids: &HashSet<String>,
        party_id: &entities::ids::PartyId,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        let invoices = self
            .db
            .invoices()
            .list_work_item_brief_entities_by_ids(&ids.iter().cloned().collect::<Vec<_>>(), executor)
            .await?;
        if invoices.len() != ids.len()
            || invoices.iter().any(|invoice| {
                invoice.invoice_direction != entities::receivable::InvoiceDirection::Sales
                    || !matches!(
                        invoice.stable.status(),
                        entities::receivable::InvoiceStatus::Registered
                            | entities::receivable::InvoiceStatus::RedInvoiced
                    )
                    || &invoice.party_id != party_id
            })
        {
            return Err(Error::Forbidden(
                "票款复核引用的发票事实缺失、未正式登记或往来主体不一致".to_string(),
            ));
        }
        Ok(invoices.into_iter().map(|invoice| invoice.base.id).collect())
    }

    /// 从成功审计证明每个票款事实已正式登记，并返回其全部登记经办人。
    async fn card_funds_audit_actors(
        &self,
        resource_type: &str,
        resource_ids: &HashSet<String>,
        operator_actions: &[&str],
        formal_actions: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        if resource_ids.is_empty() {
            return Ok(Vec::new());
        }
        let resource_id_list = resource_ids.iter().cloned().collect::<Vec<_>>();
        let audits = self
            .db
            .audit_logs()
            .list_successful_work_item_fact_audits(resource_type, &resource_id_list, executor)
            .await?;
        audited_fact_operator_actors(
            resource_type,
            resource_ids,
            &audits,
            operator_actions,
            formal_actions,
        )
    }

    /// 读取库存调整任务的制单人与既往复核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 库存调整复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 返回存在的制单、业务复核与财务复核账号 ID。
    ///
    /// # 错误
    /// 库存调整事实缺失或仓储查询失败时返回错误。
    async fn inventory_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let adjustment = self
            .db
            .stock_adjustments()
            .find_work_item_stock_adjustment(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("库存调整事实缺失".to_string()))?;
        Ok(optional_actors([
            Some(adjustment.prepared_by),
            adjustment.reviewed_by,
            adjustment.finance_reviewed_by,
        ]))
    }

    /// 读取供应商结算任务的制单人与既往复核人用于岗位分离。
    ///
    /// # 参数
    /// * `item` - 供应商结算复核工作项
    /// * `executor` - 当前责任变更事务执行器
    ///
    /// # 返回
    /// 任务版本仍匹配时返回存在的制单与复核账号 ID。
    ///
    /// # 错误
    /// 结算事实缺失、版本不一致或仓储查询失败时返回错误。
    async fn settlement_assignment_actors(
        &self,
        item: &WorkItem,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let statement = self
            .db
            .supplier_settlement_statements()
            .find_work_item_supplier_settlement(&item.business_object_id, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("供应商结算事实缺失".to_string()))?;
        if !item.matches_subject_version(&statement.subject_hash) {
            return Err(Error::Forbidden("供应商结算版本与任务不一致".to_string()));
        }
        Ok(optional_actors([
            Some(statement.prepared_by),
            statement.reviewed_by,
        ]))
    }

    /// 使用调用方 executor 重验转交目标的对象访问条件。
    ///
    /// # 参数
    /// * `item` - 待转交任务
    /// * `access` - 由当前角色与权限形成的目标账号访问快照
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 对象存在、版本匹配且目标账号满足任务类型的访问条件时返回 `Ok(())`。
    ///
    /// # 错误
    /// 对象未注册、不存在、版本变化或目标账号访问条件不足时返回错误。
    ///
    /// # 关键业务约束
    /// 供给分配任务按具体账号和 `purchase_order:create` 授权，不额外引入团队池或固定角色约束。
    async fn ensure_assignment_candidate_access_with_executor(
        &self,
        item: &WorkItem,
        access: &ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let policy = object_policy(item.work_item_type, &item.business_object_type)
            .ok_or_else(|| Error::Forbidden("任务类型未注册责任策略".to_string()))?;
        let keys = HashSet::from([(policy.object_kind, item.business_object_id.clone())]);
        let facts = self.load_object_facts(&keys, executor).await?;
        if !has_assignment_candidate_access(item, access, &facts) {
            return Err(Error::Forbidden("业务对象不可访问".to_string()));
        }
        Ok(())
    }

    /// 在同一事务内重验管理人、目标责任人及全部业务事实后执行转交与审计。
    ///
    /// # 用途
    /// 按授权快照转交任务并写入幂等审计。
    ///
    /// # 参数
    /// * `input` - 任务、目标责任人与审计字段
    ///
    /// # 返回
    /// 返回写入结果或版本冲突。
    ///
    /// # 错误
    /// 授权变化、版本冲突或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 事务内必须重验管理人和目标责任人，并以授权快照版本执行 policy CAS 后才可提交。
    async fn reassign_with_assignment_policy_audit(
        &self,
        input: AssignmentPolicyAuditInput<'_>,
    ) -> Result<WorkItemWriteOutcome> {
        let AssignmentPolicyAuditInput {
            item,
            expected_task_version,
            target_user_id,
            actor,
            receipt,
            audit_detail,
            authorization,
        } = input;
        let replay_receipt = receipt.clone();
        let replay_item_id = item.base.id.clone();
        let purchase_order_id = purchase_order_fulfillment_responsibility_id(&item)?;
        let source_user_id = item.owner_user_id.as_deref().unwrap_or("未指定").to_string();
        let selected_work_item_id = item.base.id.clone();
        let purchase_order_audit = purchase_order_id
            .as_ref()
            .map(|purchase_order_id| {
                actor.clone().resource_log_with_id(
                    format!("{}-purchase-order", receipt.id()),
                    "purchase_order.owner_reassign",
                    "purchase_order",
                    purchase_order_id.clone(),
                    Some(format!(
                        "source_user_id={source_user_id};target_user_id={target_user_id};cascade=open_fulfillment_tasks;selected_work_item_id={selected_work_item_id}"
                    )),
                )
            })
            .transpose()?;
        let audit = actor.clone().resource_log_with_id(
            receipt.id().to_string(),
            receipt.action(),
            receipt.resource_type(),
            item.base.id.clone(),
            Some(receipt.message(Some(&audit_detail))),
        )?;
        let item_id = item.base.id;
        let actor_id = actor.id().to_string();
        let actor_kind = actor.kind();
        let policy_revision = authorization.policy_revision;
        let policy_rbac = self.rbac.clone();
        let validation_rbac = policy_rbac.clone();
        let db = self.db.clone();
        let result = policy_rbac
            .run_authorized_policy_transaction(policy_revision, move |session| {
                Box::pin(async move {
                    let mut current = db
                        .work_items()
                        .find_work_item(&item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("任务不存在".to_string()))?;
                    if current.base.version != expected_task_version {
                        return Err(Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string()));
                    }
                    let allow_current_owner =
                        current.owner_user_id.as_deref() == Some(target_user_id.as_str());
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &validation_rbac,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner,
                        },
                        session,
                    )
                    .await?;
                    current = if let Some(purchase_order_id) = purchase_order_id.as_deref() {
                        reassign_purchase_order_fulfillment_responsibility(
                            &db,
                            &validation_rbac,
                            current,
                            purchase_order_id,
                            &target_user_id,
                            &actor_id,
                            &authorization,
                            session,
                        )
                        .await?
                    } else {
                        current.reassign(target_user_id.clone(), Instant::now())?;
                        db.work_items()
                            .update(&mut current, session)
                            .await
                            .map_err(|error| match error {
                                database::Error::OptimisticLockingError => {
                                    Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                                }
                                error => Error::from(error),
                            })?;
                        current
                    };
                    ensure_assignment_policy_in_transaction(
                        &db,
                        &validation_rbac,
                        AssignmentPolicyCheck {
                            actor_kind,
                            actor_id: &actor_id,
                            assignee_id: &target_user_id,
                            item: &current,
                            require_manager: true,
                            authorization: &authorization,
                            allow_current_owner: true,
                        },
                        session,
                    )
                    .await?;
                    if let Some(purchase_order_audit) = &purchase_order_audit {
                        db.audit_logs().create(purchase_order_audit, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok(current)
                })
            })
            .await;
        match result {
            Ok(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
            Err(Error::ConflictError(message)) if message == REASSIGN_VERSION_CONFLICT => {
                Ok(WorkItemWriteOutcome::VersionConflict)
            }
            Err(error) => match self.idempotent_replay(&replay_receipt, &replay_item_id).await? {
                Some(item) => Ok(WorkItemWriteOutcome::Updated(Box::new(item))),
                None => Err(error),
            },
        }
    }
}

/// 事务内分派策略重验输入。
///
/// # 用途
/// 将操作人、候选人与授权快照打包，供 [`ensure_assignment_policy_in_transaction`] 使用。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 操作人身份必须与授权快照一致。
struct AssignmentPolicyCheck<'a> {
    /// 操作人账号类型。
    actor_kind: entities::AccountKind,
    /// 操作人 ID。
    actor_id: &'a str,
    /// 目标责任人 ID。
    assignee_id: &'a str,
    /// 当前任务。
    item: &'a WorkItem,
    /// 是否要求管理人权限。
    require_manager: bool,
    /// 事务外冻结的授权快照。
    authorization: &'a AssignmentAuthorizationSnapshot,
    /// 是否允许候选人为当前责任人。
    allow_current_owner: bool,
}

/// 在任务责任事务内重放全部固定分派策略。
///
/// # 用途
/// 重验操作人授权与候选人资格后再允许写入。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `check` - 分派策略重验输入
/// * `executor` - 事务执行器
///
/// # 返回
/// 策略仍成立时返回 `Ok(())`。
///
/// # 错误
/// 身份变化、授权不足或候选人非法时返回错误。
///
/// # 关键业务约束
/// 必须在同一任务责任事务内调用。
async fn ensure_assignment_policy_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    check: AssignmentPolicyCheck<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if check.actor_kind != check.authorization.actor_kind {
        return Err(Error::Forbidden("操作账号身份已变化".to_string()));
    }
    let service = WorkItemService::new(db.clone(), rbac.clone());
    service
        .ensure_assignment_actor_access(
            check.actor_kind,
            check.actor_id,
            check.item,
            check.require_manager,
            check.authorization,
            executor,
        )
        .await?;
    service
        .ensure_assignment_candidate(
            check.assignee_id,
            check.authorization.assignee_kind,
            check.item,
            check.authorization,
            check.allow_current_owner,
            executor,
        )
        .await
}

/// 解析采购单履约任务冻结的采购责任键。
///
/// # 参数
/// * `item` - 待转交工作项
///
/// # 返回
/// 非采购履约任务返回空；采购履约任务返回采购单 ID。
///
/// # 错误
/// 对象类型、责任角色、原因码或责任键不符合固定履约合同时返回错误。
pub(super) fn purchase_order_fulfillment_responsibility_id(item: &WorkItem) -> Result<Option<String>> {
    let key = item.fulfillment_responsibility_key().map_err(|_| {
        Error::BusinessLogicError(
            "履约任务的对象、责任角色、原因码或责任键不一致，请联系管理员修复后重试".to_string(),
        )
    })?;
    Ok(match key {
        Some(FulfillmentResponsibilityKey::PurchaseOrder(id)) => Some(id),
        Some(
            FulfillmentResponsibilityKey::WarehouseReceipt(_)
            | FulfillmentResponsibilityKey::WarehouseShip(_),
        )
        | None => None,
    })
}

/// 原子变更采购单当前责任人与其全部开放采购履约任务。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 授权服务
/// * `selected` - 管理员本次选中的开放任务
/// * `purchase_order_id` - 责任键解析出的采购单 ID
/// * `target_user_id` - 新采购责任人
/// * `actor_id` - 管理员账号 ID
/// * `authorization` - 事务外冻结的授权快照
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 返回已完成转交的选中任务。
///
/// # 错误
/// 采购单、任务集合或原责任不一致，目标缺少任一履约权限，或 CAS 写入失败时返回错误。
///
/// # 关键业务约束
/// 完成和关闭的历史任务保持不变；只有同一 `purchase_order:{id}` 下的开放履约任务级联。
#[allow(clippy::too_many_arguments)]
async fn reassign_purchase_order_fulfillment_responsibility(
    db: &Database,
    rbac: &SharedRbacService,
    selected: WorkItem,
    purchase_order_id: &str,
    target_user_id: &str,
    actor_id: &str,
    authorization: &AssignmentAuthorizationSnapshot,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let (mut order, mut tasks) =
        load_purchase_order_fulfillment_scope(db, &selected, purchase_order_id, executor).await?;
    ensure_fulfillment_tasks_candidate(
        &WorkItemService::new(db.clone(), rbac.clone()),
        &tasks,
        target_user_id,
        &authorization.assignee_permissions,
        executor,
    )
    .await
    .map_err(|_| {
        Error::Forbidden("目标账号缺少一个或多个开放履约任务所需权限，采购单责任未变更".to_string())
    })?;

    order.reassign_owner(target_user_id.to_string(), actor_id.to_string())?;
    db.purchase_orders()
        .update(&mut order, executor)
        .await
        .map_err(|error| match error {
            database::Error::OptimisticLockingError => {
                Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
            }
            error => Error::from(error),
        })?;

    let reassigned_at = Instant::now();
    let mut selected_after = None;
    for task in &mut tasks {
        task.reassign(target_user_id.to_string(), reassigned_at)?;
        db.work_items()
            .update(task, executor)
            .await
            .map_err(|error| match error {
                database::Error::OptimisticLockingError => {
                    Error::ConflictError(REASSIGN_VERSION_CONFLICT.to_string())
                }
                error => Error::from(error),
            })?;
        if task.base.id == selected.base.id {
            selected_after = Some(task.clone());
        }
    }
    selected_after.ok_or_else(|| Error::ConflictError("采购单开放履约任务已变化，请刷新后重试".to_string()))
}

/// 装载并校验采购单当前责任人与全部开放履约任务的一致范围。
async fn load_purchase_order_fulfillment_scope(
    db: &Database,
    selected: &WorkItem,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<(entities::purchase_order::PurchaseOrder, Vec<WorkItem>)> {
    let responsibility_key = format!("purchase_order:{purchase_order_id}");
    let order = db
        .purchase_orders()
        .find_by_id(
            &entities::ids::PurchaseOrderId::new(purchase_order_id.to_string()),
            executor,
        )
        .await?
        .ok_or_else(|| Error::BusinessLogicError("履约任务引用的采购单不存在".to_string()))?;
    if matches!(
        order.stable.status,
        entities::purchase_order::PurchaseOrderStatus::Completed
            | entities::purchase_order::PurchaseOrderStatus::Voided
    ) {
        return Err(Error::BusinessLogicError(
            "已完成或已作废采购单不能变更责任人".to_string(),
        ));
    }
    let original_owner = order.current_owner_user_id()?.to_string();
    if selected.owner_user_id.as_deref() != Some(original_owner.as_str()) {
        return Err(Error::ConflictError(
            "采购单责任人与当前履约任务责任不一致，请刷新责任事实后重试".to_string(),
        ));
    }
    let tasks = db
        .work_items()
        .list_open_fulfillment_by_responsibility_key(&responsibility_key, executor)
        .await?;
    if tasks.is_empty() || !tasks.iter().any(|task| task.base.id == selected.base.id) {
        return Err(Error::ConflictError(
            "采购单开放履约任务已变化，请刷新后重试".to_string(),
        ));
    }
    if tasks.iter().any(|task| {
        task.owner_user_id.as_deref() != Some(original_owner.as_str())
            || task.responsibility_key() != Some(responsibility_key.as_str())
            || !matches!(
                purchase_order_fulfillment_responsibility_id(task),
                Ok(Some(task_purchase_order_id)) if task_purchase_order_id == purchase_order_id
            )
    }) {
        return Err(Error::ConflictError(
            "采购单开放履约任务责任身份不一致，请联系管理员处理后重试".to_string(),
        ));
    }
    Ok((order, tasks))
}

/// 校验目标账号可执行给定全部开放履约任务。
async fn ensure_fulfillment_tasks_candidate(
    service: &WorkItemService,
    tasks: &[WorkItem],
    target_user_id: &str,
    permissions: &[Permission],
    executor: &mut dyn Executor,
) -> Result<()> {
    let target_access = ActorAccess {
        actor_id: target_user_id.to_string(),
        permissions: permissions.to_vec(),
        participant_document_ids: HashSet::new(),
        organization_ids: Vec::new(),
        responsibility_scopes: Vec::new(),
        can_manage: false,
    };
    for task in tasks {
        service
            .ensure_assignment_candidate_access_with_executor(task, &target_access, executor)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn approval_assignment_separated(
    candidate_id: &str,
    started_by: &str,
    submitted_by: &str,
    responsibility_actor_ids: &[String],
    current_owner_user_id: Option<&str>,
    allow_current_owner: bool,
    decided_by: &[&str],
) -> bool {
    if candidate_id == started_by || candidate_id == submitted_by {
        return false;
    }
    if responsibility_actor_ids.iter().any(|actor_id| {
        actor_id == candidate_id && !(allow_current_owner && current_owner_user_id == Some(candidate_id))
    }) {
        return false;
    }
    !decided_by.contains(&candidate_id)
}

/// 工作项责任形成使用的实体岗位分离策略别名。
pub(super) type AssignmentSeparationPolicy = WorkItemAssignmentSeparationPolicy;

fn optional_actors<const N: usize>(actors: [Option<String>; N]) -> Vec<String> {
    actors.into_iter().flatten().collect()
}

/// 按资源逐一证明正式票款审计，并返回创建、过账或红冲等经办人。
pub(super) fn audited_fact_operator_actors(
    resource_type: &str,
    resource_ids: &HashSet<String>,
    audits: &[entities::AuditLog],
    operator_actions: &[&str],
    formal_actions: &[&str],
) -> Result<Vec<String>> {
    let matches_action =
        |action: &str, prefixes: &[&str]| prefixes.iter().any(|prefix| action.starts_with(prefix));
    let mut actors = Vec::new();
    for resource_id in resource_ids {
        let facts = audits
            .iter()
            .filter(|audit| {
                audit.success
                    && audit.resource_type == resource_type
                    && audit.resource_id.as_deref() == Some(resource_id.as_str())
            })
            .collect::<Vec<_>>();
        if !facts
            .iter()
            .any(|audit| matches_action(&audit.action, formal_actions))
        {
            return Err(Error::Forbidden(
                "无法从审计事实证明票款已经正式登记，任务分派失败关闭".to_string(),
            ));
        }
        actors.extend(
            facts
                .into_iter()
                .filter(|audit| matches_action(&audit.action, operator_actions))
                .map(|audit| audit.actor_id.clone()),
        );
    }
    Ok(actors)
}

pub(super) fn non_empty_assignment_actors(actors: Vec<String>) -> Result<HashSet<String>> {
    let actors = actors
        .into_iter()
        .map(|actor| actor.trim().to_string())
        .filter(|actor| !actor.is_empty() && actor != SYSTEM_OBJECT_OWNER)
        .collect::<HashSet<_>>();
    if actors.is_empty() {
        return Err(Error::Forbidden(
            "任务岗位分离所需的权威责任人事实缺失".to_string(),
        ));
    }
    Ok(actors)
}
