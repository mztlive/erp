//! 显式维护查询资格。与角色和业务命令授权独立，支持自定义岗位和终止后再授予。

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};
use serde::Deserialize;

use super::PersonDirectoryService;
use crate::entity::person_directory::{
    PersonDirectoryCategory, PersonQueryQualification, PersonQueryStatus, candidate_in_directory,
};
use crate::repository::prelude::*;
use crate::repository::OrganizationRepository;
use crate::service::access_control::resolve::DataScopeService;
use crate::{AccessControlExt, Error, Result};

/// 资格维护请求。新建不传版本，终止或再授予必须回传读取的版本。
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualificationChange {
    pub status: PersonQueryStatus,
    pub version: Option<u64>,
    pub reason: String,
}

impl PersonDirectoryService {
    /// 读取或维护单个账号的查询资格；授权在事务内按目标账号及有效主属组织判定。
    ///
    /// # 错误
    /// 非岗位类别、无范围、目标非后台账号、版本冲突或持久化失败时拒绝。
    pub async fn qualification(
        &self,
        actor: AuditActor,
        category: PersonDirectoryCategory,
        account_id: String,
        change: Option<QualificationChange>,
    ) -> Result<Option<PersonQueryQualification>> {
        if category == PersonDirectoryCategory::Business {
            return Err(Error::ValidationError("后台人员目录无需查询资格".into()));
        }
        let db = self.db.clone();
        let access = DataScopeService::new(db.clone(), self.rbac.clone());
        db.client()
            .clone()
            .with_transaction(move |executor| {
                let db = db.clone();
                let access = access.clone();
                let actor = actor.clone();
                let account_id = account_id.clone();
                let change = change.clone();
                Box::pin(async move {
                    manage_target(&db, &access, &actor, category, &account_id, change, executor).await
                })
            })
            .await
    }
}

/// 在同一事务内校验目标范围并读写资格。
async fn manage_target(
    db: &mongodb::Database,
    access: &DataScopeService,
    actor: &AuditActor,
    category: PersonDirectoryCategory,
    account_id: &str,
    change: Option<QualificationChange>,
    executor: &mut dyn Executor,
) -> Result<Option<PersonQueryQualification>> {
    let mut scope = access.resolve(actor, "person_query_qualification", "manage", executor).await?;
    scope.organizations.memberships = OrganizationRepository::new(db)
        .directory_memberships(Some(&[account_id.to_owned()]), None, scope.as_of, executor).await?;
    let org = scope.organizations.own_org(account_id, scope.as_of)?;
    if !candidate_in_directory(&scope.scope, actor.id(), account_id, org, None) {
        return Err(Error::Forbidden("目标账号不在查询资格管理范围内".into()));
    }
    let account = db
        .accounts()
        .find_by_id(account_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("后台账号不存在".into()))?;
    if account.kind != AccountKind::Admin {
        return Err(Error::ValidationError("只允许维护后台账号查询资格".into()));
    }
    let current = db.person_query_qualifications().find_for_account(account_id, category, executor).await?;
    let Some(change) = change else {
        return Ok(current);
    };
    let mut updated = apply_change(current.clone(), account_id, category, &change, actor)?;
    if current.is_some() {
        db.person_query_qualifications().update(&mut updated, executor).await?;
    } else {
        db.person_query_qualifications().create(&updated, executor).await?;
    }
    record_change(db, actor, &updated, executor).await?;
    Ok(Some(updated))
}

/// 对显式维护执行版本与状态校验，终止资格只能通过本入口显式恢复。
fn apply_change(
    current: Option<PersonQueryQualification>,
    account_id: &str,
    category: PersonDirectoryCategory,
    change: &QualificationChange,
    actor: &AuditActor,
) -> Result<PersonQueryQualification> {
    let reason = change.reason.trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(Error::ValidationError("维护原因须为 1 到 500 字".into()));
    }
    let mut row = match current {
        Some(row) if Some(row.base.version) == change.version => row,
        Some(_) => return Err(Error::ConflictError("查询资格版本已变化".into())),
        None if change.version.is_none() && change.status == PersonQueryStatus::Active => {
            let mut row = PersonQueryQualification::grant(next_id(), account_id, category)?;
            row.grant_role_id = None;
            row
        },
        None => return Err(Error::ConflictError("查询资格不存在，须先显式授予".into())),
    };
    row.status = change.status;
    row.terminated_at = (change.status == PersonQueryStatus::Terminated).then(Instant::now);
    row.managed_by = Some(actor.id().to_owned());
    row.management_reason = Some(reason.to_owned());
    Ok(row)
}

/// 将资格维护与不可变审计事件一起提交。
async fn record_change(
    db: &mongodb::Database,
    actor: &AuditActor,
    row: &PersonQueryQualification,
    executor: &mut dyn Executor,
) -> Result<()> {
    use crate::entity::access_control::{AuditEvent, AuditEventData, AuditEventId, AuditEventResult};
    let event = AuditEvent::new(
        AuditEventId::new(next_id()),
        AuditEventData {
            actor_id: actor.id().to_owned(),
            actor_label: actor.account().to_owned(),
            actor_role: actor.kind().as_str().to_owned(),
            action_type: "person_query_qualification.change".into(),
            object_type: "person_query_qualification".into(),
            object_id: Some(row.base.id.clone()),
            object_label: None,
            request_id: None,
            trace_id: None,
            result: AuditEventResult::Success,
            changed_field_names: vec!["status".into()],
            safe_digest: row.management_reason.clone(),
            source_ip: None,
            device_context: None,
        },
    )?;
    db.audit_events().create(&event, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_grant_termination_and_restore_require_current_version() {
        let actor = AuditActor::new("admin".into(), "admin".into(), AccountKind::Admin);
        let mut request = QualificationChange {
            status: PersonQueryStatus::Active,
            version: None,
            reason: "岗位调整".into(),
        };
        let row =
            apply_change(None, "custom-role-user", PersonDirectoryCategory::Sales, &request, &actor).unwrap();
        assert_eq!(row.grant_role_id, None);
        request.status = PersonQueryStatus::Terminated;
        assert!(
            apply_change(
                Some(row.clone()),
                "custom-role-user",
                PersonDirectoryCategory::Sales,
                &request,
                &actor
            )
            .is_err()
        );
        request.version = Some(row.base.version);
        let terminated =
            apply_change(Some(row), "custom-role-user", PersonDirectoryCategory::Sales, &request, &actor)
                .unwrap();
        assert!(terminated.terminated_at.is_some());
        request.status = PersonQueryStatus::Active;
        let restored = apply_change(
            Some(terminated),
            "custom-role-user",
            PersonDirectoryCategory::Sales,
            &request,
            &actor,
        )
        .unwrap();
        assert_eq!(restored.status, PersonQueryStatus::Active);
        assert!(restored.terminated_at.is_none());
        request.reason.clear();
        assert!(
            apply_change(
                Some(restored),
                "custom-role-user",
                PersonDirectoryCategory::Sales,
                &request,
                &actor
            )
            .is_err()
        );
    }
}
