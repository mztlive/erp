//! 组织变更的确定性状态迁移；写入、审计与版本推进由 Service 事务提交。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::organization::*;
use crate::{Error, Result};

/// 同一事务读取的组织事实集合。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationState {
    pub version: u64,
    pub units: Vec<OrgUnit>,
    pub memberships: Vec<OrgMembership>,
    pub management: Vec<OrgManagementAssignment>,
}

/// 组织管理命令；实体身份在服务端产生，不接受静默覆盖。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OrganizationOperation {
    CreateUnit {
        name: String,
        parent_id: Option<String>,
        kind: OrgUnitKind,
    },
    MoveUnit {
        org_unit_id: String,
        parent_id: Option<String>,
    },
    RenameUnit {
        org_unit_id: String,
        name: String,
    },
    DisableUnit {
        org_unit_id: String,
    },
    TransferMember {
        user_id: String,
        org_unit_id: String,
    },
    EndMembership {
        user_id: String,
    },
    GrantManagement {
        user_id: String,
        role_id: String,
        org_unit_id: String,
        include_descendants: bool,
        valid_to: Option<Instant>,
    },
    RevokeManagement {
        assignment_id: String,
    },
}

/// 变更请求必须携带全局组织版本及幂等身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationChangeRequest {
    pub expected_version: u64,
    pub idempotency_key: String,
    pub reason: String,
    pub change: OrganizationOperation,
}

/// 组织版本单例；所有拓扑及时间区间写入争用同一版本，阻止并发写偏差。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrganizationRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    pub revision: u64,
}

/// 幂等回执兼组织变更审计；不保存账号凭证。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrganizationChangeReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub actor_id: String,
    pub request: OrganizationChangeRequest,
    pub before: OrganizationState,
    pub after: OrganizationState,
    pub as_of: Instant,
}

impl OrganizationChangeRequest {
    /// 校验版本请求的原因和幂等键。
    ///
    /// # 错误
    /// 空白、过长或包含非安全字符的幂等键拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.idempotency_key.is_empty()
            || self.idempotency_key.len() > 128
            || !self
                .idempotency_key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(Error::ValidationError(
                "幂等键必须为不超过128位的字母、数字、下划线或短横线".into(),
            ));
        }
        if self.reason.trim().is_empty() || self.reason.chars().count() > 1000 {
            return Err(Error::ValidationError("必须填写不超过1000字的变更原因".into()));
        }
        Ok(())
    }
}

impl OrganizationState {
    /// 按当前版本准备变更结果，失败不修改调用方状态。
    ///
    /// # 错误
    /// 版本冲突、无效节点、环、重叠有效期或非法停用均拒绝。
    pub fn changed(
        &self,
        request: &OrganizationChangeRequest,
        id: &str,
        actor: &str,
        at: Instant,
    ) -> Result<Self> {
        request.validate()?;
        if request.expected_version != self.version {
            return Err(Error::ConflictError("组织范围已变化，请刷新后重试".into()));
        }
        let mut next = self.clone();
        next.apply(&request.change, id, actor, request.reason.trim(), at)?;
        OrgTree::new(&next.units)?;
        next.validate_memberships()?;
        next.version = self
            .version
            .checked_add(1)
            .ok_or_else(|| Error::Internal("组织版本溢出".into()))?;
        Ok(next)
    }

    /// 查询当前唯一主属组织；重复关系失败关闭，不默认放入根组织。
    ///
    /// # 错误
    /// 同一时点存在多条主属关系时返回冲突错误。
    pub fn own_org(&self, user_id: &str, at: Instant) -> Result<Option<&str>> {
        let values = self
            .memberships
            .iter()
            .filter(|m| m.user_id == user_id && !m.base.is_deleted() && m.validity.contains(at))
            .collect::<Vec<_>>();
        if values.len() > 1 {
            return Err(Error::ConflictError("用户存在重叠主属组织关系".into()));
        }
        Ok(values.first().map(|m| m.org_unit_id.as_str()))
    }

    /// 校验单用户全部有效期不重叠，含未来关系。
    fn validate_memberships(&self) -> Result<()> {
        let mut members = self
            .memberships
            .iter()
            .filter(|m| !m.base.is_deleted())
            .collect::<Vec<_>>();
        members.sort_by_key(|m| (&m.user_id, m.validity.valid_from));
        for m in &members {
            m.validity.validate()?;
        }
        if members
            .windows(2)
            .any(|pair| pair[0].user_id == pair[1].user_id && pair[0].validity.overlaps(&pair[1].validity))
        {
            return Err(Error::ConflictError("同一用户主属组织有效期不得重叠".into()));
        }
        Ok(())
    }

