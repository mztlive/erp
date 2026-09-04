use database::{
    AccessControlExt, DocumentRegistryExt, Executor, LegacyImportExt, NoTransaction, Transactional,
    WorkItemExt,
};
use entities::common::time::Instant;
use entities::document_registry::WorkflowActionId;
use entities::ids::WorkItemId;
use entities::legacy_import::{
    confirmation_work_item, confirmation_workflow_action, ConfirmationDecision, ConfirmationMatrixDecision,
    ConfirmationScope, ConfirmationStatus, LegacyImportBatch, LegacyImportBatchStatus,
    LegacyImportCommandIdentity, LegacyImportConfirmation, LegacyImportConfirmationData,
    LegacyImportConfirmationId,
};
use entities::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use std::collections::HashMap;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::work_item::{ProcessingState, WorkItemAllowedAction, WorkItemService, WorkItemView};

use super::dto::{
    CompleteImportBusinessConfirmationCommand, CompleteImportBusinessConfirmationResult,
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, ImportBusinessConfirmationWorkItemView,
    LegacyImportConfirmationListParams, LegacyImportConfirmationView, PageView,
    PreparedConfirmationCompletion, SortDir,
};
use super::receipt::{parse_receipt_number, required_text};
use super::{
    LegacyImportConfirmationFilter, LegacyImportService, COMMAND_FINGERPRINT_PREFIX,
    IMPORT_CONFIRMATION_AUDIT_PREFIX, IMPORT_CONFIRMATION_HANDLER, IMPORT_CONFIRMATION_OBJECT_TYPE,
    IMPORT_CONFIRMATION_ORGANIZATION, IMPORT_CONFIRMATION_WORKSPACE,
};

