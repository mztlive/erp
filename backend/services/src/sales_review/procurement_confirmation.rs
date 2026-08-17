// ---------------------------------------------------------------------
// 采购二次确认（W07）
// ---------------------------------------------------------------------

use database::{
    AccessControlExt, Executor, NoTransaction, SalesOrderExt, SalesReviewExt, Transactional, WorkItemExt,
};
use entities::ids::SalesOrderSubmissionId;
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationLine, ProcurementConfirmationLineData,
    ProcurementConfirmationStatus,
};
use entities::work_item::{WorkItem, WorkItemType};
use id_generator::next_id;
use serde::Serialize;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto;
use super::{
    PageView, ProcurementConfirmationActionBlockerView, ProcurementConfirmationAllowedAction,
    ProcurementConfirmationDetailParams, ProcurementConfirmationDetailView, ProcurementConfirmationFilter,
    ProcurementConfirmationLineView, ProcurementConfirmationListParams, ProcurementConfirmationView,
    SalesReviewService, SaveProcurementConfirmationLinesRequest, SaveProcurementConfirmationResult,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::{WorkItemAllowedAction, WorkItemService};

const SAVE_COMMAND_ACTION: &str = "procurement_confirmation.save";
pub(super) const DECISION_COMMAND_ACTION: &str = "procurement_confirmation.complete";

impl SalesReviewService {
    /// 分页查询采购确认队列。
    ///
    /// # 参数
    /// * `params` - 查询参数（`submission_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn procurement_confirmation_list(
        &self,
        params: &ProcurementConfirmationListParams,
    ) -> Result<PageView<ProcurementConfirmationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProcurementConfirmationFilter {
            submission_id: query.submission_id.map(SalesOrderSubmissionId::new),
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, dto::SortDir::Asc),
        };
        let page = self
            .db
            .procurement_confirmations()
            .search_procurement_confirmations(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProcurementConfirmationView {
                id: row.id,
                sales_order_id: row.sales_order_id,
                submission_id: row.submission_id,
                status: row.status,
                handled_by: row.handled_by,
                handled_at: row.handled_at,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询采购确认详情（批次 + 分行）。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `params` - 正式任务入口参数
    /// * `actor` - 当前已认证操作人
    /// * `rbac` - 共享授权服务
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次不存在
    pub async fn procurement_confirmation_detail(
        &self,
        id: &str,
        params: &ProcurementConfirmationDetailParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<ProcurementConfirmationDetailView> {
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        let lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let mut view = confirmation_detail_view(&confirmation, lines.clone());
        let Some(work_item_id) = params
            .work_item_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(view);
        };
        let formal = WorkItemService::new(self.db.clone(), rbac)
            .work_item_detail(work_item_id, actor)
            .await?;
        if formal.work_item_type != WorkItemType::ProcurementConfirmation
            || formal.business_object_type != "procurement_confirmation"
            || formal.business_object_id != confirmation.base.id
            || formal.subject_version != confirmation.submission_id.as_ref()
            || formal.approval_step_instance_id.is_some()
        {
            return Err(Error::BusinessLogicError(
                "正式任务与当前采购确认不匹配".to_string(),
            ));
        }
        view.work_item = Some(formal.clone());
        if !formal.allowed_actions.contains(&WorkItemAllowedAction::Process) {
            block_procurement_actions(
                &mut view,
                "START_PROCESSING_REQUIRED",
                "必须先从团队待办建立本人责任，才能执行采购确认动作",
            );
            return Ok(view);
        }

        let raw_work_item = self
            .db
            .work_items()
            .find_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认待办不存在".to_string()))?;
        if ensure_pending_confirmation(&confirmation).is_err() {
            block_procurement_actions(&mut view, "CONFIRMATION_NOT_PENDING", "采购确认已不是待处理状态");
            return Ok(view);
        }
        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
        if ensure_procurement_confirmation_actor_eligible(
            &self.db,
            &raw_work_item,
            &submission.submitted_by,
            actor.id(),
            &mut NoTransaction,
        )
        .await
        .is_err()
        {
            block_procurement_actions(
                &mut view,
                "ACTOR_INELIGIBLE_OR_SOD",
                "当前账号不再具备采购责任资格，或与销售提交人冲突",
            );
            return Ok(view);
        }

        view.allowed_actions
            .push(ProcurementConfirmationAllowedAction::Save);
        let order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        if order.commercial_status != entities::sales_order::CommercialStatus::PendingReview {
            push_procurement_blocker(
                &mut view,
                ProcurementConfirmationAllowedAction::Approve,
                "SALES_ORDER_NOT_AWAITING_CONFIRMATION",
                "销售单已不在待采购确认的审核状态",
            );
            push_procurement_blocker(
                &mut view,
                ProcurementConfirmationAllowedAction::Reject,
                "SALES_ORDER_NOT_AWAITING_CONFIRMATION",
                "销售单已不在待采购确认的审核状态",
            );
            return Ok(view);
        }
        view.allowed_actions
            .push(ProcurementConfirmationAllowedAction::Reject);

        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                std::slice::from_ref(&confirmation.submission_id),
                &mut NoTransaction,
            )
            .await?;
        if let Some((code, message)) = procurement_approval_projection_blocker(&lines, &submission_lines) {
            push_procurement_blocker(
                &mut view,
                ProcurementConfirmationAllowedAction::Approve,
                code,
                message,
            );
        } else {
            view.allowed_actions
                .push(ProcurementConfirmationAllowedAction::Approve);
        }
        Ok(view)
    }

    /// 以 W07 强类型非终结命令保存采购确认工作数据。
    ///
    /// 同一事务重验路径与对象身份、两个乐观锁版本、当前个人责任、角色/组织
    /// 资格及岗位分离，替换确认分行并同时推进确认编辑版本与任务活动版本。
    /// 审计记录是绑定操作人和完整载荷的稳定幂等收据；精确重试返回首次版本。
    ///
    /// # 错误
    /// 对象、责任或版本漂移返回冲突；资格或岗位分离不成立时失败关闭。
    pub async fn save_procurement_confirmation_lines(
        &self,
        id: &str,
        req: SaveProcurementConfirmationLinesRequest,
        actor: &AuditActor,
    ) -> Result<SaveProcurementConfirmationResult> {
        req.validate()?;
        ensure_command_confirmation_id(id, &req.action.confirmation_id)?;
        let expected_task_version = parse_task_version(&req.expected_task_version)?;
        let fingerprint = command_fingerprint(SAVE_COMMAND_ACTION, id, actor.id(), &req)?;
        let audit_id = command_audit_id(SAVE_COMMAND_ACTION, actor.id(), &req.idempotency_key);
        if let Some(result) = self.replay_save_command(&audit_id, id, &fingerprint).await? {
            return Ok(result);
        }

        let db = self.db.clone();
        let client = db.client().clone();
        let command = req.clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let confirmation_id = id.to_string();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut confirmation = db
                        .procurement_confirmations()
                        .find_by_id(&confirmation_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
                    ensure_pending_confirmation(&confirmation)?;
                    if confirmation.base.version != command.action.expected_edit_version {
                        return Err(Error::ConflictError(
                            "采购确认工作数据已变化，请刷新后重试".to_string(),
                        ));
                    }
                    ensure_submission_identity(
                        &confirmation,
                        &command.action.submission_id,
                        &command.expected_subject_version,
                    )?;
                    let mut work_item = load_procurement_confirmation_work_item(
                        &db,
                        ProcurementConfirmationTaskGuard::new(
                            &confirmation_id,
                            &confirmation.submission_id,
                            &command.work_item_id,
                            expected_task_version,
                            &command.expected_subject_version,
                            &actor_id,
                        ),
                        session,
                    )
                    .await?;
                    let submission = db
                        .sales_order_submissions()
                        .find_by_id(&confirmation.submission_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
                    ensure_procurement_confirmation_actor_eligible(
                        &db,
                        &work_item,
                        &submission.submitted_by,
                        &actor_id,
                        session,
                    )
                    .await?;
                    let lines = replace_pending_confirmation_lines(
                        &db,
                        &confirmation,
                        &command.action.lines,
                        session,
                    )
                    .await?;
                    let submission_lines = db
                        .sales_order_submission_lines()
                        .list_lines_by_submissions(std::slice::from_ref(&confirmation.submission_id), session)
                        .await?;
                    SalesReviewService::new(db.clone())
                        .ensure_confirmation_sources(&lines, &submission_lines, session)
                        .await?;
                    let now = entities::common::time::Instant::now();
                    confirmation.record_edit(&actor_id)?;
                    work_item.record_activity(&actor_id, now)?;
                    db.procurement_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    let receipt = SaveCommandReceipt {
                        edit_version: confirmation.base.version,
                        task_version: work_item.base.version,
                    };
                    let audit = actor_owned.resource_log_with_id(
                        audit_id_for_tx,
                        SAVE_COMMAND_ACTION,
                        "procurement_confirmation",
                        confirmation_id,
                        Some(save_receipt_message(&fingerprint_for_tx, receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SaveCommandReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;

        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self.replay_save_command(&audit_id, id, &fingerprint).await? {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        Ok(receipt.into_result())
    }

    /// 按稳定审计收据重放已成功的保存结果。
    async fn replay_save_command(
        &self,
        audit_id: &str,
        confirmation_id: &str,
        expected_fingerprint: &str,
    ) -> Result<Option<SaveProcurementConfirmationResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.action != SAVE_COMMAND_ACTION
            || audit.resource_type != "procurement_confirmation"
            || audit.resource_id.as_deref() != Some(confirmation_id)
            || !audit.success
        {
            return Err(Error::Internal("采购确认保存幂等收据身份非法".to_string()));
        }
        let receipt = parse_save_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("采购确认保存幂等收据为空".to_string()))?,
            expected_fingerprint,
        )?;
        Ok(Some(receipt.into_result()))
    }
}

/// W07 待办读取必须同时锁定的身份、版本与当前责任。
pub(super) struct ProcurementConfirmationTaskGuard<'a> {
    confirmation_id: &'a str,
    submission_id: &'a SalesOrderSubmissionId,
    work_item_id: &'a str,
    expected_task_version: u64,
    expected_subject_version: &'a str,
    actor_id: &'a str,
}

impl<'a> ProcurementConfirmationTaskGuard<'a> {
    pub(super) fn new(
        confirmation_id: &'a str,
        submission_id: &'a SalesOrderSubmissionId,
        work_item_id: &'a str,
        expected_task_version: u64,
        expected_subject_version: &'a str,
        actor_id: &'a str,
    ) -> Self {
        Self {
            confirmation_id,
            submission_id,
            work_item_id,
            expected_task_version,
            expected_subject_version,
            actor_id,
        }
    }
}

/// 在调用方事务内加载并校验 W07 当前待办。
pub(super) async fn load_procurement_confirmation_work_item(
    db: &mongodb::Database,
    guard: ProcurementConfirmationTaskGuard<'_>,
    executor: &mut dyn Executor,
) -> Result<WorkItem> {
    let item = db
        .work_items()
        .find_by_id(guard.work_item_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购确认待办不存在".to_string()))?;
    validate_procurement_confirmation_work_item(
        &item,
        guard.confirmation_id,
        guard.submission_id,
        guard.expected_task_version,
        guard.expected_subject_version,
        guard.actor_id,
    )?;
    Ok(item)
}

/// 校验 W07 命令携带的任务 CAS、不可变提交版本与当前个人责任。
pub(super) fn validate_procurement_confirmation_work_item(
    item: &WorkItem,
    confirmation_id: &str,
    submission_id: &SalesOrderSubmissionId,
    expected_task_version: u64,
    expected_subject_version: &str,
    actor_id: &str,
) -> Result<()> {
    if item.base.version != expected_task_version {
        return Err(Error::ConflictError(
            "待办责任或版本已变化，请刷新后重试".to_string(),
        ));
    }
    let subject_version = submission_id.to_string();
    if expected_subject_version != subject_version || item.subject_version != subject_version {
        return Err(Error::ConflictError(
            "销售提交版本已变化，请刷新后重试".to_string(),
        ));
    }
    if item.approval_step_instance_id.is_some()
        || item.work_item_type != WorkItemType::ProcurementConfirmation
        || item.business_object_type != "procurement_confirmation"
        || item.business_object_id != confirmation_id
    {
        return Err(Error::BusinessLogicError("待办与当前采购确认不匹配".to_string()));
    }
    if !item.is_owned_by(actor_id) {
        return Err(Error::Forbidden(
            "当前账号不是该待办责任人，或处理权已变化".to_string(),
        ));
    }
    Ok(())
}

/// 在 W07 工作数据或正式事实写入前重验角色、组织数据范围与岗位分离。
pub(super) async fn ensure_procurement_confirmation_actor_eligible(
    db: &mongodb::Database,
    item: &WorkItem,
    submitted_by: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let resolver = crate::approval::ApprovalAssigneeResolver::new(db.clone());
    if !resolver
        .user_is_eligible_for_assignment(actor_id, &item.owner_role, &item.owner_organization_id, executor)
        .await?
    {
        return Err(Error::Forbidden(
            "当前账号已不具备该待办的角色或数据范围".to_string(),
        ));
    }
    if submitted_by == actor_id {
        return Err(Error::Forbidden("销售提交人不得确认自己的提交".to_string()));
    }
    Ok(())
}

/// 解析前端返回的正整数任务版本。
pub(super) fn parse_task_version(value: &str) -> Result<u64> {
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError("待办版本必须是正整数字符串".to_string()))?;
    if version == 0 {
        return Err(Error::ValidationError("待办版本必须大于 0".to_string()));
    }
    Ok(version)
}

/// 校验路径采购确认与强类型载荷一致。
pub(super) fn ensure_command_confirmation_id(path_id: &str, payload_id: &str) -> Result<()> {
    if path_id != payload_id {
        return Err(Error::ConflictError("路径采购确认与命令载荷不一致".to_string()));
    }
    Ok(())
}

/// 校验采购确认仍是可编辑、可决定的当前工作事实。
pub(super) fn ensure_pending_confirmation(confirmation: &ProcurementConfirmation) -> Result<()> {
    if confirmation.stable.status != ProcurementConfirmationStatus::Pending {
        return Err(Error::ConflictError("采购确认已处理，不允许继续操作".to_string()));
    }
    Ok(())
}

/// 校验命令、确认和待办共同冻结同一不可变销售提交。
pub(super) fn ensure_submission_identity(
    confirmation: &ProcurementConfirmation,
    payload_submission_id: &str,
    expected_subject_version: &str,
) -> Result<()> {
    let actual = confirmation.submission_id.as_ref();
    if payload_submission_id != actual || expected_subject_version != actual {
        return Err(Error::ConflictError(
            "销售提交版本已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 计算覆盖完整命令、路径和鉴权操作者的稳定指纹。
pub(super) fn command_fingerprint<T: Serialize>(
    action: &str,
    path_id: &str,
    actor_id: &str,
    command: &T,
) -> Result<String> {
    let payload = serde_json::to_vec(&(action, path_id, actor_id, command))
        .map_err(|error| Error::Internal(format!("采购确认命令序列化失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(payload)))
}

/// 形成不泄漏原始幂等键的稳定审计主键。
pub(super) fn command_audit_id(action: &str, actor_id: &str, idempotency_key: &str) -> String {
    let mut digest = Sha256::new();
    digest_part(&mut digest, action);
    digest_part(&mut digest, actor_id);
    digest_part(&mut digest, idempotency_key.trim());
    format!("w07-{:x}", digest.finalize())
}

fn digest_part(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SaveCommandReceipt {
    edit_version: u64,
    task_version: u64,
}

impl SaveCommandReceipt {
    fn into_result(self) -> SaveProcurementConfirmationResult {
        SaveProcurementConfirmationResult {
            edit_version: self.edit_version,
            task_version: self.task_version.to_string(),
        }
    }
}

fn save_receipt_message(fingerprint: &str, receipt: SaveCommandReceipt) -> String {
    format!(
        "fp={fingerprint};save={}|{}",
        receipt.edit_version, receipt.task_version
    )
}

fn parse_save_receipt(message: &str, expected_fingerprint: &str) -> Result<SaveCommandReceipt> {
    let (fingerprint, versions) = message
        .strip_prefix("fp=")
        .and_then(|value| value.split_once(";save="))
        .ok_or_else(|| Error::Internal("采购确认保存幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError(
            "幂等键已用于不同的采购确认保存命令".to_string(),
        ));
    }
    let (edit_version, task_version) = versions
        .split_once('|')
        .ok_or_else(|| Error::Internal("采购确认保存幂等收据结果非法".to_string()))?;
    Ok(SaveCommandReceipt {
        edit_version: edit_version
            .parse()
            .map_err(|_| Error::Internal("采购确认保存收据编辑版本非法".to_string()))?,
        task_version: task_version
            .parse()
            .map_err(|_| Error::Internal("采购确认保存收据任务版本非法".to_string()))?,
    })
}

/// 用本次提交的分行替换待处理采购确认上的旧分行。
///
/// # 参数
/// * `db` - 数据库
/// * `confirmation` - 待处理采购确认
/// * `lines` - 本次确认分行
/// * `executor` - 由调用方传入的事务执行器
///
/// # 返回
/// 返回新写入的确认分行。
///
/// # 错误
/// 行号重复、实体校验失败或仓储写入失败时返回错误。
pub(super) async fn replace_pending_confirmation_lines(
    db: &mongodb::Database,
    confirmation: &ProcurementConfirmation,
    lines: &[dto::ProcurementConfirmationLineRequest],
    executor: &mut dyn Executor,
) -> Result<Vec<ProcurementConfirmationLine>> {
    let built = build_confirmation_lines(confirmation, lines)?;
    let old_lines = db
        .procurement_confirmation_lines()
        .list_lines_by_confirmation(&confirmation.base.id.clone().into(), executor)
        .await?;
    for mut old in old_lines {
        db.procurement_confirmation_lines()
            .soft_delete(&mut old, executor)
            .await?;
    }
    for line in &built {
        db.procurement_confirmation_lines().create(line, executor).await?;
    }
    Ok(built)
}

/// 构建采购确认分行实体。
///
/// # 参数
/// * `confirmation` - 所属确认批次
/// * `lines` - 分行请求
///
/// # 返回
/// 返回分行实体清单。
///
/// # 错误
/// 行号重复时返回错误。
fn build_confirmation_lines(
    confirmation: &ProcurementConfirmation,
    lines: &[dto::ProcurementConfirmationLineRequest],
) -> Result<Vec<ProcurementConfirmationLine>> {
    let mut built = Vec::with_capacity(lines.len());
    for line in lines {
        if built
            .iter()
            .any(|existing: &ProcurementConfirmationLine| existing.line_no == line.line_no)
        {
            return Err(Error::ValidationError(format!("行号 {} 重复", line.line_no)));
        }
        built.push(ProcurementConfirmationLine::new(
            entities::ids::ProcurementConfirmationLineId::new(next_id()),
            ProcurementConfirmationLineData {
                procurement_confirmation_id: confirmation.base.id.clone().into(),
                line_no: line.line_no,
                sales_order_submission_line_id: line.sales_order_submission_line_id.clone(),
                supplier_id: line.supplier_id.clone(),
                supplier_offering_revision_id: line.supplier_offering_revision_id.clone(),
                confirmed_quantity: line.confirmed_quantity,
                latest_cost_gross: line.latest_cost_gross,
                input_tax_rate: line.input_tax_rate,
                expected_delivery_date: line.expected_delivery_date,
                fulfillment_mode: line.fulfillment_mode.into(),
                supplier_capability_revision_id: line.supplier_capability_revision_id.clone(),
            },
        )?);
    }
    Ok(built)
}

/// 构造采购确认详情视图。
///
/// # 参数
/// * `confirmation` - 确认批次实体
/// * `lines` - 分行实体
///
/// # 返回
/// 返回详情视图。
fn confirmation_detail_view(
    confirmation: &ProcurementConfirmation,
    lines: Vec<ProcurementConfirmationLine>,
) -> ProcurementConfirmationDetailView {
    ProcurementConfirmationDetailView {
        id: confirmation.base.id.clone(),
        sales_order_id: confirmation.sales_order_id.to_string(),
        submission_id: confirmation.submission_id.to_string(),
        status: confirmation.stable.status,
        handled_by: confirmation.handled_by.clone(),
        handled_at: confirmation.handled_at.map(|instant| instant.unix_secs() as u64),
        version: confirmation.base.version,
        created_at: confirmation.base.created_at,
        lines: lines
            .into_iter()
            .map(|line| ProcurementConfirmationLineView {
                id: line.base.id,
                line_no: line.line_no,
                sales_order_submission_line_id: line.sales_order_submission_line_id.to_string(),
                supplier_id: line.supplier_id.to_string(),
                supplier_offering_revision_id: line.supplier_offering_revision_id.map(|id| id.to_string()),
                confirmed_quantity: line.confirmed_quantity,
                latest_cost_gross: line.latest_cost_gross,
                input_tax_rate: line.input_tax_rate,
                expected_delivery_date: line.expected_delivery_date,
                fulfillment_mode: line.fulfillment_mode,
                supplier_capability_revision_id: line.supplier_capability_revision_id.to_string(),
            })
            .collect(),
        work_item: None,
        allowed_actions: Vec::new(),
        action_blockers: Vec::new(),
    }
}

fn push_procurement_blocker(
    view: &mut ProcurementConfirmationDetailView,
    action: ProcurementConfirmationAllowedAction,
    code: &str,
    message: &str,
) {
    view.action_blockers
        .push(ProcurementConfirmationActionBlockerView {
            action: action.as_str().to_string(),
            code: code.to_string(),
            message: message.to_string(),
        });
}

fn block_procurement_actions(view: &mut ProcurementConfirmationDetailView, code: &str, message: &str) {
    for action in [
        ProcurementConfirmationAllowedAction::Save,
        ProcurementConfirmationAllowedAction::Approve,
        ProcurementConfirmationAllowedAction::Reject,
    ] {
        push_procurement_blocker(view, action, code, message);
    }
}

fn procurement_approval_projection_blocker(
    confirmation_lines: &[ProcurementConfirmationLine],
    submission_lines: &[entities::sales_order::SalesOrderSubmissionLine],
) -> Option<(&'static str, &'static str)> {
    if confirmation_lines.is_empty() || submission_lines.is_empty() {
        return Some(("CONFIRMATION_LINES_INCOMPLETE", "采购确认分行不完整，不能通过"));
    }
    let mut confirmed_by_submission: std::collections::HashMap<String, Vec<entities::money::Quantity>> =
        std::collections::HashMap::new();
    for line in confirmation_lines {
        if line.supplier_offering_revision_id.is_none()
            || line.supplier_capability_revision_id.as_ref().trim().is_empty()
        {
            return Some((
                "SUPPLY_SOURCE_INCOMPLETE",
                "供应商供给修订、能力修订或确认数量不完整",
            ));
        }
        if !submission_lines
            .iter()
            .any(|submission| submission.base.id == line.sales_order_submission_line_id.as_ref())
        {
            return Some((
                "SUBMISSION_LINE_MISMATCH",
                "采购确认分行与当前不可变销售提交不匹配",
            ));
        }
        confirmed_by_submission
            .entry(line.sales_order_submission_line_id.to_string())
            .or_default()
            .push(line.confirmed_quantity);
    }
    for submission in submission_lines {
        let Some(required) = submission.quantity else {
            return Some(("SUBMISSION_QUANTITY_MISSING", "销售提交缺少可核对的基础数量"));
        };
        let confirmed = confirmed_by_submission
            .get(&submission.base.id)
            .map(|quantities| {
                quantities.iter().fold(
                    required.to_decimal() - required.to_decimal(),
                    |total, quantity| total + quantity.to_decimal(),
                )
            });
        if confirmed != Some(required.to_decimal()) {
            return Some((
                "CONFIRMATION_COVERAGE_INCOMPLETE",
                "采购确认数量尚未完整覆盖销售提交",
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::ids::{SalesOrderSubmissionId, WorkItemId};
    use entities::work_item::{
        AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };

    use super::{
        ensure_command_confirmation_id, parse_save_receipt, parse_task_version,
        validate_procurement_confirmation_work_item,
    };

    fn owned_task() -> WorkItem {
        let mut task = WorkItem::new_at(
            WorkItemId::new("wi-1"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementConfirmation,
                approval_step_instance_id: None,
                business_object_type: "procurement_confirmation".to_string(),
                business_object_id: "confirmation-1".to_string(),
                subject_version: "submission-1".to_string(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-procurement".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap();
        task.reassign("buyer-1", Instant::from_unix_secs(2)).unwrap();
        task.base.version = 2;
        task
    }

    #[test]
    fn w07_command_requires_current_task_subject_and_owner() {
        let task = owned_task();
        let submission_id = SalesOrderSubmissionId::new("submission-1");

        assert!(validate_procurement_confirmation_work_item(
            &task,
            "confirmation-1",
            &submission_id,
            2,
            "submission-1",
            "buyer-1",
        )
        .is_ok());
        assert!(validate_procurement_confirmation_work_item(
            &task,
            "confirmation-1",
            &submission_id,
            1,
            "submission-1",
            "buyer-1",
        )
        .is_err());
        assert!(validate_procurement_confirmation_work_item(
            &task,
            "confirmation-1",
            &submission_id,
            2,
            "submission-old",
            "buyer-1",
        )
        .is_err());
        assert!(validate_procurement_confirmation_work_item(
            &task,
            "confirmation-1",
            &submission_id,
            2,
            "submission-1",
            "buyer-2",
        )
        .is_err());
    }

    #[test]
    fn task_version_and_path_identity_are_strict() {
        assert_eq!(parse_task_version(" 7 ").unwrap(), 7);
        assert!(parse_task_version("0").is_err());
        assert!(parse_task_version("1.0").is_err());
        assert!(ensure_command_confirmation_id("pc-1", "pc-1").is_ok());
        assert!(ensure_command_confirmation_id("pc-1", "pc-2").is_err());
    }

    #[test]
    fn save_receipt_replays_only_the_original_payload() {
        let receipt = parse_save_receipt("fp=abc;save=4|9", "abc").unwrap();
        assert_eq!(receipt.edit_version, 4);
        assert_eq!(receipt.task_version, 9);
        assert!(parse_save_receipt("fp=abc;save=4|9", "different").is_err());
    }
}
