//! 销售单首次生效的人员和业务组织归属；金额继续按当前正式版本计算。

use erp_core::common::time::Instant;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 生效时组织路径节点；历史部门分组不得读取当前树重算。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttributionOrgNode {
    pub id: String,
    pub name: String,
}

/// 与首次销售生效在同一事务保存的不可变归属快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesAttribution {
    pub attribution_user_id: String,
    pub attribution_user_name: String,
    pub attribution_org_unit_id: String,
    pub attribution_org_unit_name: String,
    pub attributed_at: Instant,
    pub attribution_version: u64,
    pub organization_version: u64,
    pub org_path: Vec<AttributionOrgNode>,
}

impl SalesAttribution {
    /// 核对快照与单据责任身份一致，并要求完整、无重复的组织路径。
    ///
    /// # 错误
    /// 缺名称、版本、路径，路径末节点不一致或身份不匹配时拒绝生效。
    pub fn validate(&self, owner: &str, org: &str) -> Result<()> {
        let identities_match = self.attribution_user_id == owner && self.attribution_org_unit_id == org;
        let path_matches = self
            .org_path
            .last()
            .is_some_and(|node| node.id == org && node.name == self.attribution_org_unit_name);
        let valid_names = !self.attribution_user_name.trim().is_empty()
            && !self.attribution_org_unit_name.trim().is_empty()
            && self.org_path.iter().all(|node| !node.id.trim().is_empty() && !node.name.trim().is_empty());
        let unique =
            self.org_path.iter().map(|node| &node.id).collect::<std::collections::BTreeSet<_>>().len()
                == self.org_path.len();
        if !identities_match
            || !path_matches
            || !valid_names
            || !unique
            || self.attribution_version != 1
            || self.organization_version == 0
        {
            return Err(Error::from("销售首次生效必须具备完整且匹配的人员和业务组织归属快照"));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn freeze_fixture(order: &mut super::SalesOrder) {
    order
        .freeze_attribution(SalesAttribution {
            attribution_user_id: order.sales_owner_user_id.clone(),
            attribution_user_name: "测试销售".into(),
            attribution_org_unit_id: order.business_org_unit_id.clone(),
            attribution_org_unit_name: "销售一部".into(),
            attributed_at: Instant::from_unix_secs(1_800_000_000),
            attribution_version: 1,
            organization_version: 1,
            org_path: vec![AttributionOrgNode {
                id: order.business_org_unit_id.clone(),
                name: "销售一部".into(),
            }],
        })
        .unwrap();
}