    /// 分派固定命令；业务责任与任务字段不属于组织状态，不参与自动更新。
    fn apply(
        &mut self,
        change: &OrganizationOperation,
        id: &str,
        actor: &str,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        match change {
            OrganizationOperation::CreateUnit { .. }
            | OrganizationOperation::MoveUnit { .. }
            | OrganizationOperation::RenameUnit { .. } => self.change_unit(change, id, actor, reason)?,
            OrganizationOperation::DisableUnit { org_unit_id } => {
                self.disable(org_unit_id, actor, reason, at)?
            }
            OrganizationOperation::TransferMember { user_id, org_unit_id } => {
                self.transfer(user_id, org_unit_id, id, actor, reason, at)?
            }
            OrganizationOperation::EndMembership { user_id } => {
                self.end_membership(user_id, actor, reason, at)?
            }
            OrganizationOperation::GrantManagement { .. } => {
                self.grant_management(change, id, actor, reason, at)?
            }
            OrganizationOperation::RevokeManagement { assignment_id } => {
                self.revoke_management(assignment_id, at)?
            }
        }
        Ok(())
    }

    /// 修改组织节点身份资料；涉及完整树的不变式由变更入口统一校验。
    fn change_unit(
        &mut self,
        change: &OrganizationOperation,
        id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<()> {
        match change {
            OrganizationOperation::CreateUnit {
                name,
                parent_id,
                kind,
            } => {
                if let Some(parent) = parent_id {
                    self.ensure_enabled(parent)?;
                }
                self.units.push(OrgUnit::new(
                    id.into(),
                    name.clone(),
                    parent_id.clone(),
                    *kind,
                    actor.into(),
                    reason.into(),
                )?);
            }
            OrganizationOperation::MoveUnit {
                org_unit_id,
                parent_id,
            } => {
                if let Some(parent) = parent_id {
                    self.ensure_enabled(parent)?;
                }
                self.unit_mut(org_unit_id, actor, reason)?.parent_id = parent_id.clone();
            }
            OrganizationOperation::RenameUnit { org_unit_id, name } => {
                let name = erp_core::validation::normalize_required_text(
                    name.clone(),
                    "组织名称不能为空",
                    100,
                    "组织名称过长",
                )?;
                self.unit_mut(org_unit_id, actor, reason)?.name = name;
            }
            _ => return Err(Error::Internal("错误的组织节点命令".into())),
        }
        Ok(())
    }

    /// 新建按角色绑定的管理关系，并校验组织可用性和 UTC 有效期。
    fn grant_management(
        &mut self,
        change: &OrganizationOperation,
        id: &str,
        actor: &str,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        match change {
            OrganizationOperation::GrantManagement {
                user_id,
                role_id,
                org_unit_id,
                include_descendants,
                valid_to,
            } => {
                self.ensure_enabled(org_unit_id)?;
                let validity = OrgValidity {
                    valid_from: at,
                    valid_to: *valid_to,
                };
                validity.validate()?;
                self.management.push(OrgManagementAssignment {
                    base: BaseModel::new(id.into()),
                    user_id: user_id.clone(),
                    role_id: role_id.clone(),
                    org_unit_id: org_unit_id.clone(),
                    include_descendants: *include_descendants,
                    validity,
                    granted_by: actor.into(),
                    reason: reason.into(),
                });
            }
            _ => return Err(Error::Internal("错误的管理关系命令".into())),
        }
        Ok(())
    }

    /// 加载可用节点并保存此次操作人和原因。
    fn unit_mut(&mut self, id: &str, actor: &str, reason: &str) -> Result<&mut OrgUnit> {
        self.ensure_enabled(id)?;
        let unit = self
            .units
            .iter_mut()
            .find(|u| u.base.id == id)
            .ok_or_else(|| Error::NotFound("组织不存在".into()))?;
        unit.changed_by = actor.into();
        unit.reason = reason.into();
        Ok(unit)
    }

    /// 停用前必须结束有效或未来成员、管理及下级关系；业务未结事实由 Service 的 Port 补验。
    fn disable(&mut self, id: &str, actor: &str, reason: &str, at: Instant) -> Result<()> {
        let pending_members = self
            .memberships
            .iter()
            .any(|m| m.org_unit_id == id && m.validity.valid_to.is_none_or(|end| end > at));
        let pending_managers = self
            .management
            .iter()
            .any(|m| m.org_unit_id == id && m.validity.valid_to.is_none_or(|end| end > at));
        let children = self
            .units
            .iter()
            .any(|u| u.enabled && u.parent_id.as_deref() == Some(id));
        if pending_members || pending_managers || children {
            return Err(Error::ConflictError(
                "组织仍有成员、管理关系或有效下级，请先完成交接".into(),
            ));
        }
        self.unit_mut(id, actor, reason)?.enabled = false;
        Ok(())
    }

    /// 结束旧关系并创建新主属关系；时点相同时不产生空有效期。
    fn transfer(
        &mut self,
        user: &str,
        org: &str,
        id: &str,
        actor: &str,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        self.ensure_enabled(org)?;
        if self.own_org(user, at)? == Some(org) {
            return Err(Error::ConflictError("用户已属于该组织".into()));
        }
        self.end_membership(user, actor, reason, at)?;
        self.memberships.push(OrgMembership {
            base: BaseModel::new(id.into()),
            user_id: user.into(),
            org_unit_id: org.into(),
            validity: OrgValidity {
                valid_from: at,
                valid_to: None,
            },
            changed_by: actor.into(),
            reason: reason.into(),
        });
        Ok(())
    }

    /// 结束当前有效关系，保留原始关系记录和此次变更原因。
    fn end_membership(&mut self, user: &str, actor: &str, reason: &str, at: Instant) -> Result<()> {
        for membership in self
            .memberships
            .iter_mut()
            .filter(|m| m.user_id == user && m.validity.contains(at))
        {
            if membership.validity.valid_from >= at {
                return Err(Error::ConflictError("关系刚生效，请稍后再调岗".into()));
            }
            membership.validity.valid_to = Some(at);
            membership.changed_by = actor.into();
            membership.reason = reason.into();
        }
        Ok(())
    }

    /// 撤销管理关系并保留授权留痕。
    fn revoke_management(&mut self, id: &str, at: Instant) -> Result<()> {
        let grant = self
            .management
            .iter_mut()
            .find(|m| m.base.id == id)
            .ok_or_else(|| Error::NotFound("管理关系不存在".into()))?;
        if !grant.validity.contains(at) || grant.validity.valid_from >= at {
            return Err(Error::ConflictError("管理关系未生效、已结束或刚刚生效".into()));
        }
        grant.validity.valid_to = Some(at);
        Ok(())
    }

    /// 组织必须存在且整条祖先路径有效。
    fn ensure_enabled(&self, id: &str) -> Result<()> {
        if OrgTree::new(&self.units)?.expand(id, false)?.is_empty() {
            return Err(Error::ValidationError("组织已停用".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> OrganizationState {
        OrganizationState {
            version: 1,
            units: vec![
                OrgUnit::new(
                    "one".into(),
                    "一部".into(),
                    None,
                    OrgUnitKind::Department,
                    "admin".into(),
                    "初始化".into(),
                )
                .unwrap(),
                OrgUnit::new(
                    "two".into(),
                    "二部".into(),
                    None,
                    OrgUnitKind::Department,
                    "admin".into(),
                    "初始化".into(),
                )
                .unwrap(),
            ],
            memberships: vec![OrgMembership {
                base: BaseModel::new("membership".into()),
                user_id: "sales".into(),
                org_unit_id: "one".into(),
                validity: OrgValidity {
                    valid_from: Instant::from_unix_secs(1),
                    valid_to: None,
                },
                changed_by: "admin".into(),
                reason: "初始化".into(),
            }],
            management: vec![],
        }
    }

    fn request(change: OrganizationOperation) -> OrganizationChangeRequest {
        OrganizationChangeRequest {
            expected_version: 1,
            idempotency_key: "change-1".into(),
            reason: "组织调整".into(),
            change,
        }
    }

    #[test]
    fn transfer_closes_old_relation_and_preserves_the_original_state() {
        let before = state();
        let next = before
            .changed(
                &request(OrganizationOperation::TransferMember {
                    user_id: "sales".into(),
                    org_unit_id: "two".into(),
                }),
                "new-membership",
                "admin",
                Instant::from_unix_secs(10),
            )
            .unwrap();
        assert_eq!(
            before.own_org("sales", Instant::from_unix_secs(10)).unwrap(),
            Some("one")
        );
        assert_eq!(
            next.own_org("sales", Instant::from_unix_secs(9)).unwrap(),
            Some("one")
        );
        assert_eq!(
            next.own_org("sales", Instant::from_unix_secs(10)).unwrap(),
            Some("two")
        );
        assert_eq!(next.memberships.len(), 2);
        assert_eq!(next.version, 2);
    }

    #[test]
    fn stale_version_cycle_and_disable_with_members_are_rejected_without_partial_changes() {
        let before = state();
        let mut change = request(OrganizationOperation::DisableUnit {
            org_unit_id: "one".into(),
        });
        assert!(before
            .changed(&change, "id", "admin", Instant::from_unix_secs(10))
            .is_err());
        change.expected_version = 0;
        assert!(matches!(
            before.changed(&change, "id", "admin", Instant::from_unix_secs(10)),
            Err(Error::ConflictError(_))
        ));
        let cycle = request(OrganizationOperation::MoveUnit {
            org_unit_id: "one".into(),
            parent_id: Some("one".into()),
        });
        assert!(before
            .changed(&cycle, "id", "admin", Instant::from_unix_secs(10))
            .is_err());
        assert_eq!(before.version, 1);
        assert!(before.units[0].enabled);
        assert_eq!(before.units[0].parent_id, None);
    }

    #[test]
    fn overlapping_memberships_and_invalid_idempotency_are_rejected() {
        let mut before = state();
        let mut duplicate = before.memberships[0].clone();
        duplicate.base.id = "duplicate".into();
        duplicate.org_unit_id = "two".into();
        before.memberships.push(duplicate);
        assert!(before.own_org("sales", Instant::from_unix_secs(10)).is_err());
        let mut invalid = request(OrganizationOperation::EndMembership {
            user_id: "sales".into(),
        });
        invalid.idempotency_key = " ".into();
        assert!(invalid.validate().is_err());
    }
}
