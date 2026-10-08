//! 工作项责任范围使用的存储无关集合代数。

use std::collections::{BTreeMap, BTreeSet};

use super::{DataScope, DataScopeType};

/// 组织范围覆盖事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizationCoverage {
    /// 覆盖全部组织。
    All,
    /// 只覆盖规范化后的明确组织集合。
    Targets(Vec<String>),
}

impl OrganizationCoverage {
    /// 从显式组织 ID 形成规范化覆盖；`*` 表示全部组织。
    ///
    /// # 参数
    /// * `targets` - 组织 ID；重复项会被去掉。
    ///
    /// # 返回
    /// 含 `*` 时返回 `All`，即使同时还有其他 ID。只有明确 ID 时返回按字典序排列的 `Targets`。输入为空时返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_targets(targets: impl IntoIterator<Item = String>) -> Option<Self> {
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        if targets.contains("*") {
            return Some(Self::All);
        }
        (!targets.is_empty()).then(|| Self::Targets(targets.into_iter().collect()))
    }

    /// 从 DataScope 事实形成明确覆盖；空范围不隐式解释为 All。
    ///
    /// # 参数
    /// * `scopes` - 同一责任主体上的范围事实。
    ///
    /// # 返回
    /// 任一公司级范围返回 `All`。否则把组织与团队目标去重并排序后返回 `Targets`。没有这些目标时返回 `None`。本人负责和协作参与不贡献组织覆盖。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_scopes(scopes: &[DataScope]) -> Option<Self> {
        if scopes.iter().any(|scope| scope.scope_type == DataScopeType::Company) {
            return Some(Self::All);
        }
        let targets = scopes
            .iter()
            .filter(|scope| matches!(scope.scope_type, DataScopeType::Organization | DataScopeType::Team))
            .flat_map(|scope| scope.scope_targets.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        (!targets.is_empty()).then_some(Self::Targets(targets))
    }

    /// 形成明确目标集合。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `All` 返回单个 `None`；明确目标按现有顺序返回 `Some(id)`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn targets(&self) -> Vec<Option<String>> {
        match self {
            Self::All => vec![None],
            Self::Targets(targets) => targets.iter().cloned().map(Some).collect(),
        }
    }

    /// 计算两个组织覆盖的交集。
    ///
    /// # 参数
    /// * `other` - 另一份组织覆盖。
    ///
    /// # 返回
    /// 任一侧为 `All` 时返回另一侧的克隆。两侧都是明确目标时返回交集；交集为空则返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        match (self, other) {
            (Self::All, coverage) | (coverage, Self::All) => Some(coverage.clone()),
            (Self::Targets(left), Self::Targets(right)) => {
                let right = right.iter().collect::<BTreeSet<_>>();
                let targets =
                    left.iter().filter(|target| right.contains(target)).cloned().collect::<Vec<_>>();
                (!targets.is_empty()).then_some(Self::Targets(targets))
            },
        }
    }

    /// 判断当前覆盖是否包含指定组织。
    ///
    /// # 参数
    /// * `organization_id` - 待判断的组织 ID。
    ///
    /// # 返回
    /// `All` 对任意 ID 返回 `true`。明确目标按已排序列表二分查找，命中时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn covers(&self, organization_id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Targets(targets) => {
                targets.binary_search_by(|target| target.as_str().cmp(organization_id)).is_ok()
            },
        }
    }
}

/// 角色与组织责任范围的规范化集合。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResponsibilityScopeSet(Vec<(String, Option<String>)>);

