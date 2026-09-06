use std::collections::HashMap;

use database::{AccessControlExt, LegacyImportExt, WorkItemExt};
use entities::legacy_import::{
    confirmation_work_item, ConfirmationMatrixDecision, ConfirmationScope, ConfirmationStatus,
    LegacyImportBatch, LegacyImportConfirmation, LegacyImportConfirmationData, LegacyImportConfirmationId,
};
use entities::work_item::{WorkItem, WorkItemCloseData, WorkItemStatus, WorkItemType};
use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use crate::audit::AuditActorLogs;
use crate::errors::{Error, Result};
use application_core::AuditActor;

use super::super::dto::{
    CreateLegacyImportConfirmationRequest, ImportBusinessConfirmationNextStep, LegacyImportConfirmationView,
};
use super::super::receipt::required_text;
use super::super::{LegacyImportService, IMPORT_CONFIRMATION_OBJECT_TYPE, IMPORT_CONFIRMATION_ORGANIZATION};
use super::query::confirmation_view;

impl LegacyImportService {
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
pub(super) fn collect_superseded_closable_work_items(
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
pub(super) fn replaced_confirmation_work_item_ids(
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