impl LegacyImportService {
    /// 分页查询导入确认事实列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`batch_id` 为主要筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn confirmation_list(
        &self,
        params: &LegacyImportConfirmationListParams,
        actor: &AuditActor,
        rbac: SharedRbacService,
    ) -> Result<PageView<LegacyImportConfirmationView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = LegacyImportConfirmationFilter {
            batch_id: query.batch_id,
            confirmation_scope: query.confirmation_scope,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .legacy_import_confirmations()
            .search_legacy_import_confirmations(&filter, &mut NoTransaction)
            .await?;
        let work_item_ids = page
            .items
            .iter()
            .map(|row| row.work_item_id.clone())
            .collect::<Vec<_>>();
        let work_items = self
            .db
            .work_items()
            .list_legacy_import_confirmations_by_ids(&work_item_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|item| (item.base.id.clone(), item))
            .collect::<HashMap<_, _>>();
        let work_item_service = WorkItemService::new(self.db.clone(), rbac);
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let work_item_id = row.work_item_id.to_string();
            let work_item = match work_item_service.work_item_detail(&work_item_id, actor).await {
                Ok(view) => Some(authorized_work_item_view(view, row.status)),
                Err(Error::Forbidden(_) | Error::NotFound(_)) => {
                    work_items.get(&work_item_id).map(read_only_work_item_view)
                }
                Err(error) => return Err(error),
            };
            items.push(LegacyImportConfirmationView {
                id: row.id,
                batch_id: row.batch_id.to_string(),
                confirmation_scope: row.confirmation_scope,
                owner_role: row.owner_role,
                batch_version: row.batch_version,
                trial_version: row.trial_version,
                status: row.status,
                decision: row.decision,
                reason_code: row.reason_code,
                comment: None,
                work_item,
                work_item_id,
                decided_by: row.decided_by,
                decided_at: row.decided_at.map(|at| at as i64),
                version: row.version,
                created_at: row.created_at,
            });
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建待确认确认事实。
    ///
    /// 批次推进到 `PendingConfirmation`（试算完成）；同一
    /// `(batch_id, scope, trial_version)` 重复提交按幂等返回既有事实。
    /// 新建开放任务指定当前操作人为个人责任人，责任角色仍按确认范围注册表确定。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人；新建任务以其为个人责任人
    ///
    /// # 返回
    /// 返回新建（或既有）确认事实的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 批次不存在
    /// * `BusinessLogicError` - 批次已进入不可确认阶段
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_confirmation(
        &self,
        req: CreateLegacyImportConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<LegacyImportConfirmationView> {
        req.validate()?;
        let confirmation_scope = ConfirmationScope::parse(&req.confirmation_scope)?;
        let owner_role = confirmation_scope.owner_role().to_string();
        let confirmation_scope = confirmation_scope.as_str().to_string();
        let import_rule_version = required_text(&req.import_rule_version, "导入规则版本不能为空")?;
        let subject_version = LegacyImportConfirmation::subject_version(
            req.batch_version,
            req.trial_version,
            &import_rule_version,
        );
        let confirmation_id = LegacyImportConfirmationId::new(next_id());
        let work_item_id = WorkItemId::new(next_id());
        let confirmation = LegacyImportConfirmation::new(
            confirmation_id,
            LegacyImportConfirmationData {
                batch_id: req.batch_id.clone(),
                confirmation_scope: confirmation_scope.clone(),
                owner_role: owner_role.clone(),
                batch_version: req.batch_version,
                trial_version: req.trial_version,
                import_rule_version: import_rule_version.clone(),
                work_item_id: work_item_id.clone(),
            },
        )?;
        let work_item = confirmation_work_item(
            work_item_id,
            &req.batch_id,
            subject_version.clone(),
            &confirmation_scope,
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "legacy_import_confirmation.create",
            "legacy_import_confirmation",
            confirmation.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let confirmation_for_tx = confirmation.clone();
        let work_item_for_tx = work_item.clone();
        let req_for_tx = req.clone();
        let scope_for_tx = confirmation_scope.clone();
        let owner_role_for_tx = owner_role.clone();
        let import_rule_for_tx = import_rule_version.clone();
        let subject_for_tx = subject_version.clone();
        let actor_id = actor.id().to_string();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(existing) = db
                        .legacy_import_confirmations()
                        .find_by_batch_scope_trial(
                            &req_for_tx.batch_id,
                            &scope_for_tx,
                            req_for_tx.trial_version,
                            session,
                        )
                        .await?
                    {
                        let existing_item = db
                            .work_items()
                            .find_by_id(existing.work_item_id.as_ref(), session)
                            .await?
                            .ok_or_else(|| Error::Internal("导入确认任务关联缺失".to_string()))?;
                        validate_confirmation_creation_replay(
                            &existing,
                            &existing_item,
                            &req_for_tx,
                            &scope_for_tx,
                            &owner_role_for_tx,
                            &import_rule_for_tx,
                            &subject_for_tx,
                        )?;
                        return Ok::<(LegacyImportConfirmation, WorkItem), crate::errors::Error>((
                            existing,
                            existing_item,
                        ));
                    }

                    let mut batch = db
                        .legacy_import_batches()
                        .find_by_id(req_for_tx.batch_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
                    validate_confirmation_creation_batch(&batch, &scope_for_tx, &import_rule_for_tx)?;
                    batch.prepare_confirmation()?;
                    let enabled_roles = db
                        .roles()
                        .enabled_roles(std::slice::from_ref(&owner_role_for_tx), session)
                        .await?;
                    if enabled_roles.len() != 1 {
                        return Err(Error::BusinessLogicError(
                            "导入确认责任角色未注册或已停用".to_string(),
                        ));
                    }
                    let mut confirmations = db
                        .legacy_import_confirmations()
                        .list_by_batch(&req_for_tx.batch_id, session)
                        .await?;
                    validate_trial_snapshot(&batch, &confirmations, &req_for_tx, &import_rule_for_tx)?;
                    invalidate_replaced_confirmation(
                        &db,
                        &mut confirmations,
                        &confirmation_for_tx,
                        &actor_id,
                        session,
                    )
                    .await?;
                    let mut current_matrix = LegacyImportConfirmation::current_matrix(
                        &confirmations,
                        req_for_tx.batch_version,
                        req_for_tx.trial_version,
                        &import_rule_for_tx,
                    );
                    current_matrix.push(confirmation_for_tx.clone());
                    batch.update_summaries(
                        batch.failure_code_summary.clone(),
                        Some(LegacyImportConfirmation::matrix_summary(
                            req_for_tx.trial_version,
                            &current_matrix,
                        )),
                    )?;
                    db.legacy_import_confirmations()
                        .create(&confirmation_for_tx, session)
                        .await?;
                    db.work_items().create(&work_item_for_tx, session).await?;
                    db.legacy_import_batches().update(&mut batch, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(LegacyImportConfirmation, WorkItem), crate::errors::Error>((
                        confirmation_for_tx,
                        work_item_for_tx,
                    ))
                })
            })
            .await;
        let (confirmation, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => match self
                .replay_confirmation_creation(
                    &req,
                    &confirmation_scope,
                    &owner_role,
                    &import_rule_version,
                    &subject_version,
                )
                .await?
            {
                Some(result) => result,
                None => return Err(error),
            },
        };

        Ok(confirmation_view(confirmation, &work_item))
    }

    /// 执行 `CompleteImportBusinessConfirmation` 强类型命令。
    ///
    /// 确认事实、批次摘要/阶段、`workflow_action`、任务完成与稳定审计
    /// 收据在同一事务提交。同一幂等键只有在全部命令字段一致时才返回原结果。
    ///
    /// # 参数
    /// * `req` - 强类型完成命令
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回确认事实、已完成任务、批次新版本、下一步和审计收据。
    ///
    /// # 错误
    /// * `NotFound` - 任务、确认事实或批次不存在
    /// * `ConflictError` - 任务/批次/试算版本或幂等指纹不一致
    /// * `Forbidden` - 当前用户不是任务责任人或已失去责任资格
    pub async fn complete_import_business_confirmation(
        &self,
        req: CompleteImportBusinessConfirmationCommand,
        actor: &AuditActor,
    ) -> Result<CompleteImportBusinessConfirmationResult> {
        req.validate()?;
        let prepared = PreparedConfirmationCompletion::try_from(req)?;
        let action = "legacy_import_confirmation.complete";
        let identity = confirmation_command_identity(actor.id(), action, &prepared);
        let fingerprint = identity.fingerprint().to_string();
        let audit_id = identity.audit_id().to_string();
        if let Some(result) = self
            .replay_confirmation_completion(&audit_id, &fingerprint, &prepared)
            .await?
        {
            return Ok(result);
        }
        let decided_at = Instant::now();
        let workflow_action_id = WorkflowActionId::new(next_id());
        let db = self.db.clone();
        let client = db.client().clone();
        let prepared_for_tx = prepared.clone();
        let actor_id = actor.id().to_string();
        let audit_actor = actor.clone();
        let rbac_for_tx = crate::iam::shared_rbac_service(self.db.clone());
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut work_item = db
                        .work_items()
                        .find_by_id(prepared_for_tx.work_item_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入确认任务不存在".to_string()))?;
                    let mut confirmation = db
                        .legacy_import_confirmations()
                        .find_by_work_item(&prepared_for_tx.work_item_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入确认事实不存在".to_string()))?;
                    let mut batch = db
                        .legacy_import_batches()
                        .find_by_id(confirmation.batch_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
                    validate_confirmation_completion(
                        &prepared_for_tx,
                        &work_item,
                        &confirmation,
                        &batch,
                        &actor_id,
                    )?;
                    WorkItemService::new(db.clone(), rbac_for_tx.clone())
                        .ensure_domain_decision_access(&audit_actor, &work_item, session)
                        .await?;
                    let _ = &work_item;
                    let mut matrix = db
                        .legacy_import_confirmations()
                        .list_by_batch(&confirmation.batch_id, session)
                        .await?;
                    confirmation.decide(
                        prepared_for_tx.decision,
                        actor_id.clone(),
                        decided_at,
                        prepared_for_tx.reason_code.clone(),
                        prepared_for_tx.comment.clone(),
                    )?;
                    work_item.record_activity(&actor_id, decided_at)?;
                    work_item.complete_by_domain_command(actor_id.clone(), decided_at)?;
                    replace_confirmation_in_matrix(&mut matrix, &confirmation);
                    let current_matrix = LegacyImportConfirmation::current_matrix(
                        &matrix,
                        confirmation.batch_version,
                        confirmation.trial_version,
                        &confirmation.import_rule_version,
                    );
                    let required_scopes = batch.required_confirmation_scopes()?;
                    let next_step = confirmation_next_step(LegacyImportConfirmation::matrix_decision(
                        prepared_for_tx.decision,
                        &current_matrix,
                        &required_scopes,
                    ));
                    batch.update_summaries(
                        batch.failure_code_summary.clone(),
                        Some(LegacyImportConfirmation::matrix_summary(
                            confirmation.trial_version,
                            &current_matrix,
                        )),
                    )?;
                    if next_step == ImportBusinessConfirmationNextStep::StartApply {
                        batch.advance(LegacyImportBatchStatus::ReadyToApply)?;
                    }
                    let workflow_action =
                        confirmation_workflow_action(workflow_action_id, &confirmation, &actor_id)?;
                    db.legacy_import_confirmations()
                        .update(&mut confirmation, session)
                        .await?;
                    db.legacy_import_batches().update(&mut batch, session).await?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.workflow_actions().create(&workflow_action, session).await?;
                    let receipt = ConfirmationCompletionReceipt {
                        result_status: confirmation_result_status(prepared_for_tx.decision),
                        task_version: work_item.base.version,
                        batch_version: batch.base.version,
                        next_step,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx.clone(),
                        action,
                        "legacy_import_confirmation",
                        confirmation.base.id.clone(),
                        Some(confirmation_completion_receipt_message(
                            &fingerprint_for_tx,
                            receipt,
                        )),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ConfirmationCompletionTransactionResult, crate::errors::Error>(
                        ConfirmationCompletionTransactionResult {
                            confirmation,
                            work_item,
                            receipt,
                        },
                    )
                })
            })
            .await;
        let result = match transaction_result {
            Ok(result) => result,
            Err(error) => match self
                .replay_confirmation_completion(&audit_id, &fingerprint, &prepared)
                .await?
            {
                Some(result) => return Ok(result),
                None => return Err(error),
            },
        };

        Ok(completion_result(result, audit_id))
    }

    /// 读取并严格核对已创建的同一试算确认任务。
    async fn replay_confirmation_creation(
        &self,
        req: &CreateLegacyImportConfirmationRequest,
        scope: &str,
        owner_role: &str,
        import_rule_version: &str,
        subject_version: &str,
    ) -> Result<Option<(LegacyImportConfirmation, WorkItem)>> {
        let Some(confirmation) = self
            .db
            .legacy_import_confirmations()
            .find_by_batch_scope_trial(&req.batch_id, scope, req.trial_version, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let work_item = self
            .db
            .work_items()
            .find_by_id(confirmation.work_item_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认任务关联缺失".to_string()))?;
        validate_confirmation_creation_replay(
            &confirmation,
            &work_item,
            req,
            scope,
            owner_role,
            import_rule_version,
            subject_version,
        )?;
        Ok(Some((confirmation, work_item)))
    }

    /// 按稳定审计收据重放已提交的导入确认命令。
    async fn replay_confirmation_completion(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        prepared: &PreparedConfirmationCompletion,
    ) -> Result<Option<CompleteImportBusinessConfirmationResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let receipt = parse_confirmation_completion_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("导入确认幂等收据缺少结果".to_string()))?,
            expected_fingerprint,
        )?;
        let confirmation = self
            .db
            .legacy_import_confirmations()
            .find_by_work_item(&prepared.work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认幂等收据对应事实缺失".to_string()))?;
        if audit.resource_id.as_deref() != Some(&confirmation.base.id) {
            return Err(Error::Internal("导入确认幂等收据与业务事实不一致".to_string()));
        }
        let work_item = self
            .db
            .work_items()
            .find_by_id(prepared.work_item_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("导入确认幂等收据对应任务缺失".to_string()))?;
        if work_item.status != WorkItemStatus::Completed
            || work_item.base.version != receipt.task_version
            || confirmation.decision != Some(prepared.decision)
        {
            return Err(Error::Internal("导入确认幂等收据对应结果不一致".to_string()));
        }
        Ok(Some(completion_result(
            ConfirmationCompletionTransactionResult {
                confirmation,
                work_item,
                receipt,
            },
            audit_id.to_string(),
        )))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConfirmationCompletionReceipt {
    result_status: ImportBusinessConfirmationResultStatus,
    task_version: u64,
    batch_version: u64,
    next_step: ImportBusinessConfirmationNextStep,
}

struct ConfirmationCompletionTransactionResult {
    confirmation: LegacyImportConfirmation,
    work_item: WorkItem,
    receipt: ConfirmationCompletionReceipt,
}

/// 校验创建确认任务时的批次与责任范围。
fn validate_confirmation_creation_batch(
    batch: &LegacyImportBatch,
    scope: &str,
    import_rule_version: &str,
) -> Result<()> {
    if !batch.has_rule_version(import_rule_version) {
        return Err(Error::ConflictError("导入规则版本已变化，请重新试算".to_string()));
    }
    let scope = ConfirmationScope::parse(scope)?;
    if !batch.required_confirmation_scopes()?.contains(&scope) {
        return Err(Error::BusinessLogicError(
            "该责任范围不属于当前批次的必要确认矩阵".to_string(),
        ));
    }
    Ok(())
}

/// 校验同一试算矩阵的版本一致性和单调性。
fn validate_trial_snapshot(
    batch: &LegacyImportBatch,
    confirmations: &[LegacyImportConfirmation],
    req: &CreateLegacyImportConfirmationRequest,
    import_rule_version: &str,
) -> Result<()> {
    LegacyImportConfirmation::ensure_trial_snapshot(
        confirmations,
        req.batch_version,
        req.trial_version,
        import_rule_version,
    )?;
    if !batch.has_rule_version(import_rule_version) {
        return Err(Error::ConflictError("导入规则版本已变化".to_string()));
    }
    Ok(())
}

/// 严格校验重复创建是否与已有事实及任务完全一致。
fn validate_confirmation_creation_replay(
    confirmation: &LegacyImportConfirmation,
    work_item: &WorkItem,
    req: &CreateLegacyImportConfirmationRequest,
    scope: &str,
    owner_role: &str,
    import_rule_version: &str,
    subject_version: &str,
) -> Result<()> {
    let exact = confirmation.batch_id == req.batch_id
        && confirmation.confirmation_scope == scope
        && confirmation.owner_role == owner_role
        && confirmation.batch_version == req.batch_version
        && confirmation.trial_version == req.trial_version
        && confirmation.import_rule_version == import_rule_version
        && work_item.base.id == confirmation.work_item_id.to_string()
        && work_item.work_item_type == WorkItemType::ImportBusinessConfirmation
        && work_item.business_object_type == IMPORT_CONFIRMATION_OBJECT_TYPE
        && work_item.business_object_id == req.batch_id.to_string()
        && work_item.responsibility_key() == Some(scope)
        && work_item.subject_version == subject_version
        && work_item.owner_role == owner_role
        && work_item.owner_organization_id == IMPORT_CONFIRMATION_ORGANIZATION;
    if exact {
        return Ok(());
    }
    Err(Error::ConflictError(
        "同一批次、范围与试算版本已用于不同的确认任务".to_string(),
    ))
}

/// 将新试算取代的旧待确认事实失效，并关闭关联任务（INT-R28 批量读写）。
///
/// 预收集全部被取代确认的正式任务 ID，单次 `$in` 批量装载任务快照；
/// 先完成全部实体 `invalidate`/`close` 迁移，再经仓储批量 CAS 写回，
/// 均在调用方同一事务执行器内，任一失败整体回滚。已关闭任务跳过写回，
/// 关联缺失失败关闭。
///
/// # 参数
/// * `db` - 数据库
/// * `confirmations` - 当前试算矩阵（就地失效被取代项）
/// * `replacement` - 新试算确认事实
/// * `actor_id` - 当前操作人
/// * `executor` - 调用方事务执行器
///
/// # 错误
/// 确认失效、关联任务缺失或任务关闭/写入失败时返回错误。
///
/// # 约束
/// 不自行开启或提交事务；批量读取不改变软删除与缺失语义。
async fn invalidate_replaced_confirmation(
    db: &Database,
    confirmations: &mut [LegacyImportConfirmation],
    replacement: &LegacyImportConfirmation,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let replaced_ids = replaced_confirmation_work_item_ids(confirmations, replacement.trial_version);
    if replaced_ids.is_empty() {
        return Ok(());
    }
    let work_items = db
        .work_items()
        .list_legacy_import_confirmations_by_ids(&replaced_ids, executor)
        .await?;
    let mut work_items_by_id = HashMap::new();
    for item in work_items {
        work_items_by_id.insert(item.base.id.clone(), item);
    }
    let replacement_id = LegacyImportConfirmationId::new(replacement.base.id.clone());
    for confirmation in confirmations
        .iter_mut()
        .filter(|item| item.is_replaced_by(replacement.trial_version))
    {
        confirmation.invalidate(replacement_id.clone(), Instant::now())?;
    }
    let mut to_close = collect_superseded_closable_work_items(
        confirmations,
        replacement.trial_version,
        &mut work_items_by_id,
        &replacement_id,
    )?;
    for work_item in to_close.iter_mut() {
        work_item.close(
            actor_id,
            WorkItemCloseData {
                close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string(),
            },
            Instant::now(),
        )?;
    }
    db.legacy_import_confirmations()
        .persist_invalidated_confirmations(confirmations, &replacement_id, executor)
        .await?;
    db.work_items()
        .persist_closed_confirmation_work_items(&mut to_close, executor)
        .await?;
    Ok(())
}

/// 从批量装载的任务快照中取出需关闭的取代任务（INT-R28 纯映射）。
///
/// 仅处理本轮已标注失效的确认：关联缺失失败关闭，已关闭任务跳过，
/// 开放任务返回调用方关闭。返回任务按矩阵顺序排列。
///
/// # 参数
/// * `confirmations` - 已完成 `invalidate` 迁移的矩阵
/// * `replacement_trial_version` - 新试算版本
/// * `work_items_by_id` - 按任务 ID 索引的批量快照（命中项被取出）
/// * `replacement_id` - 本轮替代确认事实 ID
///
/// # 返回
/// 返回需关闭的开放任务；无关闭项时返回空集合。
///
/// # 错误
/// 任一已失效确认关联任务缺失时返回 `Internal` 错误。
///
/// # 约束
/// 纯内存映射，不访问数据库；不改变缺失与已关闭语义。
fn collect_superseded_closable_work_items(
    confirmations: &[LegacyImportConfirmation],
    replacement_trial_version: u32,
    work_items_by_id: &mut HashMap<String, WorkItem>,
    replacement_id: &LegacyImportConfirmationId,
) -> Result<Vec<WorkItem>> {
    let mut to_close = Vec::new();
    for confirmation in confirmations.iter().filter(|item| {
        item.status == ConfirmationStatus::Invalidated
            && item.replacement_confirmation_id.as_ref() == Some(replacement_id)
            && item.trial_version < replacement_trial_version
    }) {
        let work_item = work_items_by_id
            .remove(confirmation.work_item_id.as_ref())
            .ok_or_else(|| Error::Internal("被新试算取代的确认任务缺失".to_string()))?;
        if work_item.status == WorkItemStatus::Open {
            to_close.push(work_item);
        }
    }
    Ok(to_close)
}

/// 预收集被新试算取代确认的正式任务 ID（INT-R28 纯映射）。
///
/// 仅保留仍待确认且试算版本更低的确认，保持矩阵顺序并去重，供单次批量装载。
///
/// # 参数
/// * `confirmations` - 当前试算矩阵
/// * `replacement_trial_version` - 新试算版本
///
/// # 返回
/// 返回去重后的正式任务 ID；无取代项时返回空集合。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯内存过滤，不访问数据库；不改变取代判定语义。
fn replaced_confirmation_work_item_ids(
    confirmations: &[LegacyImportConfirmation],
    replacement_trial_version: u32,
) -> Vec<WorkItemId> {
    use std::collections::HashSet;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    for confirmation in confirmations
        .iter()
        .filter(|item| item.is_replaced_by(replacement_trial_version))
    {
        if seen.insert(confirmation.work_item_id.to_string()) {
            ids.push(confirmation.work_item_id.clone());
        }
    }
    ids
}

/// 将本次决策后的事实替换进内存矩阵。
fn replace_confirmation_in_matrix(
    confirmations: &mut [LegacyImportConfirmation],
    decided: &LegacyImportConfirmation,
) {
    if let Some(current) = confirmations
        .iter_mut()
        .find(|item| item.base.id == decided.base.id)
    {
        *current = decided.clone();
    }
}

/// 将领域确认矩阵决策映射为服务契约下一步。
///
/// # 参数
/// * `decision` - 领域层确定的唯一矩阵决策
///
/// # 返回
/// 返回 HTTP 契约使用的下一步枚举。
fn confirmation_next_step(decision: ConfirmationMatrixDecision) -> ImportBusinessConfirmationNextStep {
    match decision {
        ConfirmationMatrixDecision::AwaitOtherConfirmations => {
            ImportBusinessConfirmationNextStep::AwaitOtherConfirmations
        }
        ConfirmationMatrixDecision::StartApply => ImportBusinessConfirmationNextStep::StartApply,
        ConfirmationMatrixDecision::FixAndRevalidate => ImportBusinessConfirmationNextStep::FixAndRevalidate,
    }
}

/// 校验完成命令锁定的任务、事实、批次和当前责任。
fn validate_confirmation_completion(
    command: &PreparedConfirmationCompletion,
    work_item: &WorkItem,
    confirmation: &LegacyImportConfirmation,
    batch: &LegacyImportBatch,
    actor_id: &str,
) -> Result<()> {
    if work_item.base.version != command.expected_task_version
        || !batch.has_version(command.expected_batch_version)
    {
        return Err(Error::ConflictError(
            "导入确认任务或批次版本已变化，请刷新后重试".to_string(),
        ));
    }
    let expected_subject = LegacyImportConfirmation::subject_version(
        confirmation.batch_version,
        confirmation.trial_version,
        &confirmation.import_rule_version,
    );
    if command.expected_subject_version != expected_subject
        || work_item.subject_version != expected_subject
        || confirmation.trial_version != command.expected_trial_version
    {
        return Err(Error::ConflictError("导入确认的批次或试算快照已变化".to_string()));
    }
    let owner_role = ConfirmationScope::parse(&command.confirmation_scope)?.owner_role();
    let task_matches = work_item.work_item_type == WorkItemType::ImportBusinessConfirmation
        && work_item.business_object_type == IMPORT_CONFIRMATION_OBJECT_TYPE
        && work_item.business_object_id == confirmation.batch_id.to_string()
        && work_item.responsibility_key() == Some(command.confirmation_scope.as_str())
        && work_item.owner_role == owner_role
        && work_item.owner_organization_id == IMPORT_CONFIRMATION_ORGANIZATION;
    let fact_matches = confirmation.work_item_id == command.work_item_id
        && confirmation.batch_id == command.batch_id
        && confirmation.confirmation_scope == command.confirmation_scope
        && confirmation.owner_role == owner_role
        && confirmation.is_pending()
        && batch.base.id == confirmation.batch_id.to_string()
        && batch.accepts_confirmation_decision()
        && batch.import_rule_version == confirmation.import_rule_version;
    if !task_matches || !fact_matches {
        return Err(Error::BusinessLogicError(
            "导入确认任务、责任范围或批次不匹配".to_string(),
        ));
    }
    let command_scope = ConfirmationScope::parse(&command.confirmation_scope)?;
    if !batch.required_confirmation_scopes()?.contains(&command_scope) {
        return Err(Error::BusinessLogicError(
            "当前确认范围已不属于批次必要矩阵".to_string(),
        ));
    }
    if !work_item.is_owned_by(actor_id) {
        return Err(Error::Forbidden("当前账号不是该导入确认的当前责任人".to_string()));
    }
    Ok(())
}

/// 把任务实体映射为 W18 真实任务投影。
fn work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.base.id.clone(),
        work_item_type: item.work_item_type,
        task_version: item.base.version.to_string(),
        subject_version: item.subject_version.clone(),
        status: item.status,
        owner_role: item.owner_role.clone(),
        owner_organization_id: item.owner_organization_id.clone(),
        owner_user_id: item.owner_user_id.clone(),
        processing_state: "READY".to_string(),
        allowed_actions: Vec::new(),
        action_blockers: Vec::new(),
        handler_key: IMPORT_CONFIRMATION_HANDLER.to_string(),
        destination_workspace_id: IMPORT_CONFIRMATION_WORKSPACE.to_string(),
    }
}

