//! 内部组织、主属成员和按角色授予的管理关系；不承载法人或仓库身份。

use std::collections::{BTreeMap, BTreeSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 内部组织分类，用于按部门或团队解析审批负责人。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrgUnitKind {
    /// 部门节点，可作为部门负责人解析的目标。
    Department,
    /// 团队节点，可作为团队负责人解析的目标。
    Team,
}

/// 内部组织树节点，通过停用保留历史归属与引用。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgUnit {
    /// 组织稳定 ID 与持久化元数据，平铺到节点记录中。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 组织展示名称，创建和改名时去除首尾空白。
    pub name: String,
    /// 直接上级的组织 ID，根节点为 `None`。
    pub parent_id: Option<String>,
    /// 节点类别，供审批按部门或团队选择负责人。
    pub kind: OrgUnitKind,
    /// 当前负责人指派；未配置、已清除或旧记录缺省时为 `None`，历史指派留在变更回执中。
    #[serde(default)]
    pub leader: Option<OrgLeader>,
    /// 节点是否启用；范围解析还要求整条祖先链启用。
    pub enabled: bool,
    /// 最近一次修改节点的操作人 ID。
    pub changed_by: String,
    /// 最近一次节点变更的审计原因。
    pub reason: String,
}

/// 单个组织的当前唯一负责人指派，自本次变更时点生效，随提交写入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgLeader {
    /// 服务端为本次负责人指派生成的 ID。
    pub assignment_id: String,
    /// 被指派为负责人的账号 ID。
    pub user_id: String,
    /// 负责人指派的生效时点。
    pub assigned_at: Instant,
    /// 设置负责人的操作人 ID。
    pub assigned_by: String,
}

/// 组织关系采用 UTC 半开有效期；客户归属沿用独立的自然日规则。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgValidity {
    /// 关系开始生效的时点，计入有效期。
    pub valid_from: Instant,
    /// 关系开始失效的时点，`None` 表示无固定终点。
    pub valid_to: Option<Instant>,
}

impl OrgValidity {
    /// 校验组织关系的有效期；允许无终点，终点小于或等于起点时返回 `Error::ValidationError`。
    pub fn validate(&self) -> Result<()> {
        if self.valid_to.is_some_and(|end| end <= self.valid_from) {
            return Err(Error::ValidationError("有效期结束必须晚于开始".into()));
        }
        Ok(())
    }

    /// 判断组织关系在服务端时点 `at` 是否有效，包含起点、排除终点；无终点时持续有效。
    pub fn contains(&self, at: Instant) -> bool {
        self.valid_from <= at && self.valid_to.is_none_or(|end| at < end)
    }

    /// 判断两个半开有效期是否相交；首尾相接不算重叠，无终点视为持续有效。
    pub fn overlaps(&self, other: &Self) -> bool {
        self.valid_to.is_none_or(|end| other.valid_from < end)
            && other.valid_to.is_none_or(|end| self.valid_from < end)
    }
}

/// 账号在指定时段的主属组织关系；同一账号同一时点至多一条有效关系。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgMembership {
    /// 主属关系的独立 ID 与软删除等持久化元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 此关系归属的账号 ID。
    pub user_id: String,
    /// 该时段所属的组织节点 ID。
    pub org_unit_id: String,
    /// 平铺存储的主属关系生效区间，调岗时用于衔接新旧关系。
    #[serde(flatten)]
    pub validity: OrgValidity,
    /// 建立或结束此关系的最近一次操作人 ID。
    pub changed_by: String,
    /// 建立或结束此关系的最近一次审计原因。
    pub reason: String,
}

impl OrgMembership {
    /// 从当前及未来的主属事实中，取指定账号严格晚于 `at` 的最早起止时点，供限定授权期限。
    /// 跳过软删除关系，无后续边界时返回 `None`；当前关系的重叠校验由 `primary_org` 负责。
    pub fn next_boundary(memberships: &[Self], user_id: &str, at: Instant) -> Option<Instant> {
        memberships
            .iter()
            .filter(|membership| !membership.base.is_deleted() && membership.user_id == user_id)
            .flat_map(|membership| [Some(membership.validity.valid_from), membership.validity.valid_to])
            .flatten()
            .filter(|boundary| *boundary > at)
            .min()
    }

    /// 从完整成员事实或按账号及时点限定的事实中，解析 `at` 时唯一有效的主属组织 ID。
    /// 跳过软删除和有效期外的关系；没有有效关系时返回 `None`，多条有效关系返回冲突。
    pub fn primary_org<'a>(memberships: &'a [Self], user_id: &str, at: Instant) -> Result<Option<&'a str>> {
        let mut current = memberships.iter().filter(|membership| {
            membership.user_id == user_id && !membership.base.is_deleted() && membership.validity.contains(at)
        });
        let first = current.next();
        if current.next().is_some() {
            return Err(Error::ConflictError("用户存在重叠主属组织关系".into()));
        }
        Ok(first.map(|membership| membership.org_unit_id.as_str()))
    }
}

