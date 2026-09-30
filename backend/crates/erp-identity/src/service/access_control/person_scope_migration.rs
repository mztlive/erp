//! 显式人员范围迁移；预览零写入，应用在单个策略事务内重新计算。
use std::sync::Arc;

use entity_core::BaseModel;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::AccessControlService;
use super::consumers::{WIRED_CONSUMERS, registration, validate_binding, validate_scope_type};
use crate::access_control::{
    AuditEvent, AuditEventData, AuditEventId, AuditEventResult, DataScope, DataScopeSubjectType,
};
use crate::entity::access_control::person_scope::{PersonDataScope, PersonScopeExpression, PersonScopeTerm};
use crate::entity::access_control::person_scope_migration::expression;
use crate::entity::access_control::personal_grant::PersonalBusinessGrant;
use crate::entity::organization_change::OrganizationState;
use crate::ports::ScopeTargetPort;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::repository::access_control::personal_grant::PersonalBusinessGrantRepositoryExt;
use crate::repository::prelude::*;
use crate::{AccessControlExt, Error, Permission, Result, RolePermissionSnapshot, SharedRbacService};

#[derive(Debug, Serialize)]
pub struct PersonScopeMigrationReport {
    pub user_id: String,
    pub policy_version: u64,
    pub skipped_existing: usize,
    pub configurations: Vec<PersonDataScope>,
    pub blockers: Vec<String>,
    pub applied: bool,
}

/// 预览或应用指定人员的迁移；已有人员配置不覆盖。
/// # 参数
/// 数据库、策略服务、明确人员、是否应用。
/// # 返回
/// 候选配置及全部阻断项。
/// # 错误
/// 数据库失败、旧版本或阻断项存在时不写入。
async fn migrate(
    db: Database,
    rbac: SharedRbacService,
    user: String,
    expected_version: Option<u64>,
    targets: Option<Arc<dyn ScopeTargetPort>>,
) -> Result<PersonScopeMigrationReport> {
    let Some(expected_version) = expected_version else {
        return db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { plan(&db, &rbac, &user, &targets, executor).await })
            })
            .await;
    };
    let service = rbac.clone();
    rbac.run_authorized_policy_transaction(expected_version, move |executor| {
        Box::pin(async move {
            let mut report = plan(&db, &service, &user, &targets, executor).await?;
            if !report.blockers.is_empty() {
                return Err(Error::ValidationError(format!("迁移被阻止：{}", report.blockers.join("；"))));
            }
            for config in &report.configurations {
                db.person_data_scopes().create(config, executor).await?;
            }
            let event = AuditEvent::new(
                AuditEventId::new(id_generator::next_id()),
                AuditEventData {
                    actor_id: "system:person-scope-migration".into(),
                    actor_label: "人员范围迁移命令".into(),
                    actor_role: "system".into(),
                    action_type: "person_data_scope.migrate".into(),
                    object_type: "person_data_scope".into(),
                    object_id: Some(user.clone()),
                    object_label: None,
                    request_id: None,
                    trace_id: None,
                    result: AuditEventResult::Success,
                    changed_field_names: vec!["expression".into()],
                    safe_digest: None,
                    source_ip: None,
                    device_context: None,
                },
            )?;
            db.audit_events().create(&event, executor).await?;
            report.applied = true;
            Ok(report)
        })
    })
    .await
}

/// 显式初始化新超级管理员的公司范围；已有配置或撤销记录不重建。
/// # 参数
/// 数据库、人员及调用方事务。
/// # 返回
/// 明确初始化完成。
/// # 错误
/// 数据库失败拒绝原事务。
pub(crate) async fn seed_company(db: &Database, user: &str, executor: &mut dyn Executor) -> Result<()> {
    for (resource, actions, dimensions) in WIRED_CONSUMERS {
        for action in *actions {
            let scope = PersonDataScope {
                base: BaseModel::new(id_generator::next_id()),
                user_id: user.into(),
                resource: (*resource).into(),
                action: (*action).into(),
                expression: PersonScopeExpression {
                    additive: false,
                    history_read: false,
                    alternatives: vec![vec![PersonScopeTerm {
                        scope_type: crate::access_control::DataScopeType::Company,
                        target_dimension: dimensions[0],
                        target_mode: None,
                        include_descendants: None,
                        scope_targets: vec![],
                    }]],
                    condition: None,
                },
            };
            db.person_data_scopes().create(&scope, executor).await?;
        }
    }
    Ok(())
}