/// 为不在当前责任范围的查询人返回最小只读任务投影。
fn read_only_work_item_view(item: &WorkItem) -> ImportBusinessConfirmationWorkItemView {
    let mut view = work_item_view(item);
    view.owner_user_id = None;
    if item.status == WorkItemStatus::Open {
        view.action_blockers
            .push("当前账号不在该责任范围，任务仅可查看。".to_string());
    }
    view
}

/// 把统一待办的 actor 安全投影合并为 W18 责任与领域动作。
fn authorized_work_item_view(
    item: WorkItemView,
    confirmation_status: ConfirmationStatus,
) -> ImportBusinessConfirmationWorkItemView {
    let mut allowed_actions = item
        .allowed_actions
        .iter()
        .copied()
        .map(work_item_action_code)
        .map(str::to_string)
        .collect::<Vec<_>>();
    append_confirmation_actions(&mut allowed_actions, confirmation_status, &item.allowed_actions);
    let mut action_blockers = item
        .action_blockers
        .into_iter()
        .map(|blocker| blocker.message)
        .collect::<Vec<_>>();
    if let Some(blocker) = item.processing_blocker {
        action_blockers.push(blocker.message);
    }
    ImportBusinessConfirmationWorkItemView {
        work_item_id: item.id,
        work_item_type: item.work_item_type,
        task_version: item.task_version,
        subject_version: item.subject_version,
        status: item.status,
        owner_role: item.owner_role,
        owner_organization_id: item.owner_organization_id,
        owner_user_id: item.owner_user_id,
        processing_state: processing_state_code(item.processing_state).to_string(),
        allowed_actions,
        action_blockers,
        handler_key: item.handler_key,
        destination_workspace_id: item.destination_workspace_id,
    }
}

