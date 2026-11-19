//! 组织变更的确定性状态迁移；写入、审计与版本推进由 Service 事务提交。

use std::collections::BTreeSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use super::organization::*;
use super::person_profile_change::PersonProfileChange;
use crate::{Error, Result};

/// 同一事务中的组织事实快照，供变更计算和审计对照。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationState {
    /// 快照对应的全局组织版本，用于拒绝过期写入。
    pub version: u64,
    /// 组织拓扑中的节点，含停用节点。
    pub units: Vec<OrgUnit>,
    /// 主属关系记录，用有效期区分历史、当前和未来归属。
    pub memberships: Vec<OrgMembership>,
    /// 仅供旧审计读取的管理关系，当前授权和变更校验均不使用；空集合时省略。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub management: Vec<OrgManagementAssignment>,
}

/// 版本化的组织管理命令；新实体 ID 由服务端生成，更新须匹配当前组织版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum OrganizationOperation {
    /// 在同一次变更中更新单人的姓名、角色或主属组织。
    UpdatePersonProfile {
        /// 人员资料差异及用于并发校验的原始值。
        profile: PersonProfileChange,
    },
    /// 创建启用的新组织节点。
    CreateUnit {
        /// 新组织名称，创建时规范化并校验。
        name: String,
        /// 启用的直接上级 ID，`None` 表示创建根节点。
        parent_id: Option<String>,
        /// 新节点的部门或团队类别。
        kind: OrgUnitKind,
    },
    /// 设置、替换或清除组织的当前负责人。
    SetLeader {
        /// 要修改负责人指派的组织 ID。
        org_unit_id: String,
        /// 新负责人账号 ID，`None` 表示明确清除指派。
        user_id: Option<String>,
    },
    /// 调整节点的直接上级，变更后校验整棵树。
    MoveUnit {
        /// 要迁移的组织节点 ID。
        org_unit_id: String,
        /// 新的直接上级 ID，`None` 表示移为根节点。
        parent_id: Option<String>,
    },
    /// 修改组织的展示名称。
    RenameUnit {
        /// 要改名的组织 ID。
        org_unit_id: String,
        /// 待规范化的新名称。
        name: String,
    },
    /// 停用组织，保留节点和历史关系。
    DisableUnit {
        /// 拟停用的组织 ID，须先处理成员和启用下级。
        org_unit_id: String,
    },
    /// 自本次变更时点设置单个账号的主属组织。
    TransferMember {
        /// 要调整主属关系的账号 ID。
        user_id: String,
        /// 调岗后的主属组织 ID。
        org_unit_id: String,
    },
    /// 在同一事务内为多人调整主属组织，任一人失败即拒绝整批变更。
    TransferMembers {
        /// 待调岗的账号 ID 列表，至少一项，各项须非空且唯一。
        user_ids: Vec<String>,
        /// 本批人员共同调入的主属组织 ID。
        org_unit_id: String,
    },
    /// 自本次变更时点结束当前主属关系。
    EndMembership {
        /// 要结束主属关系的账号 ID。
        user_id: String,
    },
    /// 旧管理关系授予命令，仅供审计读取，请求校验拒绝执行。
    GrantManagement {
        /// 当时获授管理范围的账号 ID。
        user_id: String,
        /// 当时绑定的授予角色 ID。
        role_id: String,
        /// 原管理范围的起始组织 ID。
        org_unit_id: String,
        /// 原授权是否覆盖起始组织的下级。
        include_descendants: bool,
        /// 原授予请求的失效时点，`None` 表示无固定终点。
        valid_to: Option<Instant>,
    },
    /// 旧管理关系撤销命令，仅供审计读取，请求校验拒绝执行。
    RevokeManagement {
        /// 当时要撤销的管理关系 ID。
        assignment_id: String,
    },
}

/// 提交或预览组织变更的请求，携带并发校验和幂等回放所需信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OrganizationChangeRequest {
    /// 发起变更时看到的全局组织版本，过期时须刷新。
    pub expected_version: u64,
    /// 在同一操作人下标识本次请求，重试时沿用。
    pub idempotency_key: String,
    /// 组织调整的审计原因，应用命令时去除首尾空白。
    pub reason: String,
    /// 本次请求携带的组织管理命令。
    pub change: OrganizationOperation,
}

/// 组织版本单例，串行化拓扑和有效期写入，防止并发写偏差。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrganizationRevision {
    /// 全局版本单例的固定记录 ID 与持久化元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 事务提交后可读的全局组织版本。
    pub revision: u64,
}

