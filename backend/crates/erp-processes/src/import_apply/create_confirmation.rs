use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::entity::AuditLog;
use erp_core::ids::WorkItemId;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_import::repository::prelude::*;
use erp_import::{
    ConfirmationMatrixDecision, ConfirmationScope, CreateLegacyImportConfirmationRequest,
    ImportBusinessConfirmationNextStep, LegacyImportBatch, LegacyImportConfirmation,
    LegacyImportConfirmationData, LegacyImportConfirmationId, LegacyImportExt, required_text,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemType};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::confirmation_query::confirmation_view;
use super::dto::LegacyImportConfirmationView;
use super::{IMPORT_CONFIRMATION_OBJECT_TYPE, IMPORT_CONFIRMATION_ORGANIZATION, ImportApplyProcess};
use crate::audit::persist_log;
use crate::{Error, Result};

impl ImportApplyProcess {
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
        let prepared = PreparedConfirmationCreation::new(req, actor)?;
        let identity = prepared.identity.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let transaction_result = client
            .with_transaction(move |executor| Box::pin(async move { prepared.execute(&db, executor).await }))
            .await;
        let (confirmation, work_item) = match transaction_result {
            Ok(result) => result,
            Err(error) => match identity.existing(&self.db, &mut NoTransaction).await? {
                Some(result) => result,
                None => return Err(error),
            },
        };
        Ok(confirmation_view(confirmation, &work_item))
    }
}

/// 创建与提交失败恢复共用的精确身份，保留原规范化与首错顺序。
#[derive(Clone)]
struct ConfirmationCreationIdentity {
    req: CreateLegacyImportConfirmationRequest,
    scope: String,
    owner_role: String,
    import_rule_version: String,
    subject_version: String,
}

impl ConfirmationCreationIdentity {
    fn new(req: CreateLegacyImportConfirmationRequest) -> Result<Self> {
        req.validate()?;
        let scope = ConfirmationScope::parse(&req.confirmation_scope)?;
        let owner_role = scope.owner_role().to_string();
        let scope = scope.as_str().to_string();
        let import_rule_version = required_text(&req.import_rule_version, "导入规则版本不能为空")?;
        let subject_version = LegacyImportConfirmation::subject_version(
            req.batch_version,
            req.trial_version,
            &import_rule_version,
        );
        Ok(Self { req, scope, owner_role, import_rule_version, subject_version })
    }

    async fn existing(
        &self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<Option<(LegacyImportConfirmation, WorkItem)>> {
        let Some(confirmation) = db
            .legacy_import_confirmations()
            .find_by_batch_scope_trial(&self.req.batch_id, &self.scope, self.req.trial_version, executor)
            .await?
        else {
            return Ok(None);
        };
        let work_item = db
            .work_items()
            .find_by_id(confirmation.work_item_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::Internal("导入确认任务关联缺失".to_string()))?;
        validate_confirmation_creation_replay(
            &confirmation,
            &work_item,
            &self.req,
            &self.scope,
            &self.owner_role,
            &self.import_rule_version,
            &self.subject_version,
        )?;
        Ok(Some((confirmation, work_item)))
    }
}

/// 事务前准备 ID、任务及审计，事务内只执行原子事实读写。
struct PreparedConfirmationCreation {
    identity: ConfirmationCreationIdentity,
    confirmation: LegacyImportConfirmation,
    work_item: WorkItem,
    audit: AuditLog,
    actor_id: String,
}

impl PreparedConfirmationCreation {
    fn new(req: CreateLegacyImportConfirmationRequest, actor: &AuditActor) -> Result<Self> {
        let identity = ConfirmationCreationIdentity::new(req)?;
        let confirmation_id = LegacyImportConfirmationId::new(next_id());
        let work_item_id = WorkItemId::new(next_id());
        let confirmation = LegacyImportConfirmation::new(
            confirmation_id,
            LegacyImportConfirmationData {
                batch_id: identity.req.batch_id.clone(),
                confirmation_scope: identity.scope.clone(),
                owner_role: identity.owner_role.clone(),
                batch_version: identity.req.batch_version,
                trial_version: identity.req.trial_version,
                import_rule_version: identity.import_rule_version.clone(),
                work_item_id: work_item_id.clone(),
            },
        )?;
        let work_item = super::factories::confirmation_work_item(
            work_item_id,
            &identity.req.batch_id,
            identity.subject_version.clone(),
            &identity.scope,
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "legacy_import_confirmation.create",
            "legacy_import_confirmation",
            confirmation.base.id.clone(),
        )?;
        Ok(Self { identity, confirmation, work_item, audit, actor_id: actor.id().to_string() })
    }

    async fn execute(
        self,
        db: &Database,
        executor: &mut dyn Executor,
    ) -> Result<(LegacyImportConfirmation, WorkItem)> {
        if let Some(existing) = self.identity.existing(db, executor).await? {
            return Ok(existing);
        }
        let mut batch = db
            .legacy_import_batches()
            .find_by_id(self.identity.req.batch_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("导入批次不存在".to_string()))?;
        validate_confirmation_creation_batch(
            &batch,
            &self.identity.scope,
            &self.identity.import_rule_version,
        )?;
        batch.prepare_confirmation()?;
        let enabled_roles =
            db.roles().enabled_roles(std::slice::from_ref(&self.identity.owner_role), executor).await?;
        if enabled_roles.len() != 1 {
            return Err(Error::BusinessLogicError("导入确认责任角色未注册或已停用".to_string()));
        }
        self.persist(db, batch, executor).await
    }

