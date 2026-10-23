//! 审批消费的有界组织负责人查询，所有读取沿用调用方事务。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Repository};
use serde::Serialize;

use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind};
use crate::repository::OrganizationRepository;
use crate::repository::organization::ORG_UNITS;
use crate::{Error, Result};

/// 查询申请人主属组织到根的完整有效路径，按最近同类组织取得负责人。
/// # 参数
/// 数据库、申请人、团队或部门类型、同一解析时点及调用方执行器。
/// # 返回
/// 返回含唯一负责人及生效依据的组织节点。
/// # 错误
/// 主属缺失或重复、组织失效、循环、层级超限、负责人缺失或读取失败时拒绝。
pub async fn applicant_leader(
    db: &Database,
    applicant: &str,
    kind: OrgUnitKind,
    at: Instant,
    ex: &mut dyn Executor,
) -> Result<OrgUnit> {
    let memberships = OrganizationRepository::new(db).primary_memberships(applicant, at, ex).await?;
    let primary = OrgMembership::primary_org(&memberships, applicant, at)?
        .ok_or_else(|| Error::ValidationError("申请人尚未设置所属组织，请在组织与人员中调整部门".into()))?;
    let repository = Repository::<OrgUnit>::new(db, ORG_UNITS);
    let mut id = Some(primary.to_owned());
    let mut path = Vec::new();
    let mut seen = HashSet::new();
    while let Some(current) = id {
        if !seen.insert(current.clone()) || path.len() >= 100 {
            return Err(Error::ValidationError("组织路径循环或超过100级，请检查组织配置".into()));
        }
        let unit = repository
            .find_by_id(&current, ex)
            .await?
            .ok_or_else(|| Error::ValidationError("申请人所属组织路径不完整，请检查组织配置".into()))?;
        id = unit.parent_id.clone();
        path.push(unit);
    }
    OrgUnit::leader_on_path(path, kind, at)
}

/// 当前账号负责人预览；不返回人员账号或配置权限。
#[derive(Serialize)]
pub struct LeaderPreview {
    pub name: String,
    pub organization_name: String,
}

/// 在同一只读事务内预览当前内部账号的负责人及审批资格。
/// # 参数
/// 数据库、认证账号和组织类型；不接受其他人员身份。
/// # 返回
/// 负责人姓名和来源组织。
/// # 错误
/// 非内部账号、组织缺失、负责人或审批资格失效时拒绝。
pub async fn own_leader_preview(db: Database, actor: AuditActor, kind: OrgUnitKind) -> Result<LeaderPreview> {
    use persistence_core::Transactional;

    use super::qualification::qualified_account;
    use crate::AccessControlExt;
    db.client()
        .clone()
        .with_transaction(move |ex| {
            Box::pin(async move {
                let applicant = db
                    .accounts()
                    .find_by_id(actor.id(), ex)
                    .await?
                    .filter(|a| a.is_active_backoffice() && a.kind == actor.kind())
                    .ok_or_else(|| Error::Forbidden("当前账号不可查询内部组织负责人".into()))?;
                let unit = applicant_leader(&db, &applicant.base.id, kind, Instant::now(), ex).await?;
                let leader = unit.leader.ok_or_else(|| Error::ValidationError("组织负责人未设置".into()))?;
                let person = qualified_account(
                    &db,
                    &leader.user_id,
                    &["approval_instance:read", "approval_instance:decide"],
                    ex,
                )
                .await?;
                Ok(LeaderPreview { name: person.name, organization_name: unit.name })
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::organization::OrgLeader;
    fn unit(id: &str, kind: OrgUnitKind, leader: Option<&str>) -> OrgUnit {
        let mut unit = OrgUnit::new(id.into(), id.into(), None, kind, "admin".into(), "测试".into()).unwrap();
        unit.leader = leader.map(|user| OrgLeader {
            assignment_id: format!("{id}-assignment"),
            user_id: user.into(),
            assigned_at: Instant::from_unix_secs(1),
            assigned_by: "admin".into(),
        });
        unit
    }
    #[test]
    fn legacy_units_read_and_cleared_leader_serializes_as_null() {
        let mut value = serde_json::to_value(unit("team", OrgUnitKind::Team, Some("leader"))).unwrap();
        value.as_object_mut().unwrap().remove("leader");
        let legacy: OrgUnit = serde_json::from_value(value).unwrap();
        assert!(legacy.leader.is_none());
        let stored = serde_json::to_value(&legacy).unwrap();
        assert_eq!(stored.get("leader"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn nearest_kind_and_missing_leader_are_explicit() {
        let team = unit("一组", OrgUnitKind::Team, Some("组长"));
        let department = unit("销售部", OrgUnitKind::Department, Some("经理"));
        let path = vec![team.clone(), department.clone()];
        let at = Instant::from_unix_secs(10);
        assert_eq!(
            OrgUnit::leader_on_path(path.clone(), OrgUnitKind::Team, at).unwrap().leader.unwrap().user_id,
            "组长"
        );
        assert_eq!(
            OrgUnit::leader_on_path(path, OrgUnitKind::Department, at).unwrap().leader.unwrap().user_id,
            "经理"
        );
        assert!(OrgUnit::leader_on_path(vec![department.clone()], OrgUnitKind::Team, at).is_err());
        let mut missing = team;
        missing.leader = None;
        assert!(OrgUnit::leader_on_path(vec![missing, department], OrgUnitKind::Team, at).is_err());
    }
    #[test]
    fn disabled_ancestor_and_future_assignment_are_rejected() {
        let team = unit("一组", OrgUnitKind::Team, Some("组长"));
        let mut department = unit("销售部", OrgUnitKind::Department, Some("经理"));
        department.enabled = false;
        assert!(
            OrgUnit::leader_on_path(
                vec![team.clone(), department],
                OrgUnitKind::Team,
                Instant::from_unix_secs(10)
            )
            .is_err()
        );
        assert!(OrgUnit::leader_on_path(vec![team], OrgUnitKind::Team, Instant::from_unix_secs(0)).is_err());
    }
}