/// 组织变更的幂等回执与前后状态审计，记录业务事实和操作人 ID，不保存账号凭证。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrganizationChangeReceipt {
    /// 回执记录的 ID 与持久化元数据；普通请求的 ID 由操作人和幂等键组成。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 发起变更的操作人 ID。
    pub actor_id: String,
    /// 原始变更请求，用于检验重试载荷是否一致。
    pub request: OrganizationChangeRequest,
    /// 变更命令应用前的组织事实及版本。
    pub before: OrganizationState,
    /// 变更命令应用后的组织事实及版本。
    pub after: OrganizationState,
    /// 计算变更时采用的服务端时点，也是本次关系变动的生效边界。
    pub as_of: Instant,
}

impl OrganizationChangeRequest {
    /// 在计算新状态前校验命令、幂等键和原因。
    /// 拒绝已停用的授予与撤销管理命令；传播人员资料展开错误；批量调岗须有人员且账号 ID 非空、唯一。
    /// 幂等键须为 1 至 128 字节的 ASCII 字母、数字、下划线或短横线；原因去首尾空白后须非空，原值最多 1000 字符。
    pub fn validate(&self) -> Result<()> {
        match &self.change {
            OrganizationOperation::GrantManagement { .. }
            | OrganizationOperation::RevokeManagement { .. } => {
                return Err(Error::ValidationError("部门管理关系已停用，请在人员数据范围中设置授权".into()));
            },
            OrganizationOperation::UpdatePersonProfile { profile } => {
                profile.operations()?;
            },
            OrganizationOperation::TransferMembers { user_ids, .. } => {
                if user_ids.is_empty() {
                    return Err(Error::ValidationError("请选择人员".into()));
                }
                if user_ids.iter().any(String::is_empty)
                    || user_ids.iter().collect::<BTreeSet<_>>().len() != user_ids.len()
                {
                    return Err(Error::ValidationError("人员重复或为空".into()));
                }
            },
            _ => {},
        }

        if self.idempotency_key.is_empty()
            || self.idempotency_key.len() > 128
            || !self.idempotency_key.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(Error::ValidationError("幂等键必须为不超过128位的字母、数字、下划线或短横线".into()));
        }
        if self.reason.trim().is_empty() || self.reason.chars().count() > 1000 {
            return Err(Error::ValidationError("必须填写不超过1000字的变更原因".into()));
        }
        Ok(())
    }
}

impl OrganizationState {
    /// 校验请求与期望版本后，在快照副本内按 `at` 应用命令，保留原状态。
    /// `id` 用于本次新建的组织、主属关系或负责人指派；子操作使用派生 ID。
    /// 命令、组织树及成员有效期（含重叠）均校验成功后将版本加一；版本溢出报错。
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