/// 只有当前责任人且确认事实仍待处理时，才追加 W18 正式领域动作。
fn append_confirmation_actions(
    actions: &mut Vec<String>,
    confirmation_status: ConfirmationStatus,
    responsibility_actions: &[WorkItemAllowedAction],
) {
    if confirmation_status != ConfirmationStatus::Pending
        || !responsibility_actions.contains(&WorkItemAllowedAction::Process)
    {
        return;
    }
    actions.push("CONFIRM_SCOPE".to_string());
    actions.push("RETURN_FOR_FIX".to_string());
}

/// 返回统一责任动作的稳定 wire code。
fn work_item_action_code(action: WorkItemAllowedAction) -> &'static str {
    match action {
        WorkItemAllowedAction::View => "VIEW",
        WorkItemAllowedAction::Process => "PROCESS",
        WorkItemAllowedAction::Approve => "APPROVE",
        WorkItemAllowedAction::Reject => "REJECT",
        WorkItemAllowedAction::Reassign => "REASSIGN",
        WorkItemAllowedAction::Close => "CLOSE",
    }
}

/// 返回统一处理状态的稳定 wire code。
fn processing_state_code(state: ProcessingState) -> &'static str {
    match state {
        ProcessingState::Ready => "READY",
        ProcessingState::ApprovalBlocked => "APPROVAL_BLOCKED",
    }
}