/// 旧部门管理关系的审计记录；当时由账号所持且具备完整动作权限的指定角色启用，现已停用。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgManagementAssignment {
    /// 历史管理关系的 ID 与持久化元数据。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 历史管理范围授予的账号 ID。
    pub user_id: String,
    /// 当时绑定的授予角色 ID。
    pub role_id: String,
    /// 历史管理范围的起始组织 ID。
    pub org_unit_id: String,
    /// 当时是否将起始组织的下级纳入管理范围。
    pub include_descendants: bool,
    /// 平铺存储的历史管理关系有效期。
    #[serde(flatten)]
    pub validity: OrgValidity,
    /// 当时授予管理关系的操作人 ID。
    pub granted_by: String,
    /// 当时授予管理关系的审计原因。
    pub reason: String,
}

/// 借用已校验的组织节点，提供祖先路径和下级范围解析。
pub struct OrgTree<'a> {
    // 按组织 ID 定位节点，供逐级查找上级。
    nodes: BTreeMap<&'a str, &'a OrgUnit>,
    // 按父组织 ID 索引直接子节点，供按分支展开范围。
    children: BTreeMap<&'a str, Vec<&'a OrgUnit>>,
}

impl<'a> OrgTree<'a> {
    /// 用完整节点集合建立组织和直接下级索引。
    /// 先拒绝重复 ID，再按输入顺序验证父链；悬空父级、自引用和环均报错。
    pub fn new(nodes: &'a [OrgUnit]) -> Result<Self> {
        let mut tree = Self {
            nodes: nodes.iter().map(|node| (node.base.id.as_str(), node)).collect(),
            children: BTreeMap::new(),
        };
        if tree.nodes.len() != nodes.len() {
            return Err(Error::ValidationError("组织身份重复".into()));
        }

        for node in nodes {
            tree.validate_path(&node.base.id)?;
            if let Some(parent) = node.parent_id.as_deref() {
                tree.children.entry(parent).or_default().push(node);
            }
        }
        Ok(tree)
    }

    /// 沿构造时校验过的父链返回从根到目标的路径，含目标节点。
    /// 目标 ID 不存在时返回 `Error::ValidationError`。
    pub fn path(&self, id: &str) -> Result<Vec<&'a OrgUnit>> {
        let mut path = Vec::new();
        let mut node = self.nodes.get(id).ok_or_else(|| Error::ValidationError("组织节点不存在".into()))?;
        loop {
            path.push(*node);
            let Some(parent) = node.parent_id.as_deref() else {
                break;
            };
            node = self.nodes.get(parent).expect("构造时已验证完整祖先链，树不可变");
        }

        path.reverse();
        Ok(path)
    }

    // 建树时逐级追溯到根，遇到缺失父级或重复经过同一节点即报错。
    fn validate_path(&self, id: &str) -> Result<()> {
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(Error::ValidationError("组织父子关系不得形成环".into()));
            }
            let node = self.nodes.get(id).ok_or_else(|| Error::ValidationError("组织节点不存在".into()))?;
            current = node.parent_id.as_deref();
        }
        Ok(())
    }

    /// 按 ID 有序返回目标组织；`descendants` 为真时沿启用分支递归纳入下级。
    /// 未知目标报错；目标或任一祖先停用时返回空集合，展开时跳过停用分支。
    pub fn expand(&self, id: &str, descendants: bool) -> Result<BTreeSet<String>> {
        let target_path = self.path(id)?;
        if target_path.iter().any(|node| !node.enabled) {
            return Ok(BTreeSet::new());
        }

        let mut result = BTreeSet::from([id.to_string()]);
        if !descendants {
            return Ok(result);
        }

        let mut pending = vec![id];
        while let Some(parent) = pending.pop() {
            if let Some(children) = self.children.get(parent) {
                for child in children.iter().filter(|child| child.enabled) {
                    result.insert(child.base.id.clone());
                    pending.push(child.base.id.as_str());
                }
            }
        }
        Ok(result)
    }
}