    async fn persist(
        self,
        db: &Database,
        mut batch: LegacyImportBatch,
        executor: &mut dyn Executor,
    ) -> Result<(LegacyImportConfirmation, WorkItem)> {
        let mut confirmations =
            db.legacy_import_confirmations().list_by_batch(&self.identity.req.batch_id, executor).await?;
        validate_trial_snapshot(
            &batch,
            &confirmations,
            &self.identity.req,
            &self.identity.import_rule_version,
        )?;
        super::supersede::invalidate_replaced_confirmation(
            db,
            &mut confirmations,
            &self.confirmation,
            &self.actor_id,
            executor,
        )
        .await?;
        let mut current_matrix = LegacyImportConfirmation::current_matrix(
            &confirmations,
            self.identity.req.batch_version,
            self.identity.req.trial_version,
            &self.identity.import_rule_version,
        );
        current_matrix.push(self.confirmation.clone());
        batch.update_summaries(
            batch.failure_code_summary.clone(),
            Some(LegacyImportConfirmation::matrix_summary(self.identity.req.trial_version, &current_matrix)),
        )?;
        db.legacy_import_confirmations().create(&self.confirmation, executor).await?;
        db.work_items().create(&self.work_item, executor).await?;
        db.legacy_import_batches().update(&mut batch, executor).await?;
        persist_log(db, &self.audit, executor).await?;
        Ok((self.confirmation, self.work_item))
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
        return Err(Error::BusinessLogicError("该责任范围不属于当前批次的必要确认矩阵".to_string()));
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
    Err(Error::ConflictError("同一批次、范围与试算版本已用于不同的确认任务".to_string()))
}

/// 将本次决策后的事实替换进内存矩阵。
///
/// # 参数
/// * `confirmations` - 当前确认矩阵。
/// * `decided` - 已决策的确认事实。
///
/// # 返回
/// 找到相同 `id` 时就地替换；矩阵中不存在该事实时保持原样。
///
/// # 错误
/// 不返回错误。
pub(super) fn replace_confirmation_in_matrix(
    confirmations: &mut [LegacyImportConfirmation],
    decided: &LegacyImportConfirmation,
) {
    if let Some(current) = confirmations.iter_mut().find(|item| item.base.id == decided.base.id) {
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
///
/// # 错误
/// 不返回错误。
pub(super) fn confirmation_next_step(
    decision: ConfirmationMatrixDecision,
) -> ImportBusinessConfirmationNextStep {
    match decision {
        ConfirmationMatrixDecision::AwaitOtherConfirmations => {
            ImportBusinessConfirmationNextStep::AwaitOtherConfirmations
        },
        ConfirmationMatrixDecision::StartApply => ImportBusinessConfirmationNextStep::StartApply,
        ConfirmationMatrixDecision::FixAndRevalidate => ImportBusinessConfirmationNextStep::FixAndRevalidate,
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::ids::LegacyImportBatchId;
    use erp_import::{CreateLegacyImportConfirmationRequest, LegacyImportConfirmation};

    use super::{
        ConfirmationCreationIdentity, PreparedConfirmationCreation, validate_confirmation_creation_replay,
    };
    use crate::Error;

    fn request() -> CreateLegacyImportConfirmationRequest {
        CreateLegacyImportConfirmationRequest {
            batch_id: LegacyImportBatchId::new("batch-creation"),
            confirmation_scope: " sales ".into(),
            batch_version: 4,
            trial_version: 2,
            import_rule_version: " rule-1 ".into(),
        }
    }

    #[test]
    fn prepared_confirmation_links_normalized_identity_task_and_actor() {
        let actor = AuditActor::new("actor-creation".into(), "sales".into(), AccountKind::Admin);
        let prepared = PreparedConfirmationCreation::new(request(), &actor).unwrap();
        let identity = &prepared.identity;
        assert_eq!(identity.scope, "SALES");
        assert_eq!(identity.owner_role, "role-sales");
        assert_eq!(identity.import_rule_version, "rule-1");
        assert_eq!(identity.subject_version, LegacyImportConfirmation::subject_version(4, 2, "rule-1"));
        assert_eq!(prepared.confirmation.work_item_id.as_ref(), prepared.work_item.base.id);
        assert_eq!(prepared.work_item.owner_user_id.as_deref(), Some("actor-creation"));
        assert_eq!(prepared.audit.resource_id.as_deref(), Some(prepared.confirmation.base.id.as_str()));
        validate_confirmation_creation_replay(
            &prepared.confirmation,
            &prepared.work_item,
            &identity.req,
            &identity.scope,
            &identity.owner_role,
            &identity.import_rule_version,
            &identity.subject_version,
        )
        .unwrap();
        let mut changed_task = prepared.work_item.clone();
        changed_task.subject_version = "foreign-trial".into();
        let error = validate_confirmation_creation_replay(
            &prepared.confirmation,
            &changed_task,
            &identity.req,
            &identity.scope,
            &identity.owner_role,
            &identity.import_rule_version,
            &identity.subject_version,
        )
        .unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "同一批次、范围与试算版本已用于不同的确认任务")
        );
    }

    #[test]
    fn confirmation_creation_rejects_unregistered_scope_before_building_facts() {
        let mut req = request();
        req.confirmation_scope = "UNREGISTERED".into();
        let error = match ConfirmationCreationIdentity::new(req) {
            Ok(_) => panic!("unregistered scope must fail"),
            Err(error) => error,
        };
        assert!(matches!(error, Error::Logic(_)));
    }
}