impl ResponsibilityScopeSet {
    /// 构造去重并稳定排序的责任范围集合。
    ///
    /// 同一角色存在 `None` 时表示覆盖全部组织，必须吸收该角色的明确组织项。
    ///
    /// # 参数
    /// * `values` - 角色 ID 与组织 ID。组织为 `None` 表示该角色覆盖全部组织。
    ///
    /// # 返回
    /// 返回按角色 ID、组织 ID 稳定排序的集合。同一角色一旦出现 `None`，只保留这一项。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(values: impl IntoIterator<Item = (String, Option<String>)>) -> Self {
        let mut normalized = BTreeMap::<String, Option<BTreeSet<String>>>::new();
        for (role_id, organization_id) in values {
            match normalized.entry(role_id) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(organization_id.map(|value| BTreeSet::from([value])));
                },
                std::collections::btree_map::Entry::Occupied(mut entry) => {
                    let Some(targets) = entry.get_mut() else {
                        continue;
                    };
                    match organization_id {
                        Some(value) => {
                            targets.insert(value);
                        },
                        None => {
                            entry.insert(None);
                        },
                    }
                },
            }
        }
        Self(
            normalized
                .into_iter()
                .flat_map(|(role_id, targets)| match targets {
                    Some(targets) => {
                        targets.into_iter().map(|target| (role_id.clone(), Some(target))).collect::<Vec<_>>()
                    },
                    None => vec![(role_id, None)],
                })
                .collect(),
        )
    }

    /// 合并责任范围集合。
    ///
    /// # 参数
    /// * `other` - 需要并入的责任范围。
    ///
    /// # 返回
    /// 返回两侧合并后再规范化的集合。同一角色的全部组织覆盖会吸收明确组织。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn union(&self, other: &Self) -> Self {
        Self::new(self.0.iter().cloned().chain(other.0.iter().cloned()))
    }

    /// 计算两个责任范围集合的交集。
    ///
    /// `None` 表示对应角色覆盖全部组织；与明确组织集合相交时保留明确集合。
    ///
    /// # 参数
    /// * `other` - 另一份责任范围。
    ///
    /// # 返回
    /// 只保留两侧都出现的角色。两侧都覆盖全部组织时保留 `None`；一侧为全部组织时保留另一侧的明确组织；两侧都是明确组织时保留交集。没有共同角色时返回空集合。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn intersect(&self, other: &Self) -> Self {
        let roles = self
            .0
            .iter()
            .map(|(role, _)| role)
            .chain(other.0.iter().map(|(role, _)| role))
            .collect::<BTreeSet<_>>();
        let values = roles.into_iter().flat_map(|role| {
            let left = organizations_for_role(&self.0, role);
            let right = organizations_for_role(&other.0, role);
            match (left, right) {
                (Some(None), Some(organizations)) | (Some(organizations), Some(None)) => organizations
                    .map(|targets| {
                        targets.into_iter().map(|target| (role.clone(), Some(target))).collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![(role.clone(), None)]),
                (Some(Some(left)), Some(Some(right))) => {
                    left.intersection(&right).cloned().map(|target| (role.clone(), Some(target))).collect()
                },
                (None, _) | (_, None) => Vec::new(),
            }
        });
        Self::new(values)
    }

    /// 判断集合是否覆盖指定角色与组织。
    ///
    /// # 参数
    /// * `role_id` - 角色 ID。
    /// * `organization_id` - 组织 ID。
    ///
    /// # 返回
    /// 存在该角色，且其组织为 `None` 或等于 `organization_id` 时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn covers(&self, role_id: &str, organization_id: &str) -> bool {
        self.0.iter().any(|(role, organization)| {
            role == role_id && organization.as_deref().is_none_or(|allowed| allowed == organization_id)
        })
    }

    /// 返回规范化范围切片。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(角色 ID, 组织 ID)` 的只读切片。组织为 `None` 表示该角色覆盖全部组织。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_slice(&self) -> &[(String, Option<String>)] {
        &self.0
    }
}

/// 返回指定角色的覆盖；外层 `None` 表示角色不存在，内层 `None` 表示全部组织。
fn organizations_for_role(
    values: &[(String, Option<String>)],
    role_id: &str,
) -> Option<Option<BTreeSet<String>>> {
    let mut found = false;
    let mut targets = BTreeSet::new();
    for (_, organization_id) in values.iter().filter(|(role, _)| role == role_id) {
        found = true;
        let Some(organization_id) = organization_id else {
            return Some(None);
        };
        targets.insert(organization_id.clone());
    }
    found.then_some(Some(targets))
}

#[cfg(test)]
mod tests {
    use super::{OrganizationCoverage, ResponsibilityScopeSet};

    #[test]
    fn coverage_intersection_and_covers_are_set_based() {
        let left = OrganizationCoverage::Targets(vec!["a".to_string(), "b".to_string()]);
        let right = OrganizationCoverage::Targets(vec!["b".to_string(), "c".to_string()]);
        assert_eq!(left.intersect(&right), Some(OrganizationCoverage::Targets(vec!["b".to_string()])));
        assert!(OrganizationCoverage::All.covers("any"));
        assert!(!left.covers("c"));
    }

    #[test]
    fn responsibility_scope_normalizes_and_supports_all_organizations() {
        let set = ResponsibilityScopeSet::new([
            ("role".to_string(), Some("org".to_string())),
            ("role".to_string(), None),
            ("role".to_string(), None),
        ]);
        assert_eq!(set.as_slice(), &[("role".to_string(), None)]);
        assert!(set.covers("role", "other"));
        assert!(!set.covers("other-role", "org"));
    }

    #[test]
    fn responsibility_scope_union_and_intersection_apply_role_wildcards() {
        let left = ResponsibilityScopeSet::new([
            ("buyer".to_string(), Some("org-a".to_string())),
            ("buyer".to_string(), Some("org-b".to_string())),
            ("finance".to_string(), None),
        ]);
        let right = ResponsibilityScopeSet::new([
            ("buyer".to_string(), Some("org-b".to_string())),
            ("buyer".to_string(), Some("org-c".to_string())),
            ("finance".to_string(), Some("org-a".to_string())),
        ]);

        assert_eq!(
            left.intersect(&right).as_slice(),
            &[
                ("buyer".to_string(), Some("org-b".to_string())),
                ("finance".to_string(), Some("org-a".to_string())),
            ]
        );
        assert_eq!(
            left.union(&right).as_slice(),
            &[
                ("buyer".to_string(), Some("org-a".to_string())),
                ("buyer".to_string(), Some("org-b".to_string())),
                ("buyer".to_string(), Some("org-c".to_string())),
                ("finance".to_string(), None),
            ]
        );
        assert!(ResponsibilityScopeSet::default().intersect(&left).as_slice().is_empty());
    }
}
