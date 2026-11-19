//! 来源授权行的分页差异计算，内存只保留本次目标和当前页。
use std::collections::BTreeSet;

use super::{GrantSource, GrantTarget, ResourceGrant};

/// 记录来源授权的分页同步状态，只保留本次目标集合和待建目标。
pub(crate) struct GrantRows {
    // 来源本次发布的完整读取目标，用于判定旧行是否仍应生效。
    targets: BTreeSet<GrantTarget>,
    // 尚未在任何已处理页中找到对应授权行的目标。
    missing: BTreeSet<GrantTarget>,
    // 已保存的来源记录版本，刷新时写入每条旧行。
    version: u64,
}
impl GrantRows {
    /// 用已验证的来源冻结目标与版本；扫描历史行前，所有目标均视为待新增。
    pub(crate) fn new(source: &GrantSource) -> Self {
        let targets = source.targets.iter().cloned().collect::<BTreeSet<_>>();
        Self { missing: targets.clone(), targets, version: source.base.version }
    }

    /// 原地刷新当前页的未删除授权行，使仍在目标集合中的行生效，并将所有行推进到本次来源版本。
    /// 从待新增集合剔除本页已存在的目标；仓储负责持久化。
    pub(crate) fn refresh(&mut self, rows: &mut [ResourceGrant]) {
        for row in rows {
            let target = GrantTarget { resource: row.resource, id: row.resource_id.clone() };
            row.active = self.targets.contains(&target);
            row.source_version = self.version;
            self.missing.remove(&target);
        }
    }

    /// 处理完全部历史页后，取出仍需创建授权行的有序唯一目标。
    pub(crate) fn missing(self) -> BTreeSet<GrantTarget> {
        self.missing
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::entity::resource_access::{ResourceKind, SourceKey};

    // 验证跨页按资源和 ID 匹配；保留行和失效行都推进版本，仅缺行目标待新增，空目标使旧行失效。
    #[test]
    fn paged_delta_preserves_resource_identity_and_versions() {
        let mut source = GrantSource::new(SourceKey {
            rule: "manual".into(),
            business_type: "manual".into(),
            business_id: "command".into(),
            recipient: "user".into(),
        })
        .unwrap();
        source.base.version = 7;
        let target = GrantTarget { resource: ResourceKind::SalesOrder, id: "same".into() };
        let added = GrantTarget { resource: ResourceKind::SalesOrder, id: "new".into() };
        source.targets = vec![target.clone(), added.clone()];
        let row = |resource| ResourceGrant {
            base: BaseModel::new("row".into()),
            recipient: "user".into(),
            source_id: source.base.id.clone(),
            source_version: 6,
            resource,
            resource_id: "same".into(),
            action: "read".into(),
            active: true,
        };

        let mut delta = GrantRows::new(&source);
        let mut first = vec![row(ResourceKind::SalesOrder)];
        delta.refresh(&mut first);
        assert!(first[0].active);
        assert_eq!(first[0].source_version, 7);

        let mut second = vec![row(ResourceKind::PurchaseOrder)];
        delta.refresh(&mut second);
        assert!(!second[0].active);
        assert_eq!(second[0].source_version, 7);
        assert_eq!(delta.missing(), BTreeSet::from([added]));

        source.targets.clear();
        let mut empty = GrantRows::new(&source);
        empty.refresh(&mut first);
        assert!(!first[0].active);
        assert!(empty.missing().is_empty());
    }
}