/// 合并确认事实与对应任务投影。
fn confirmation_view(
    confirmation: LegacyImportConfirmation,
    work_item: &WorkItem,
) -> LegacyImportConfirmationView {
    let mut view: LegacyImportConfirmationView = confirmation.into();
    view.work_item = Some(work_item_view(work_item));
    view
}

/// 把事务结果组装为强类型响应信封。
fn completion_result(
    result: ConfirmationCompletionTransactionResult,
    audit_receipt: String,
) -> CompleteImportBusinessConfirmationResult {
    CompleteImportBusinessConfirmationResult {
        result_status: result.receipt.result_status,
        confirmation: confirmation_view(result.confirmation, &result.work_item),
        work_item: work_item_view(&result.work_item),
        batch_version: result.receipt.batch_version,
        next_step: result.receipt.next_step,
        audit_receipt,
    }
}

/// 返回决策对应的稳定结果状态。
fn confirmation_result_status(decision: ConfirmationDecision) -> ImportBusinessConfirmationResultStatus {
    match decision {
        ConfirmationDecision::ConfirmScope => ImportBusinessConfirmationResultStatus::Confirmed,
        ConfirmationDecision::ReturnForFix => ImportBusinessConfirmationResultStatus::Rejected,
    }
}

/// 构造导入确认命令的领域幂等身份。
///
/// # 参数
/// * `actor_id` - 当前确认人
/// * `action` - 稳定审计动作
/// * `command` - 已解析并规范化的确认命令
///
/// # 返回
/// 返回不暴露原始幂等键的审计 ID 与完整命令指纹。
fn confirmation_command_identity(
    actor_id: &str,
    action: &str,
    command: &PreparedConfirmationCompletion,
) -> LegacyImportCommandIdentity {
    let task_version = command.expected_task_version.to_string();
    let batch_version = command.expected_batch_version.to_string();
    let trial_version = command.expected_trial_version.to_string();
    LegacyImportCommandIdentity::new(
        IMPORT_CONFIRMATION_AUDIT_PREFIX,
        actor_id,
        action,
        command.work_item_id.as_ref(),
        &command.idempotency_key,
        &[
            command.work_item_id.as_ref(),
            command.batch_id.as_ref(),
            &task_version,
            &command.expected_subject_version,
            &batch_version,
            &trial_version,
            &command.confirmation_scope,
            command.decision.as_str(),
            command.reason_code.as_deref().unwrap_or_default(),
            command.comment.as_deref().unwrap_or_default(),
        ],
    )
}

