//! 把演示岗位放进部门；业务数据范围由人员配置维护。已符合的关系不重复写入。

use std::collections::HashMap;
use std::future::Future;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::organization::{OrgMembership, OrgUnit, OrgUnitKind};
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_identity::{AccessControlExt, AccountCoreRepositoryExt};

use super::DemoMasterDataService;
use super::spec::{self, DepartmentSpec};
use crate::adapters::organization_service;
use crate::{Error, Result};

struct OrgSnap {
    version: u64,
    units: Vec<OrgUnit>,
    memberships: Vec<OrgMembership>,
}

impl DemoMasterDataService {
    /// 按规格补齐部门和成员。账号已在其他部门时改到规格部门。
    ///
    /// # 参数
    /// * `actor` - 当前操作人
    /// * `user_ids` - 演示岗位与账号 ID 的映射
    /// * `notices` - 本次组织调整的提示记录
    ///
    /// # 返回
    /// 返回经编译器验证为 `Send` 的 future，完成后部门与成员符合规格。
    ///
    /// # 错误
    /// 组织查询、调整或规格关系校验失败时返回错误。
    #[expect(
        clippy::manual_async_fn,
        reason = "显式 Send 返回边界用于验证嵌套异步调用，避免调用方的高阶生命周期推导失败"
    )]
    pub(super) fn ensure_departments(
        &self,
        actor: &AuditActor,
        user_ids: &HashMap<String, String>,
        notices: &mut Vec<String>,
    ) -> impl Future<Output = Result<()>> + Send {
        async move {
            let spec = spec::foundation_spec();
            let mut snap = self.org_snapshot(actor).await?;
            let root_id = self.ensure_root(actor, &mut snap).await?;
            let mut changed = false;
            for department in &spec.departments {
                let unit_id = self.ensure_department(actor, &mut snap, &root_id, department).await?;
                changed |=
                    self.seat_members(actor, &mut snap, &unit_id, department, user_ids, notices).await?;
            }
            if changed {
                notices.push("已按演示部门放置岗位账号".to_string());
            }
            Ok(())
        }
    }

    async fn org_snapshot(&self, actor: &AuditActor) -> Result<OrgSnap> {
        let view = organization_service(self.db.clone(), self.rbac.clone()).state(actor).await?;
        Ok(OrgSnap { version: view.version, units: view.units, memberships: view.memberships })
    }

    async fn ensure_root(&self, actor: &AuditActor, snap: &mut OrgSnap) -> Result<String> {
        let name = spec::foundation_spec().root_department.clone();
        if let Some(unit) = find_unit(&snap.units, &name, None)? {
            return live_unit(unit);
        }
        self.apply_org(
            actor,
            snap,
            "demo-org-unit-hq",
            OrganizationOperation::CreateUnit { name, parent_id: None, kind: OrgUnitKind::Department },
        )
        .await?;
        let created = find_unit(&snap.units, &spec::foundation_spec().root_department, None)?
            .ok_or_else(|| Error::Internal("创建总部后未返回部门".to_string()))?;
        Ok(created.base.id.clone())
    }

    async fn ensure_department(
        &self,
        actor: &AuditActor,
        snap: &mut OrgSnap,
        root_id: &str,
        department: &DepartmentSpec,
    ) -> Result<String> {
        if let Some(unit) = find_unit(&snap.units, &department.name, Some(root_id))? {
            return live_unit(unit);
        }
        self.apply_org(
            actor,
            snap,
            &format!("demo-org-unit-{}", department.key),
            OrganizationOperation::CreateUnit {
                name: department.name.clone(),
                parent_id: Some(root_id.to_string()),
                kind: OrgUnitKind::Department,
            },
        )
        .await?;
        let created = find_unit(&snap.units, &department.name, Some(root_id))?
            .ok_or_else(|| Error::Internal(format!("创建{}后未返回部门", department.name)))?;
        Ok(created.base.id.clone())
    }

    async fn seat_members(
        &self,
        actor: &AuditActor,
        snap: &mut OrgSnap,
        unit_id: &str,
        department: &DepartmentSpec,
        user_ids: &HashMap<String, String>,
        notices: &mut Vec<String>,
    ) -> Result<bool> {
        let mut changed = false;
        for login in &department.accounts {
            let Some(user_id) = self.login_id(login, user_ids).await? else {
                notices.push(format!("没有账号 {login}，未放入{}", department.name));
                continue;
            };
            if active_membership(snap, &user_id)?.is_some_and(|row| row.org_unit_id == unit_id) {
                continue;
            }
            self.apply_org(
                actor,
                snap,
                &format!("demo-org-member-{login}"),
                OrganizationOperation::TransferMember { user_id, org_unit_id: unit_id.to_string() },
            )
            .await?;
            changed = true;
        }
        Ok(changed)
    }

    /// 按当前快照构造稳定命令；组织变化后使用新键，避免回放旧版本调整。
    async fn apply_org(
        &self,
        actor: &AuditActor,
        snap: &mut OrgSnap,
        key: &str,
        operation: OrganizationOperation,
    ) -> Result<()> {
        let request = organization_request(key, snap.version, operation);
        let service = organization_service(self.db.clone(), self.rbac.clone());
        service.preview(actor, request.clone()).await?;
        service.change(actor, request).await?;
        *snap = self.org_snapshot(actor).await?;
        Ok(())
    }

    async fn login_id(&self, login: &str, user_ids: &HashMap<String, String>) -> Result<Option<String>> {
        if let Some(id) = user_ids.get(login) {
            return Ok(Some(id.clone()));
        }
        let found = self.db.accounts().find_by_account(login, &mut persistence_core::NoTransaction).await?;
        Ok(found.map(|account| account.base.id))
    }
}