impl OrgUnit {
    /// 在从申请人主属组织到根的完整路径中，选取最近的 `kind` 节点供审批解析负责人。
    /// 路径中有停用或软删除节点、没有匹配类型，或选中节点在 `at` 无已生效指派时拒绝；只检查最近匹配节点的指派。
    pub fn leader_on_path(path: Vec<Self>, kind: OrgUnitKind, at: Instant) -> Result<Self> {
        if path.iter().any(|unit| !unit.enabled || unit.base.is_deleted()) {
            return Err(Error::ValidationError("申请人所属组织或上级组织已停用，请调整所属组织".into()));
        }

        let label = match kind {
            OrgUnitKind::Team => "团队",
            OrgUnitKind::Department => "部门",
        };
        let unit = path.into_iter().find(|unit| unit.kind == kind).ok_or_else(|| {
            Error::ValidationError(format!("申请人没有所属{label}，请检查组织配置或审批流程"))
        })?;

        if unit.leader.as_ref().is_none_or(|leader| leader.assigned_at > at) {
            return Err(Error::ValidationError(format!(
                "{}未配置有效负责人，请在组织与人员中设置负责人",
                unit.name
            )));
        }
        Ok(unit)
    }

    /// 创建默认启用且未指派负责人的组织节点，`parent_id` 为 `None` 时表示根节点；父链由变更事务校验。
    /// 名称、操作人和原因去除首尾空白后必填，分别限制为 100、128 和 1000 个字符。
    pub fn new(
        id: String,
        name: String,
        parent_id: Option<String>,
        kind: OrgUnitKind,
        actor: String,
        reason: String,
    ) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id),
            name: normalize_required_text(name, "组织名称不能为空", 100, "组织名称过长")?,
            parent_id,
            kind,
            leader: None,
            enabled: true,
            changed_by: normalize_required_text(actor, "操作人不能为空", 128, "操作人身份过长")?,
            reason: normalize_required_text(reason, "变更原因不能为空", 1000, "变更原因过长")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::iter::once;
    use std::time::Instant as Stopwatch;

    use super::*;

    // 用 Unix 秒数构造指定账号和组织的主属关系，便于测试起止边界。
    fn membership(user: &str, org: &str, from: i64, to: Option<i64>) -> OrgMembership {
        OrgMembership {
            base: BaseModel::fake(),
            user_id: user.into(),
            org_unit_id: org.into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(from),
                valid_to: to.map(Instant::from_unix_secs),
            },
            changed_by: "admin".into(),
            reason: "测试主属".into(),
        }
    }

    // 验证主属解析按账号和半开区间取值，忽略软删除并拒绝有效关系重叠。
    #[test]
    fn primary_org_preserves_validity_deletion_and_overlap_rules() {
        let mut rows = vec![
            membership("user", "old", 10, Some(20)),
            membership("other", "ignored", 0, None),
            membership("user", "new", 20, None),
        ];
        assert_eq!(OrgMembership::primary_org(&rows, "user", Instant::from_unix_secs(9)).unwrap(), None);
        assert_eq!(
            OrgMembership::primary_org(&rows, "user", Instant::from_unix_secs(10)).unwrap(),
            Some("old")
        );
        assert_eq!(
            OrgMembership::primary_org(&rows, "user", Instant::from_unix_secs(20)).unwrap(),
            Some("new")
        );
        rows[2].base.deleted_at = 21;
        assert_eq!(OrgMembership::primary_org(&rows, "user", Instant::from_unix_secs(20)).unwrap(), None);
        rows.push(membership("user", "overlap", 10, Some(20)));
        assert!(matches!(
            OrgMembership::primary_org(&rows, "user", Instant::from_unix_secs(10)),
            Err(Error::ConflictError(message)) if message == "用户存在重叠主属组织关系"
        ));
    }

    // 验证未来起止边界的选取、软删除过滤及无后续边界时的结果。
    #[test]
    fn membership_boundary_includes_future_starts_and_current_ends_only() {
        let mut rows = vec![
            membership("user", "current", 10, Some(30)),
            membership("user", "future", 20, None),
            membership("other", "ignored", 11, Some(12)),
        ];
        assert_eq!(
            OrgMembership::next_boundary(&rows, "user", Instant::from_unix_secs(10)),
            Some(Instant::from_unix_secs(20))
        );
        rows[1].base.deleted_at = 11;
        assert_eq!(
            OrgMembership::next_boundary(&rows, "user", Instant::from_unix_secs(10)),
            Some(Instant::from_unix_secs(30))
        );
        assert_eq!(OrgMembership::next_boundary(&rows, "user", Instant::from_unix_secs(30)), None);
        assert_eq!(OrgMembership::next_boundary(&rows, "missing", Instant::from_unix_secs(10)), None);
        assert_eq!(
            OrgMembership::next_boundary(&rows, "user", Instant::from_unix_secs(5)),
            Some(Instant::from_unix_secs(10))
        );
    }

    // 构造指定父级的启用部门，供组织树路径和范围测试复用。
    fn unit(id: &str, parent: Option<&str>) -> OrgUnit {
        OrgUnit::new(
            id.into(),
            id.into(),
            parent.map(str::to_owned),
            OrgUnitKind::Department,
            "admin".into(),
            "初始化".into(),
        )
        .unwrap()
    }

    // 验证自引用、跨节点循环和缺失父级都无法建树。
    #[test]
    fn tree_rejects_cycles_and_missing_parents() {
        assert!(OrgTree::new(&[unit("a", Some("a"))]).is_err());
        assert!(OrgTree::new(&[unit("a", Some("b")), unit("b", Some("a"))]).is_err());
        assert!(OrgTree::new(&[unit("a", Some("missing"))]).is_err());
    }

    // 验证下级须显式展开、路径从根开始，停用节点会截断其下级范围。
    #[test]
    fn descendants_are_explicit_and_disabled_branches_are_excluded() {
        let mut nodes = vec![unit("a", None), unit("b", Some("a")), unit("c", Some("b")), unit("x", None)];
        let tree = OrgTree::new(&nodes).unwrap();
        assert_eq!(tree.expand("a", false).unwrap(), BTreeSet::from(["a".into()]));
        assert_eq!(tree.expand("a", true).unwrap().len(), 3);
        assert_eq!(
            tree.path("c").unwrap().iter().map(|n| n.base.id.as_str()).collect::<Vec<_>>(),
            ["a", "b", "c"]
        );

        nodes[1].enabled = false;
        assert_eq!(OrgTree::new(&nodes).unwrap().expand("a", true).unwrap(), BTreeSet::from(["a".into()]));
    }

    // 在 5100 个节点的多根树上反复校验目标分支及有序结果，并记录耗时。
    #[test]
    fn large_subtree_expansion_sample() {
        let mut nodes = Vec::new();
        for root in 0..100 {
            let id = format!("root-{root:03}");
            nodes.push(unit(&id, None));
            for child in 0..50 {
                nodes.push(unit(&format!("{id}-{child:03}"), Some(&id)));
            }
        }
        let tree = OrgTree::new(&nodes).unwrap();
        let expected = once("root-042".to_string())
            .chain((0..50).map(|child| format!("root-042-{child:03}")))
            .collect::<BTreeSet<_>>();
        let started = Stopwatch::now();
        for _ in 0..20 {
            assert_eq!(tree.expand("root-042", true).unwrap(), expected);
        }
        eprintln!("org_subtree_5100_nodes_20_runs={:?}", started.elapsed());
    }

    // 验证乱序建树仍按 ID 返回范围，停用祖先截断分支，未知目标报错。
    #[test]
    fn subtree_index_preserves_disabled_ancestor_and_sorted_output_rules() {
        let mut nodes = vec![
            unit("z", Some("b")),
            unit("b", Some("a")),
            unit("a", None),
            unit("c", Some("a")),
            unit("x", None),
        ];
        let tree = OrgTree::new(&nodes).unwrap();
        assert_eq!(
            tree.expand("a", true).unwrap(),
            BTreeSet::from(["a".into(), "b".into(), "c".into(), "z".into()])
        );
        assert!(tree.expand("unknown", true).is_err());

        nodes[1].enabled = false;
        let tree = OrgTree::new(&nodes).unwrap();
        for descendants in [false, true] {
            assert!(tree.expand("z", descendants).unwrap().is_empty());
            assert!(tree.expand("b", descendants).unwrap().is_empty());
        }
        assert_eq!(tree.expand("a", true).unwrap(), BTreeSet::from(["a".into(), "c".into()]));
    }

    // 验证建树优先报告重复 ID，再按输入顺序报告父链的首个错误。
    #[test]
    fn subtree_index_keeps_full_tree_validation_and_error_precedence() {
        let duplicate = [unit("a", Some("missing")), unit("a", None)];
        assert!(matches!(OrgTree::new(&duplicate), Err(Error::ValidationError(message))
            if message == "组织身份重复"));
        let cycle_first = [unit("a", Some("a")), unit("b", Some("missing"))];
        assert!(matches!(OrgTree::new(&cycle_first), Err(Error::ValidationError(message))
            if message == "组织父子关系不得形成环"));
        let missing_first = [unit("b", Some("missing")), unit("a", Some("a"))];
        assert!(matches!(OrgTree::new(&missing_first), Err(Error::ValidationError(message))
            if message == "组织节点不存在"));
    }

    // 验证调岗交界归新关系、相邻区间不重叠、无终点区间持续有效并拒绝空区间。
    #[test]
    fn validity_is_half_open_and_adjacent_transfers_do_not_overlap() {
        let at = Instant::from_unix_secs;
        let before = OrgValidity { valid_from: at(1), valid_to: Some(at(10)) };
        let after = OrgValidity { valid_from: at(10), valid_to: None };
        assert!(before.contains(at(1)));
        assert!(!before.contains(at(10)));
        assert!(after.contains(at(10)));
        assert!(!before.overlaps(&after));
        assert!(after.overlaps(&after));
        assert!(OrgValidity { valid_from: at(1), valid_to: Some(at(1)) }.validate().is_err());
    }
}