        next.version = self.version.checked_add(1).ok_or_else(|| Error::Internal("组织版本溢出".into()))?;
        Ok(next)
    }

    /// 按快照查询账号在 `at` 的唯一有效主属组织 ID；无归属时返回 `None`，多条有效关系返回 `Error::ConflictError`。
    pub fn own_org(&self, user_id: &str, at: Instant) -> Result<Option<&str>> {
        OrgMembership::primary_org(&self.memberships, user_id, at)
    }

    // 校验所有未软删除关系的区间，再按账号和起点检查相邻区间，提前拒绝未来归属重叠。
    fn validate_memberships(&self) -> Result<()> {
        let mut members = self.memberships.iter().filter(|m| !m.base.is_deleted()).collect::<Vec<_>>();
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

    // 将命令应用到组织快照；资料更新只展开其中的调岗，业务单据负责人和任务保持原归属。
    fn apply(
        &mut self,
        change: &OrganizationOperation,
        id: &str,
        actor: &str,
        reason: &str,
        at: Instant,
    ) -> Result<()> {
        match change {
            OrganizationOperation::UpdatePersonProfile { profile } => {
                for (index, operation) in profile.operations()?.iter().enumerate() {
                    self.apply(operation, &format!("{id}-{index}"), actor, reason, at)?;
                }
            },
            OrganizationOperation::CreateUnit { name, parent_id, kind } => {
                self.create_unit(name, parent_id.as_deref(), *kind, id, actor, reason)?;
            },
            OrganizationOperation::MoveUnit { org_unit_id, parent_id } => {
                self.move_unit(org_unit_id, parent_id.as_deref(), actor, reason)?;
            },
            OrganizationOperation::RenameUnit { org_unit_id, name } => {
                self.rename_unit(org_unit_id, name, actor, reason)?;
            },
            OrganizationOperation::SetLeader { org_unit_id, user_id } => {
                if user_id.as_ref().is_some_and(|id| id.trim().is_empty()) {
                    return Err(Error::ValidationError("请选择负责人，或明确清除负责人".into()));
                }
                self.unit_mut(org_unit_id, actor, reason)?.leader = user_id.as_ref().map(|user| OrgLeader {
                    assignment_id: id.into(),
                    user_id: user.clone(),
                    assigned_at: at,
                    assigned_by: actor.into(),
                });
            },
            OrganizationOperation::DisableUnit { org_unit_id } => {
                self.disable(org_unit_id, actor, reason, at)?
            },
            OrganizationOperation::TransferMember { user_id, org_unit_id } => {
                self.transfer(user_id, org_unit_id, id, actor, reason, at)?
            },
            OrganizationOperation::TransferMembers { user_ids, org_unit_id } => {
                for (index, user_id) in user_ids.iter().enumerate() {
                    self.transfer(user_id, org_unit_id, &format!("{id}-{index}"), actor, reason, at)?;
                }
            },
            OrganizationOperation::EndMembership { user_id } => {
                self.end_membership(user_id, actor, reason, at)?
            },
            OrganizationOperation::GrantManagement { .. }
            | OrganizationOperation::RevokeManagement { .. } => {
                return Err(Error::ValidationError("部门管理关系已停用，请在人员数据范围中设置授权".into()));
            },
        }
        Ok(())
    }

    // 创建启用节点；指定上级时先确认该上级存在且全祖先链启用。
    fn create_unit(
        &mut self,
        name: &str,
        parent_id: Option<&str>,
        kind: OrgUnitKind,
        id: &str,
        actor: &str,
        reason: &str,
    ) -> Result<()> {
        if let Some(parent) = parent_id {
            self.ensure_enabled(parent)?;
        }
        self.units.push(OrgUnit::new(
            id.into(),
            name.into(),
            parent_id.map(str::to_owned),
            kind,
            actor.into(),
            reason.into(),
        )?);
        Ok(())
    }

    // 在目标及新父级可用时调整父链，成环检查由变更后的整树校验负责。
    fn move_unit(&mut self, id: &str, parent_id: Option<&str>, actor: &str, reason: &str) -> Result<()> {
        if let Some(parent) = parent_id {
            self.ensure_enabled(parent)?;
        }
        self.unit_mut(id, actor, reason)?.parent_id = parent_id.map(str::to_owned);
        Ok(())
    }

    // 先校验并规范化新名称，再更新可用节点的名称及审计信息。
    fn rename_unit(&mut self, id: &str, name: &str, actor: &str, reason: &str) -> Result<()> {
        let name = normalize_required_text(name.into(), "组织名称不能为空", 100, "组织名称过长")?;
        self.unit_mut(id, actor, reason)?.name = name;
        Ok(())
    }

    // 确认节点及其祖先链启用后，记录操作人和原因并返回节点的可变引用。
    fn unit_mut(&mut self, id: &str, actor: &str, reason: &str) -> Result<&mut OrgUnit> {
        self.ensure_enabled(id)?;
        let unit = self
            .units
            .iter_mut()
            .find(|u| u.base.id == id)
            .expect("ensure_enabled 已确认节点存在，且组织集合尚未修改");

        unit.changed_by = actor.into();
        unit.reason = reason.into();
        Ok(unit)
    }

    // 有尚未结束的主属记录或启用的直接下级时拒绝停用；未结业务由服务经业务端口另行校验。
    fn disable(&mut self, id: &str, actor: &str, reason: &str, at: Instant) -> Result<()> {
        let pending_members = self
            .memberships
            .iter()
            .any(|m| m.org_unit_id == id && m.validity.valid_to.is_none_or(|end| end > at));
        let children = self.units.iter().any(|u| u.enabled && u.parent_id.as_deref() == Some(id));
        if pending_members || children {
            return Err(Error::ConflictError("组织仍有成员或有效下级，请先完成交接".into()));
        }

        self.unit_mut(id, actor, reason)?.enabled = false;
        Ok(())
    }

    // 校验目标组织并拒绝调入原组织，再于同一时点结束旧主属、建立无终点的新主属。
    // 旧关系恰在本次时点生效时，由结束逻辑拒绝空区间。
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
            validity: OrgValidity { valid_from: at, valid_to: None },
            changed_by: actor.into(),
            reason: reason.into(),
        });
        Ok(())
    }

    // 将覆盖变更时点的主属关系截断至该时点并更新审计信息；起点等于结束时点时拒绝。
    fn end_membership(&mut self, user: &str, actor: &str, reason: &str, at: Instant) -> Result<()> {
        for membership in self.memberships.iter_mut().filter(|m| m.user_id == user && m.validity.contains(at))
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

    // 借助已校验的树确认目标存在，且目标至根的路径全部启用。
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

    // 验证负责人替换保留原快照、更新生效时点、隔离其他关系并拒绝旧版本重写。
    #[test]
    fn leader_changes_keep_history_and_do_not_change_membership_or_permissions() {
        let before = state();
        let first = before
            .changed(
                &request(OrganizationOperation::SetLeader {
                    org_unit_id: "one".into(),
                    user_id: Some("manager".into()),
                }),
                "assignment1",
                "admin",
                Instant::from_unix_secs(10),
            )
            .unwrap();
        assert!(before.units[0].leader.is_none());
        assert_eq!(first.units[0].leader.as_ref().unwrap().user_id, "manager");
        assert_eq!(first.memberships, before.memberships);
        assert_eq!(first.management, before.management);

        let mut next = request(OrganizationOperation::SetLeader {
            org_unit_id: "one".into(),
            user_id: Some("next".into()),
        });
        next.expected_version = first.version;
        let after = first.changed(&next, "assignment2", "admin", Instant::from_unix_secs(20)).unwrap();
        assert_eq!(first.units[0].leader.as_ref().unwrap().user_id, "manager");
        assert_eq!(after.units[0].leader.as_ref().unwrap().assigned_at, Instant::from_unix_secs(20));
        assert!(after.changed(&next, "assignment3", "admin", Instant::from_unix_secs(30)).is_err());
    }

    // 构造版本为 1、含两个根部门及一条销售人员主属关系的初始快照。
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
                validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
                changed_by: "admin".into(),
                reason: "初始化".into(),
            }],
            management: vec![],
        }
    }

    // 为待测命令补齐匹配初始快照的版本、固定幂等键和审计原因。
    fn request(change: OrganizationOperation) -> OrganizationChangeRequest {
        OrganizationChangeRequest {
            expected_version: 1,
            idempotency_key: "change-1".into(),
            reason: "组织调整".into(),
            change,
        }
    }

    // 验证创建、迁移及改名会更新拓扑、规范化名称和审计信息，并保留原快照。
    #[test]
    fn unit_commands_update_identity_topology_and_audit() {
        let before = state();
        let at = Instant::from_unix_secs(10);
        let created = before
            .changed(
                &request(OrganizationOperation::CreateUnit {
                    name: " 新团队 ".into(),
                    parent_id: Some("one".into()),
                    kind: OrgUnitKind::Team,
                }),
                "new",
                "operator",
                at,
            )
            .unwrap();
        let unit = created.units.iter().find(|unit| unit.base.id == "new").unwrap();
        assert_eq!(unit.name, "新团队");
        assert_eq!(unit.parent_id.as_deref(), Some("one"));
        assert_eq!(unit.kind, OrgUnitKind::Team);
        assert_eq!(unit.changed_by, "operator");
        assert_eq!(unit.reason, "组织调整");

        let moved = before
            .changed(
                &request(OrganizationOperation::MoveUnit {
                    org_unit_id: "two".into(),
                    parent_id: Some("one".into()),
                }),
                "id",
                "operator",
                at,
            )
            .unwrap();
        assert_eq!(moved.units[1].parent_id.as_deref(), Some("one"));
        assert_eq!(moved.units[1].changed_by, "operator");

        let renamed = before
            .changed(
                &request(OrganizationOperation::RenameUnit {
                    org_unit_id: "two".into(),
                    name: " 新名称 ".into(),
                }),
                "id",
                "operator",
                at,
            )
            .unwrap();
        assert_eq!(renamed.units[1].name, "新名称");
        assert_eq!(renamed.units[1].changed_by, "operator");

        assert_eq!(before.units.len(), 2);
        assert_eq!(before.units[1].name, "二部");
        assert_eq!(before.units[1].parent_id, None);
    }

    // 验证未知节点、停用祖先和非法名称均以错误拒绝，并保留原快照。
    #[test]
    fn unit_commands_reject_missing_nodes_disabled_ancestors_and_invalid_names() {
        let mut before = state();
        before.units[0].enabled = false;
        before.units[1].parent_id = Some("one".into());
        let original = before.clone();
        for operation in [
            OrganizationOperation::RenameUnit { org_unit_id: "missing".into(), name: "名称".into() },
            OrganizationOperation::RenameUnit { org_unit_id: "two".into(), name: "名称".into() },
            OrganizationOperation::RenameUnit { org_unit_id: "two".into(), name: " ".into() },
            OrganizationOperation::MoveUnit { org_unit_id: "two".into(), parent_id: None },
            OrganizationOperation::CreateUnit {
                name: "新团队".into(),
                parent_id: Some("two".into()),
                kind: OrgUnitKind::Team,
            },
        ] {
            assert!(
                before.changed(&request(operation), "id", "operator", Instant::from_unix_secs(10)).is_err()
            );
            assert_eq!(before, original);
        }
    }

    // 验证人员资料更新只生成一次调岗、版本只增一次，目标组织无效时保留原状态。
    #[test]
    fn profile_changes_membership_once_or_fails_whole_plan() {
        let before = state();
        let profile = PersonProfileChange {
            user_id: "sales".into(),
            expected_name: "姓名".into(),
            expected_role_ids: None,
            role_ids: None,
            name: Some("新姓名".into()),
            org_unit_id: Some("two".into()),
            remove_management_ids: vec![],
            add_management: vec![],
        };
        let change = request(OrganizationOperation::UpdatePersonProfile { profile: profile.clone() });
        let after = before.changed(&change, "id", "admin", Instant::from_unix_secs(10)).unwrap();
        assert_eq!(after.version, before.version + 1);
        assert_eq!(after.own_org("sales", Instant::from_unix_secs(10)).unwrap(), Some("two"));
        assert!(after.management.is_empty());

        let mut invalid = profile;
        invalid.org_unit_id = Some("missing".into());
        assert!(
            before
                .changed(
                    &request(OrganizationOperation::UpdatePersonProfile { profile: invalid }),
                    "id",
                    "admin",
                    Instant::from_unix_secs(10)
                )
                .is_err()
        );
        assert_eq!(before.own_org("sales", Instant::from_unix_secs(10)).unwrap(), Some("one"));
        assert!(before.management.is_empty());
    }

    // 验证调岗边界前后分别解析到旧、新组织，历史记录和原快照均保留。
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
        assert_eq!(before.own_org("sales", Instant::from_unix_secs(10)).unwrap(), Some("one"));
        assert_eq!(next.own_org("sales", Instant::from_unix_secs(9)).unwrap(), Some("one"));
        assert_eq!(next.own_org("sales", Instant::from_unix_secs(10)).unwrap(), Some("two"));
        assert_eq!(next.memberships.len(), 2);
        assert_eq!(next.version, 2);
    }

    // 验证整批人员同次调岗且仅提升一次版本；单人失败、重复名单或空名单拒绝整批。
    #[test]
    fn batch_transfer_moves_all_users_or_changes_nothing() {
        let mut before = state();
        before.memberships.push(OrgMembership {
            base: BaseModel::new("buyer-membership".into()),
            user_id: "buyer".into(),
            org_unit_id: "one".into(),
            validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
            changed_by: "admin".into(),
            reason: "初始化".into(),
        });
        let change = request(OrganizationOperation::TransferMembers {
            user_ids: vec!["sales".into(), "buyer".into()],
            org_unit_id: "two".into(),
        });
        let next = before.changed(&change, "batch", "admin", Instant::from_unix_secs(10)).unwrap();
        assert_eq!(next.version, 2);
        assert_eq!(next.own_org("sales", Instant::from_unix_secs(10)).unwrap(), Some("two"));
        assert_eq!(next.own_org("buyer", Instant::from_unix_secs(10)).unwrap(), Some("two"));
        assert_eq!(next.memberships.len(), 4);
        assert_eq!(before.memberships.len(), 2);

        let mut already = before.clone();
        already.memberships[1].org_unit_id = "two".into();
        assert!(already.changed(&change, "batch", "admin", Instant::from_unix_secs(10)).is_err());
        assert_eq!(already.version, 1);
        assert_eq!(already.memberships.len(), 2);

        let duplicate = OrganizationOperation::TransferMembers {
            user_ids: vec!["sales".into(), "sales".into()],
            org_unit_id: "two".into(),
        };
        assert!(request(duplicate).validate().is_err());
        let empty = OrganizationOperation::TransferMembers { user_ids: vec![], org_unit_id: "two".into() };
        assert!(request(empty).validate().is_err());
    }

    // 验证旧版本、自成环和带成员停用均被拒绝，原版本与拓扑保持不变。
    #[test]
    fn stale_version_cycle_and_disable_with_members_are_rejected_without_partial_changes() {
        let before = state();
        let mut change = request(OrganizationOperation::DisableUnit { org_unit_id: "one".into() });
        assert!(before.changed(&change, "id", "admin", Instant::from_unix_secs(10)).is_err());

        change.expected_version = 0;
        assert!(matches!(
            before.changed(&change, "id", "admin", Instant::from_unix_secs(10)),
            Err(Error::ConflictError(_))
        ));

        let cycle = request(OrganizationOperation::MoveUnit {
            org_unit_id: "one".into(),
            parent_id: Some("one".into()),
        });
        assert!(before.changed(&cycle, "id", "admin", Instant::from_unix_secs(10)).is_err());

        assert_eq!(before.version, 1);
        assert!(before.units[0].enabled);
        assert_eq!(before.units[0].parent_id, None);
    }

    // 验证旧管理命令仍可序列化往返，重读后仍被请求校验和状态变更入口拒绝。
    #[test]
    fn legacy_management_commands_deserialize_but_cannot_change_state() {
        let before = state();
        for operation in [
            OrganizationOperation::GrantManagement {
                user_id: "sales".into(),
                role_id: "role".into(),
                org_unit_id: "one".into(),
                include_descendants: false,
                valid_to: None,
            },
            OrganizationOperation::RevokeManagement { assignment_id: "old".into() },
        ] {
            let request = request(operation);
            let encoded = serde_json::to_string(&request).unwrap();
            let decoded = serde_json::from_str::<OrganizationChangeRequest>(&encoded).unwrap();
            assert_eq!(decoded, request);
            assert!(matches!(decoded.validate(), Err(Error::ValidationError(_))));
            assert!(before.changed(&decoded, "id", "admin", Instant::from_unix_secs(10)).is_err());
        }
        assert_eq!(before.version, 1);
        assert!(before.management.is_empty());
    }

    // 验证含旧管理关系的快照可重读，部门仍可停用并原样保留历史关系。
    #[test]
    fn historical_management_does_not_block_department_disable() {
        let mut before = state();
        before.management.push(OrgManagementAssignment {
            base: BaseModel::new("legacy".into()),
            user_id: "sales".into(),
            role_id: "old-role".into(),
            org_unit_id: "two".into(),
            include_descendants: true,
            validity: OrgValidity { valid_from: Instant::from_unix_secs(1), valid_to: None },
            granted_by: "admin".into(),
            reason: "历史".into(),
        });

        let snapshot = serde_json::to_value(&before).unwrap();
        assert_eq!(snapshot["management"].as_array().unwrap().len(), 1);
        let before: OrganizationState = serde_json::from_value(snapshot).unwrap();

        let after = before
            .changed(
                &request(OrganizationOperation::DisableUnit { org_unit_id: "two".into() }),
                "id",
                "admin",
                Instant::from_unix_secs(10),
            )
            .unwrap();
        assert!(!after.units[1].enabled);
        assert_eq!(after.management, before.management);
    }

    // 验证空管理历史在新快照中省略，并兼容缺失字段和旧空数组。
    #[test]
    fn current_snapshots_omit_retired_management_and_accept_empty_history() {
        let before = state();
        let mut snapshot = serde_json::to_value(&before).unwrap();
        assert!(snapshot.get("management").is_none());
        assert_eq!(serde_json::from_value::<OrganizationState>(snapshot.clone()).unwrap(), before);
        snapshot["management"] = serde_json::json!([]);
        assert_eq!(serde_json::from_value::<OrganizationState>(snapshot).unwrap(), before);
    }

    // 验证主属查询拒绝重叠关系，变更请求校验拒绝空白幂等键。
    #[test]
    fn overlapping_memberships_and_invalid_idempotency_are_rejected() {
        let mut before = state();
        let mut duplicate = before.memberships[0].clone();
        duplicate.base.id = "duplicate".into();
        duplicate.org_unit_id = "two".into();
        before.memberships.push(duplicate);
        assert!(before.own_org("sales", Instant::from_unix_secs(10)).is_err());

        let mut invalid = request(OrganizationOperation::EndMembership { user_id: "sales".into() });
        invalid.idempotency_key = " ".into();
        assert!(invalid.validate().is_err());
    }
}