/// 将导入确认的最小结果收据编码到审计消息。
fn confirmation_completion_receipt_message(
    fingerprint: &str,
    receipt: ConfirmationCompletionReceipt,
) -> String {
    let result = match receipt.result_status {
        ImportBusinessConfirmationResultStatus::Confirmed => "C",
        ImportBusinessConfirmationResultStatus::Rejected => "R",
        ImportBusinessConfirmationResultStatus::Unknown => "U",
    };
    let next = match receipt.next_step {
        ImportBusinessConfirmationNextStep::AwaitOtherConfirmations => "W",
        ImportBusinessConfirmationNextStep::StartApply => "A",
        ImportBusinessConfirmationNextStep::FixAndRevalidate => "F",
    };
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={result}|{}|{}|{next}",
        receipt.task_version, receipt.batch_version
    )
}

/// 解析并核对导入确认审计收据。
fn parse_confirmation_completion_receipt(
    message: &str,
    expected_fingerprint: &str,
) -> Result<ConfirmationCompletionReceipt> {
    let (fingerprint, encoded) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("导入确认幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("幂等键已用于不同的导入确认命令".to_string()));
    }
    let fields = encoded.split('|').collect::<Vec<_>>();
    let [result, task_version, batch_version, next] = fields.as_slice() else {
        return Err(Error::Internal("导入确认幂等收据结果非法".to_string()));
    };
    let result_status = match *result {
        "C" => ImportBusinessConfirmationResultStatus::Confirmed,
        "R" => ImportBusinessConfirmationResultStatus::Rejected,
        "U" => ImportBusinessConfirmationResultStatus::Unknown,
        _ => return Err(Error::Internal("导入确认幂等收据状态非法".to_string())),
    };
    let next_step = match *next {
        "W" => ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
        "A" => ImportBusinessConfirmationNextStep::StartApply,
        "F" => ImportBusinessConfirmationNextStep::FixAndRevalidate,
        _ => return Err(Error::Internal("导入确认幂等收据下一步非法".to_string())),
    };
    Ok(ConfirmationCompletionReceipt {
        result_status,
        task_version: parse_receipt_number(task_version, "任务版本")?,
        batch_version: parse_receipt_number(batch_version, "批次版本")?,
        next_step,
    })
}

