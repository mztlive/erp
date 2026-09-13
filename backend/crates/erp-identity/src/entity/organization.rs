//! 内部组织、主属成员和按角色授予的管理关系；不承载法人或仓库身份。

use std::collections::{BTreeMap, BTreeSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 内部组织节点类型。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrgUnitKind {
    Department,
    Team,
}

/// 内部组织节点；历史节点只允许停用。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgUnit {
    #[serde(flatten)]
    pub base: BaseModel,
    pub name: String,
    pub parent_id: Option<String>,
    pub kind: OrgUnitKind,
    pub enabled: bool,
    pub changed_by: String,
    pub reason: String,
}

/// UTC 半开有效期；不改变客户归属的自然日规则。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrgValidity {
    pub valid_from: Instant,
    pub valid_to: Option<Instant>,
}

impl OrgValidity {
    /// 校验有效期起止顺序。
    ///
    /// # 错误
    /// 空区间和倒置区间均返回校验错误。
    pub fn validate(&self) -> Result<()> {
        if self.valid_to.is_some_and(|end| end <= self.valid_from) {
            return Err(Error::ValidationError("有效期结束必须晚于开始".into()));
        }
        Ok(())
    }

    /// 判断指定服务端时点是否处于有效期。
    ///
    /// # 返回
    /// 起点包含，终点排除。
    pub fn contains(&self, at: Instant) -> bool {
        self.valid_from <= at && self.valid_to.is_none_or(|end| at < end)
    }

    /// 判断两个半开区间是否重叠。
    ///
    /// # 返回
    /// 相邻区间返回 false，无终点表示持续有效。
    pub fn overlaps(&self, other: &Self) -> bool {
        self.valid_to.is_none_or(|end| other.valid_from < end)
            && other.valid_to.is_none_or(|end| self.valid_from < end)
    }
}

/// 用户唯一主属组织的时段事实。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgMembership {
    #[serde(flatten)]
    pub base: BaseModel,
    pub user_id: String,
    pub org_unit_id: String,
    #[serde(flatten)]
    pub validity: OrgValidity,
    pub changed_by: String,
    pub reason: String,
}

/// 管理范围仅由当前持有且有完整动作权限的指定角色激活。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct OrgManagementAssignment {
    #[serde(flatten)]
    pub base: BaseModel,
    pub user_id: String,
    pub role_id: String,
    pub org_unit_id: String,
    pub include_descendants: bool,
    #[serde(flatten)]
    pub validity: OrgValidity,
    pub granted_by: String,
    pub reason: String,
}

/// 当前组织树的已验证索引。
pub struct OrgTree<'a> {
    nodes: BTreeMap<&'a str, &'a OrgUnit>,
}

impl<'a> OrgTree<'a> {
    /// 验证唯一身份、父节点存在及无环，建立组织树索引。
    ///
    /// # 错误
    /// 重复节点、悬空父节点、自引用或环均拒绝。
    pub fn new(nodes: &'a [OrgUnit]) -> Result<Self> {
        let tree = Self {
            nodes: nodes.iter().map(|node| (node.base.id.as_str(), node)).collect(),
        };
        if tree.nodes.len() != nodes.len() {
            return Err(Error::ValidationError("组织身份重复".into()));
        }
        for node in nodes {
            tree.path(&node.base.id)?;
        }
        Ok(tree)
    }

    /// 返回根节点至当前节点的路径，包含当前节点。
    ///
    /// # 错误
    /// 路径存在环或缺失节点时拒绝，不生成残缺归属快照。
    pub fn path(&self, id: &str) -> Result<Vec<&'a OrgUnit>> {
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err(Error::ValidationError("组织父子关系不得形成环".into()));
            }
            let node = self
                .nodes
                .get(id)
                .ok_or_else(|| Error::ValidationError("组织节点不存在".into()))?;
            path.push(*node);
            current = node.parent_id.as_deref();
        }
        path.reverse();
        Ok(path)
    }

    /// 展开明确目标及可选的有效下级。
    ///
    /// # 错误
    /// 未知节点拒绝；停用节点及位于停用祖先下的节点不贡献范围。
    pub fn expand(&self, id: &str, descendants: bool) -> Result<BTreeSet<String>> {
        let target_path = self.path(id)?;
        if target_path.iter().any(|node| !node.enabled) {
            return Ok(BTreeSet::new());
        }
        let mut result = BTreeSet::from([id.to_string()]);
        if !descendants {
            return Ok(result);
        }
        for candidate in self.nodes.keys() {
            let path = self.path(candidate)?;
            if path.iter().all(|node| node.enabled) && path.iter().any(|node| node.base.id == id) {
                result.insert((*candidate).to_string());
            }
        }
        Ok(result)
    }
}

impl OrgUnit {
    /// 构造启用组织；父子关系由完整树在事务中校验。
    ///
    /// # 错误
    /// 名称、操作人或原因缺失及过长时拒绝。
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
            enabled: true,
            changed_by: normalize_required_text(actor, "操作人不能为空", 128, "操作人身份过长")?,
            reason: normalize_required_text(reason, "变更原因不能为空", 1000, "变更原因过长")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn tree_rejects_cycles_and_missing_parents() {
        assert!(OrgTree::new(&[unit("a", Some("a"))]).is_err());
        assert!(OrgTree::new(&[unit("a", Some("b")), unit("b", Some("a"))]).is_err());
        assert!(OrgTree::new(&[unit("a", Some("missing"))]).is_err());
    }

    #[test]
    fn descendants_are_explicit_and_disabled_branches_are_excluded() {
        let mut nodes = vec![
            unit("a", None),
            unit("b", Some("a")),
            unit("c", Some("b")),
            unit("x", None),
        ];
        let tree = OrgTree::new(&nodes).unwrap();
        assert_eq!(tree.expand("a", false).unwrap(), BTreeSet::from(["a".into()]));
        assert_eq!(tree.expand("a", true).unwrap().len(), 3);
        assert_eq!(
            tree.path("c")
                .unwrap()
                .iter()
                .map(|n| n.base.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        nodes[1].enabled = false;
        assert_eq!(
            OrgTree::new(&nodes).unwrap().expand("a", true).unwrap(),
            BTreeSet::from(["a".into()])
        );
    }

    #[test]
    fn validity_is_half_open_and_adjacent_transfers_do_not_overlap() {
        let at = Instant::from_unix_secs;
        let before = OrgValidity {
            valid_from: at(1),
            valid_to: Some(at(10)),
        };
        let after = OrgValidity {
            valid_from: at(10),
            valid_to: None,
        };
        assert!(before.contains(at(1)));
        assert!(!before.contains(at(10)));
        assert!(after.contains(at(10)));
        assert!(!before.overlaps(&after));
        assert!(after.overlaps(&after));
        assert!(OrgValidity {
            valid_from: at(1),
            valid_to: Some(at(1))
        }
        .validate()
        .is_err());
    }
}