/// 只按明确人员与当前有效动作加载迁移输入。
async fn plan(
    db: &Database,
    rbac: &SharedRbacService,
    user: &str,
    targets: &Option<Arc<dyn ScopeTargetPort>>,
    executor: &mut dyn Executor,
) -> Result<PersonScopeMigrationReport> {
    let (snapshot, ids) = migration_identity(db, rbac, user, executor).await?;
    let rules = migration_rules(db, user, &ids, executor).await?;
    let grants = db.personal_business_grants().for_person(user, None, executor).await?;
    let existing = db.person_data_scopes().for_person(user, None, None, executor).await?;
    let mut report = PersonScopeMigrationReport {
        user_id: user.into(),
        policy_version: snapshot.policy_revision(),
        skipped_existing: existing.len(),
        configurations: vec![],
        blockers: vec![],
        applied: false,
    };
    if db.data_scopes().has_legacy_user_limit(user, executor).await? {
        report.blockers.push("存在未完成v2绑定的旧个人范围".into());
    }
    let state = OrganizationRepository::new(db).state(executor).await?;
    state.own_org(user, Instant::now())?;
    for (resource, actions, _) in WIRED_CONSUMERS {
        for action in *actions {
            if existing.iter().any(|s| s.resource == *resource && s.action == *action) {
                continue;
            }
            let eligible = snapshot
                .granting_role_ids(&Permission::parse(format!("{resource}:{action}"))?)
                .into_iter()
                .filter(|id| ids.contains(id))
                .collect::<Vec<_>>();
            if eligible.is_empty() {
                continue;
            }
            match candidate(
                user,
                (resource, action),
                &eligible,
                (&rules, &grants),
                (&state, targets),
                executor,
            )
            .await
            {
                Ok(candidate) => report.configurations.push(candidate),
                Err(error) => report.blockers.push(format!("{resource}:{action}: {error}")),
            }
        }
    }
    Ok(report)
}

/// 使用真实消费者校验与解析拒绝非法目标及绑定。
async fn validate_migration(
    candidate: &PersonDataScope,
    state: &crate::entity::organization_change::OrganizationState,
    targets: &Option<Arc<dyn ScopeTargetPort>>,
    executor: &mut dyn Executor,
) -> Result<()> {
    for term in candidate
        .expression
        .alternatives
        .iter()
        .flatten()
        .chain(candidate.expression.condition.iter().flatten())
    {
        let rule = term.rule(&candidate.resource, &candidate.action, &candidate.user_id, true)?;
        validate_binding(&rule.binding)?;
        validate_scope_type(&candidate.resource, term.scope_type)?;
        if !term.scope_targets.is_empty()
            && term.target_dimension != crate::access_control::ScopeDimension::InternalOrg
        {
            targets
                .as_ref()
                .ok_or_else(|| Error::ValidationError("迁移缺少业务目标身份校验".into()))?
                .validate_targets(term.target_dimension, &term.scope_targets, executor)
                .await?;
        }
    }
    let consumer = registration(&candidate.resource, &candidate.action)?;
    candidate.resolve(state, consumer.required_dimensions, consumer.allows_history, Instant::now())?;
    Ok(())
}

impl AccessControlService {
    /// 显式迁移一名人员的旧规则，预览不写入。
    /// # 参数
    /// 人员身份及预览版本；None 表示仅预览。
    /// # 返回
    /// 配置候选及阻断报告。
    /// # 错误
    /// 无法等价迁移、版本冲突或持久化错误拒绝应用。
    pub async fn migrate_person_scopes(
        &self,
        user: String,
        expected_version: Option<u64>,
    ) -> Result<PersonScopeMigrationReport> {
        migrate(self.db.clone(), self.scope_rbac()?, user, expected_version, self.targets.clone()).await
    }
}

/// 按人员和有效角色读取旧输入并验证完整原绑定。
async fn migration_rules(
    db: &Database,
    user: &str,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<DataScope>> {
    let mut rules = db.data_scopes().list_by_subjects(DataScopeSubjectType::Role, ids, executor).await?;
    rules.extend(db.data_scopes().list_by_subject(DataScopeSubjectType::User, user, executor).await?);
    for rule in &rules {
        if rule.binding.enabled {
            rule.binding.validate(rule.scope_type, &rule.scope_targets)?;
            validate_binding(&rule.binding)?;
            validate_scope_type(&rule.binding.resource, rule.scope_type)?;
        }
    }
    Ok(rules)
}

/// 生成单个候选后用实际解析器与目标端口校验。
async fn candidate(
    user: &str,
    action: (&str, &str),
    eligible: &[String],
    source: (&[DataScope], &[PersonalBusinessGrant]),
    runtime: (&OrganizationState, &Option<Arc<dyn ScopeTargetPort>>),
    executor: &mut dyn Executor,
) -> Result<PersonDataScope> {
    let (resource, action) = action;
    let expression = expression(
        user,
        eligible,
        resource,
        action,
        source.0,
        source.1,
        registration(resource, action)?.allows_history,
    )?;
    let candidate = PersonDataScope {
        base: BaseModel::new(id_generator::next_id()),
        user_id: user.into(),
        resource: resource.into(),
        action: action.into(),
        expression,
    };
    validate_migration(&candidate, runtime.0, runtime.1, executor).await?;
    Ok(candidate)
}

/// 在同一执行器校验迁移账号和当前完整操作资格。
async fn migration_identity(
    db: &Database,
    rbac: &SharedRbacService,
    user: &str,
    executor: &mut dyn Executor,
) -> Result<(RolePermissionSnapshot, Vec<String>)> {
    let account = db
        .accounts()
        .find_by_id(user, executor)
        .await?
        .filter(|a| a.is_active_backoffice())
        .ok_or_else(|| Error::ValidationError("迁移人员不存在或未启用".into()))?;
    let permissions = WIRED_CONSUMERS
        .iter()
        .flat_map(|(r, actions, _)| actions.iter().map(move |a| Permission::parse(format!("{r}:{a}"))))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let snapshot = rbac.role_permission_snapshot(account.kind, user, &permissions).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    let roles = db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
    let ids = roles.iter().map(|r| r.base.id.clone()).collect::<Vec<_>>();
    Ok((snapshot, ids))
}