#[cfg(test)]
mod tests {
    use entities::common::time::BusinessDate;
    use entities::ids::{LegacyImportBatchId, LegacyImportConfirmationId, SourceSystemId, WorkItemId};
    use entities::legacy_import::{LegacyImportBatchData, LegacyImportConfirmationData};

    use super::*;

    fn batch() -> LegacyImportBatch {
        let mut batch = LegacyImportBatch::new(
            LegacyImportBatchId::new("batch-1"),
            LegacyImportBatchData {
                batch_no: "IMP-1".to_string(),
                source_system_id: SourceSystemId::new("source-1"),
                source_object_set: "CUSTOMER,CARD_OPENING_AR".to_string(),
                baseline_date: BusinessDate::from_ymd(2026, 8, 14).unwrap(),
                import_rule_version: "rule-1".to_string(),
                source_file_hmac: None,
                status: LegacyImportBatchStatus::PendingConfirmation,
                total_rows: 1,
                success_rows: 0,
                failed_rows: 0,
                failure_code_summary: None,
                confirmation_status_summary: None,
            },
        )
        .unwrap();
        batch.base.version = 4;
        batch
    }

    fn confirmation() -> LegacyImportConfirmation {
        LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("confirmation-1"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".to_string(),
                owner_role: "role-sales".to_string(),
                batch_version: 1,
                trial_version: 2,
                import_rule_version: "rule-1".to_string(),
                work_item_id: WorkItemId::new("work-item-1"),
            },
        )
        .unwrap()
    }

    fn work_item() -> WorkItem {
        let mut item = confirmation_work_item(
            WorkItemId::new("work-item-1"),
            &LegacyImportBatchId::new("batch-1"),
            LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
            "SALES",
            "user-1",
        )
        .unwrap();
        item.base.version = 3;
        item
    }

    fn completion_command() -> PreparedConfirmationCompletion {
        PreparedConfirmationCompletion {
            work_item_id: WorkItemId::new("work-item-1"),
            batch_id: LegacyImportBatchId::new("batch-1"),
            expected_task_version: 3,
            expected_subject_version: LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
            expected_batch_version: 4,
            expected_trial_version: 2,
            confirmation_scope: "SALES".to_string(),
            decision: ConfirmationDecision::ConfirmScope,
            reason_code: None,
            comment: Some("确认".to_string()),
            idempotency_key: "request-1".to_string(),
        }
    }

    #[test]
    fn create_command_rejects_client_owned_task_fields() {
        let payload = serde_json::json!({
            "batch_id": "batch-1",
            "confirmation_scope": "SALES",
            "owner_role": "role-root",
            "batch_version": 1,
            "trial_version": 2,
            "import_rule_version": "rule-1",
            "work_item_id": "forged-task"
        });

        assert!(serde_json::from_value::<CreateLegacyImportConfirmationRequest>(payload).is_err());
    }

    #[test]
    fn same_batch_confirmation_scopes_use_distinct_server_responsibility_keys() {
        let batch_id = LegacyImportBatchId::new("batch-1");
        let subject_version = LegacyImportConfirmation::subject_version(1, 2, "rule-1");
        let sales = confirmation_work_item(
            WorkItemId::new("work-item-sales"),
            &batch_id,
            subject_version.clone(),
            " sales ",
            "user-sales",
        )
        .unwrap();
        let procurement = confirmation_work_item(
            WorkItemId::new("work-item-procurement"),
            &batch_id,
            subject_version,
            "PROCUREMENT",
            "user-procurement",
        )
        .unwrap();

        assert_eq!(sales.business_object_id, "batch-1");
        assert_eq!(procurement.business_object_id, "batch-1");
        assert_eq!(sales.responsibility_key(), Some("SALES"));
        assert_eq!(procurement.responsibility_key(), Some("PROCUREMENT"));
        assert_eq!(sales.owner_role, "role-sales");
        assert_eq!(procurement.owner_role, "role-procurement");
    }

    #[test]
    fn completion_requires_exact_task_subject_batch_and_current_owner() {
        let command = completion_command();
        let item = work_item();
        let confirmation = confirmation();
        let batch = batch();

        validate_confirmation_completion(&command, &item, &confirmation, &batch, "user-1").unwrap();
        assert!(
            validate_confirmation_completion(&command, &item, &confirmation, &batch, "other-user").is_err()
        );
        let mut stale = command;
        stale.expected_trial_version = 3;
        assert!(validate_confirmation_completion(&stale, &item, &confirmation, &batch, "user-1").is_err());
    }

    #[test]
    fn domain_actions_require_pending_fact_and_process_responsibility() {
        let mut mine = vec!["VIEW".to_string(), "PROCESS".to_string()];
        append_confirmation_actions(
            &mut mine,
            ConfirmationStatus::Pending,
            &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
        );
        assert_eq!(mine, ["VIEW", "PROCESS", "CONFIRM_SCOPE", "RETURN_FOR_FIX"]);

        let mut view_only = vec!["VIEW".to_string()];
        append_confirmation_actions(
            &mut view_only,
            ConfirmationStatus::Pending,
            &[WorkItemAllowedAction::View],
        );
        assert_eq!(view_only, ["VIEW"]);

        let mut completed = vec!["VIEW".to_string(), "PROCESS".to_string()];
        append_confirmation_actions(
            &mut completed,
            ConfirmationStatus::Confirmed,
            &[WorkItemAllowedAction::View, WorkItemAllowedAction::Process],
        );
        assert_eq!(completed, ["VIEW", "PROCESS"]);
    }

    #[test]
    fn unauthorized_projection_masks_current_owner_and_has_no_actions() {
        let view = read_only_work_item_view(&work_item());

        assert_eq!(view.owner_user_id, None);
        assert!(view.allowed_actions.is_empty());
        assert_eq!(view.action_blockers, ["当前账号不在该责任范围，任务仅可查看。"]);
    }

    #[test]
    fn return_for_fix_is_rejected_without_successor() {
        let next = confirmation_next_step(ConfirmationMatrixDecision::FixAndRevalidate);

        assert_eq!(next, ImportBusinessConfirmationNextStep::FixAndRevalidate);
        assert_eq!(
            confirmation_result_status(ConfirmationDecision::ReturnForFix),
            ImportBusinessConfirmationResultStatus::Rejected
        );
    }

    #[test]
    fn last_confirmation_prepares_batch_without_starting_application() {
        let next = confirmation_next_step(ConfirmationMatrixDecision::StartApply);
        let mut import_batch = batch();
        if next == ImportBusinessConfirmationNextStep::StartApply {
            import_batch
                .advance(LegacyImportBatchStatus::ReadyToApply)
                .unwrap();
        }

        assert_eq!(next, ImportBusinessConfirmationNextStep::StartApply);
        assert_eq!(import_batch.status, LegacyImportBatchStatus::ReadyToApply);
    }

    #[test]
    fn replaced_work_item_ids_batch_single_query_semantics() {
        fn replaced(id: &str, scope: &str, trial: u32, work_item: &str) -> LegacyImportConfirmation {
            LegacyImportConfirmation::new(
                LegacyImportConfirmationId::new(id),
                LegacyImportConfirmationData {
                    batch_id: LegacyImportBatchId::new("batch-1"),
                    confirmation_scope: scope.to_string(),
                    owner_role: "role-sales".to_string(),
                    batch_version: 1,
                    trial_version: trial,
                    import_rule_version: "rule-1".to_string(),
                    work_item_id: WorkItemId::new(work_item),
                },
            )
            .unwrap()
        }
        let confirmations = vec![
            replaced("c-1", "SALES", 1, "work-item-1"),
            replaced("c-2", "PROCUREMENT", 1, "work-item-2"),
            replaced("c-3", "OPERATIONS", 2, "work-item-1"),
            replaced("c-4", "WAREHOUSE", 3, "work-item-4"),
        ];
        let ids = replaced_confirmation_work_item_ids(&confirmations, 3);
        assert_eq!(ids.len(), 2);
        assert_eq!(ids[0].to_string(), "work-item-1");
        assert_eq!(ids[1].to_string(), "work-item-2");
        assert!(replaced_confirmation_work_item_ids(&confirmations, 1).is_empty());
        assert!(replaced_confirmation_work_item_ids(&[], 3).is_empty());
    }

    #[test]
    fn superseded_closable_skips_closed_and_fails_closed_on_missing() {
        use std::collections::HashMap;
        fn invalidated(id: &str, scope: &str, work_item: &str) -> LegacyImportConfirmation {
            let mut confirmation = LegacyImportConfirmation::new(
                LegacyImportConfirmationId::new(id),
                LegacyImportConfirmationData {
                    batch_id: LegacyImportBatchId::new("batch-1"),
                    confirmation_scope: scope.to_string(),
                    owner_role: "role-sales".to_string(),
                    batch_version: 1,
                    trial_version: 1,
                    import_rule_version: "rule-1".to_string(),
                    work_item_id: WorkItemId::new(work_item),
                },
            )
            .unwrap();
            confirmation
                .invalidate(
                    LegacyImportConfirmationId::new("c-new"),
                    Instant::from_unix_secs(1_700_000_000),
                )
                .unwrap();
            confirmation
        }
        let batch_id = LegacyImportBatchId::new("batch-1");
        let subject = LegacyImportConfirmation::subject_version(1, 1, "rule-1");
        let open = confirmation_work_item(
            WorkItemId::new("work-item-1"),
            &batch_id,
            subject.clone(),
            "SALES",
            "user-1",
        )
        .unwrap();
        let mut closed = confirmation_work_item(
            WorkItemId::new("work-item-2"),
            &batch_id,
            subject,
            "PROCUREMENT",
            "user-2",
        )
        .unwrap();
        closed
            .close(
                "user-2",
                WorkItemCloseData {
                    close_reason: "SUPERSEDED_BY_NEW_IMPORT_TRIAL".to_string(),
                },
                Instant::from_unix_secs(1_700_000_001),
            )
            .unwrap();
        let confirmations = vec![
            invalidated("c-1", "SALES", "work-item-1"),
            invalidated("c-2", "PROCUREMENT", "work-item-2"),
        ];
        let mut map = HashMap::new();
        map.insert(open.base.id.clone(), open);
        map.insert(closed.base.id.clone(), closed);
        let replacement = LegacyImportConfirmationId::new("c-new");
        let to_close =
            collect_superseded_closable_work_items(&confirmations, 3, &mut map, &replacement).unwrap();
        assert_eq!(to_close.len(), 1);
        assert_eq!(to_close[0].base.id, "work-item-1");
        assert!(map.is_empty());

        let confirmations = vec![invalidated("c-1", "SALES", "work-item-missing")];
        let mut empty = HashMap::new();
        assert!(collect_superseded_closable_work_items(&confirmations, 3, &mut empty, &replacement).is_err());
    }

    #[test]
    fn idempotency_receipt_rejects_same_key_with_different_command() {
        let identity = confirmation_command_identity("user-1", "complete", &completion_command());
        let fingerprint = identity.fingerprint().to_string();
        let receipt = ConfirmationCompletionReceipt {
            result_status: ImportBusinessConfirmationResultStatus::Confirmed,
            task_version: 4,
            batch_version: 5,
            next_step: ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
        };
        let message = confirmation_completion_receipt_message(&fingerprint, receipt);

        assert_eq!(
            parse_confirmation_completion_receipt(&message, &fingerprint).unwrap(),
            receipt
        );
        assert!(parse_confirmation_completion_receipt(&message, &"0".repeat(64)).is_err());
        assert!(!identity.audit_id().contains("request-1"));
    }
}
