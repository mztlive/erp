//! 审批消费的有界组织负责人查询，所有读取沿用调用方事务。
use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::approval_instance;
use mongodb::Database;
use persistence_core::{Executor, Repository};
use serde::Serialize;

use crate::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind};
use crate::repository::OrganizationRepository;
use crate::repository::organization::ORG_UNITS;
use crate::{Error, Result, SharedRbacService};

/// 为审批解析申请人主属组织到根的有效路径，取最近的指定类型组织及其有效负责人。主属缺失或重复、组织停用、循环、层级超限、负责人缺失或读库失败时返回错误。
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

/// 当前账号可用的审批负责人展示摘要，供审批前确认人员及所属组织。
#[derive(Serialize)]
pub struct LeaderPreview {
    /// 已核验审批资格的负责人显示名。
    pub name: String,
    /// 命中负责人规则的团队或部门名称。
    pub organization_name: String,
}

/// 在同一事务内按当前后台账号解析负责人，并确认其同一启用角色同时授予审批读取及决定权限。成功时仅交付姓名和来源组织。
pub async fn own_leader_preview(
    db: Database,
    identity: SharedRbacService,
    actor: AuditActor,
    kind: OrgUnitKind,
) -> Result<LeaderPreview> {
    use persistence_core::Transactional;

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

                let subject = AuditActor::new(leader.user_id.clone(), leader.user_id, AccountKind::Admin);
                let required = [approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION];
                let qualified =
                    identity.qualified_permissions(&subject, &required, ex).await?.ok_or_else(|| {
                        Error::ValidationError("审批负责人账号不存在或已停用，请调整负责人".into())
                    })?;
                if !qualified.same_role(&required) {
                    return Err(Error::ValidationError(
                        "审批负责人缺少审批读取或决定权限，请联系管理员配置角色".into(),
                    ));
                }

                Ok(LeaderPreview {
                    name: qualified.account().display_name.clone(),
                    organization_name: unit.name,
                })
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::organization::OrgLeader;
    // 构造指定类型的测试组织，可选设置负责人，供路径和有效期场景使用。
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
    // 验证旧版组织记录仍能反序列化，显式清空负责人时会写出 `null`。
    #[test]
    fn legacy_units_read_and_cleared_leader_serializes_as_null() {
        let mut value = serde_json::to_value(unit("team", OrgUnitKind::Team, Some("leader"))).unwrap();
        value.as_object_mut().unwrap().remove("leader");
        let legacy: OrgUnit = serde_json::from_value(value).unwrap();
        assert!(legacy.leader.is_none());
        let stored = serde_json::to_value(&legacy).unwrap();
        assert_eq!(stored.get("leader"), Some(&serde_json::Value::Null));
    }

    // 验证查找取最近的指定类型组织，该组织缺负责人时明确报错。
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
    // 验证上级组织停用或负责人尚未生效时，不能把该人作为当前审批负责人。
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
