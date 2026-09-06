use erp_audit::AuditExt;
use erp_core::ids::WorkItemId;
use erp_identity::AccessControlExt;
use erp_import::LegacyImportExt;
use erp_import::{
    ConfirmationMatrixDecision, ConfirmationScope, LegacyImportBatch, LegacyImportConfirmation,
    LegacyImportConfirmationData, LegacyImportConfirmationId,
};
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use erp_workflow::WorkItemExt;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use services::{Error, Result};

use super::confirmation_query::confirmation_view;
use super::{ImportApplyService, IMPORT_CONFIRMATION_OBJECT_TYPE, IMPORT_CONFIRMATION_ORGANIZATION};
use erp_import::required_text;
use erp_import::{
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep, LegacyImportConfirmationView,
};

impl ImportApplyService {
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
        let work_item = super::factories::confirmation_work_item(
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
                        return Ok::<(LegacyImportConfirmation, WorkItem), services::Error>((
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
                    super::supersede::invalidate_replaced_confirmation(
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
                    Ok::<(LegacyImportConfirmation, WorkItem), services::Error>((
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

/// 将本次决策后的事实替换进内存矩阵。
pub(super) fn replace_confirmation_in_matrix(
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
pub(super) fn confirmation_next_step(
    decision: ConfirmationMatrixDecision,
) -> ImportBusinessConfirmationNextStep {
    match decision {
        ConfirmationMatrixDecision::AwaitOtherConfirmations => {
            ImportBusinessConfirmationNextStep::AwaitOtherConfirmations
        }
        ConfirmationMatrixDecision::StartApply => ImportBusinessConfirmationNextStep::StartApply,
        ConfirmationMatrixDecision::FixAndRevalidate => ImportBusinessConfirmationNextStep::FixAndRevalidate,
    }
}
