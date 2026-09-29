//! 把演示岗位放进部门，并让销售领导管理销售部。已符合的关系不重复写入。

use std::collections::HashMap;
use std::future::Future;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::entity::organization::{OrgManagementAssignment, OrgMembership, OrgUnit, OrgUnitKind};
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
    management: Vec<OrgManagementAssignment>,
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
    /// 返回经编译器验证为 `Send` 的 future，完成后部门、成员与管理关系符合规格。
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
            let sales_id = unit_id(&snap, &spec.sales_department, Some(&root_id))?;
            changed |= self.ensure_sales_management(actor, &mut snap, &sales_id, user_ids).await?;
            if changed {
                notices.push("已按演示部门放置岗位账号".to_string());
            }
            Ok(())
        }
    }

    async fn org_snapshot(&self, actor: &AuditActor) -> Result<OrgSnap> {
        let view = organization_service(self.db.clone(), self.rbac.clone()).state(actor).await?;
        Ok(OrgSnap {
            version: view.version,
            units: view.units,
            memberships: view.memberships,
            management: view.management,
        })
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

    async fn ensure_sales_management(
        &self,
        actor: &AuditActor,
        snap: &mut OrgSnap,
        sales_id: &str,
        user_ids: &HashMap<String, String>,
    ) -> Result<bool> {
        let spec = spec::foundation_spec();
        let Some(leader_id) = self.login_id(&spec.sales_leader_account, user_ids).await? else {
            return Ok(false);
        };
        let mut changed = false;
        let grants = snap
            .management
            .iter()
            .filter(|row| {
                row.user_id == leader_id
                    && row.role_id == spec.sales_leader_role_id
                    && row.validity.contains(Instant::now())
            })
            .cloned()
            .collect::<Vec<_>>();
        let exact = grants.iter().find(|row| {
            row.org_unit_id == sales_id && row.include_descendants && row.validity.valid_to.is_none()
        });
        for grant in &grants {
            if exact.is_some_and(|row| row.base.id == grant.base.id) {
                continue;
            }
            self.apply_org(
                actor,
                snap,
                &format!("demo-org-revoke-{}", grant.base.id),
                OrganizationOperation::RevokeManagement { assignment_id: grant.base.id.clone() },
            )
            .await?;
            changed = true;
        }
        if exact.is_none() {
            self.apply_org(
                actor,
                snap,
                "demo-org-grant-sales-leader",
                OrganizationOperation::GrantManagement {
                    user_id: leader_id,
                    role_id: spec.sales_leader_role_id.clone(),
                    org_unit_id: sales_id.to_string(),
                    include_descendants: true,
                    valid_to: None,
                },
            )
            .await?;
            changed = true;
        }
        Ok(changed)
    }

    async fn apply_org(
        &self,
        actor: &AuditActor,
        snap: &mut OrgSnap,
        key: &str,
        operation: OrganizationOperation,
    ) -> Result<()> {
        let request = OrganizationChangeRequest {
            expected_version: snap.version,
            idempotency_key: key.to_string(),
            reason: "演示主数据岗位部门".to_string(),
            change: operation,
        };
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

fn unit_id(snap: &OrgSnap, name: &str, parent_id: Option<&str>) -> Result<String> {
    find_unit(&snap.units, name, parent_id)?
        .ok_or_else(|| Error::Internal(format!("部门 {name} 不存在")))
        .map(|unit| unit.base.id.clone())
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