/// 绑定组织版本，使同一请求可重试、后续修复不与历史回执冲突。
fn organization_request(key: &str, version: u64, change: OrganizationOperation) -> OrganizationChangeRequest {
    OrganizationChangeRequest {
        expected_version: version,
        idempotency_key: format!("{key}-v{version}"),
        reason: "演示主数据岗位部门".to_string(),
        change,
    }
}

fn find_unit<'a>(units: &'a [OrgUnit], name: &str, parent_id: Option<&str>) -> Result<Option<&'a OrgUnit>> {
    let matches = units
        .iter()
        .filter(|unit| unit.name == name && unit.parent_id.as_deref() == parent_id)
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Error::ValidationError(format!("部门 {name} 重名，请先合并后重新初始化")));
    }
    Ok(matches.first().copied())
}

fn live_unit(unit: &OrgUnit) -> Result<String> {
    if unit.enabled && unit.kind == OrgUnitKind::Department {
        Ok(unit.base.id.clone())
    } else {
        Err(Error::ValidationError(format!("部门 {} 已停用或类型不匹配", unit.name)))
    }
}

fn active_membership<'a>(snap: &'a OrgSnap, user_id: &str) -> Result<Option<&'a OrgMembership>> {
    let matches = snap
        .memberships
        .iter()
        .filter(|row| row.user_id == user_id && row.validity.contains(Instant::now()))
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err(Error::ValidationError(format!("{user_id} 存在多个有效所属部门")));
    }
    Ok(matches.first().copied())
}

#[cfg(test)]
mod tests {
    use erp_identity::Error as IdentityError;
    use erp_identity::entity::organization_change::OrganizationState;

    use super::*;

    /// 创建可确定时间下执行真实归属迁移的部门状态。
    fn state() -> OrganizationState {
        let unit = |id: &str| {
            OrgUnit::new(id.into(), id.into(), None, OrgUnitKind::Department, "admin".into(), "初始化".into())
                .unwrap()
        };
        OrganizationState {
            version: 1,
            units: vec![unit("sales"), unit("other")],
            ..OrganizationState::default()
        }
    }

    /// 同输入重试保持完整请求一致；人工转部门后的修复采用新键并恢复归属。
    #[test]
    fn member_repair_after_org_change_uses_new_receipt_identity() {
        let target =
            OrganizationOperation::TransferMember { user_id: "seller".into(), org_unit_id: "sales".into() };
        let original = organization_request("demo-org-member-seller", 1, target.clone());
        let retry = organization_request("demo-org-member-seller", 1, target.clone());
        assert_eq!(original, retry);
        original.validate().unwrap();
        let seated = state().changed(&original, "seat-1", "admin", Instant::from_unix_secs(10)).unwrap();
        let move_request = organization_request(
            "manual-transfer",
            seated.version,
            OrganizationOperation::TransferMember { user_id: "seller".into(), org_unit_id: "other".into() },
        );
        let moved = seated.changed(&move_request, "seat-2", "admin", Instant::from_unix_secs(20)).unwrap();
        assert_eq!(moved.own_org("seller", Instant::from_unix_secs(20)).unwrap(), Some("other"));
        let repair = organization_request("demo-org-member-seller", moved.version, target);
        assert_ne!(repair.idempotency_key, original.idempotency_key);
        let repaired = moved.changed(&repair, "seat-3", "admin", Instant::from_unix_secs(30)).unwrap();
        assert_eq!(repaired.own_org("seller", Instant::from_unix_secs(30)).unwrap(), Some("sales"));
        assert!(matches!(
            moved.changed(&original, "stale", "admin", Instant::from_unix_secs(30)),
            Err(IdentityError::ConflictError(_))
        ));
    }
}
