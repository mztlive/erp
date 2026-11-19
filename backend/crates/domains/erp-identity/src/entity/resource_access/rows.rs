//! 来源授权行的分页差异计算，内存只保留本次目标和当前页。
use super::{GrantSource, GrantTarget, ResourceGrant};

/// 跨页记录本次发布的每个目标是否已有授权行。
pub(crate) struct GrantRows<'a> {
    // 按资源和 ID 排序去重；标记表示已扫描到该目标的历史行。
    targets: Vec<(&'a GrantTarget, bool)>,
    // 已保存的来源记录版本，刷新时写入每条旧行。
    version: u64,
}
impl<'a> GrantRows<'a> {
    /// 用已验证的来源冻结目标与版本；扫描历史行前，所有目标均视为待新增。
    pub(crate) fn new(source: &'a GrantSource) -> Self {
        let mut targets = source.targets.iter().map(|target| (target, false)).collect::<Vec<_>>();
        targets.sort_unstable_by_key(|(target, _)| *target);
        targets.dedup_by_key(|(target, _)| *target);
        Self { targets, version: source.base.version }
    }

    /// 原地刷新当前页的未删除授权行，使仍在目标集合中的行生效，并将所有行推进到本次来源版本。
    /// 标记本页已有的目标，仓储负责持久化。
    pub(crate) fn refresh(&mut self, rows: &mut [ResourceGrant]) {
        for row in rows {
            let target = self.targets.binary_search_by(|(target, _)| {
                (target.resource, target.id.as_str()).cmp(&(row.resource, row.resource_id.as_str()))
            });
            row.active = target.is_ok();
            if let Ok(index) = target {
                self.targets[index].1 = true;
            }
            row.source_version = self.version;
        }
    }

    /// 处理完全部历史页后，按资源和 ID 顺序交出仍需创建授权行的唯一目标。
    pub(crate) fn missing(self) -> impl Iterator<Item = &'a GrantTarget> {
        self.targets.into_iter().filter_map(|(target, seen)| (!seen).then_some(target))
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
        let purchase = GrantTarget { resource: ResourceKind::PurchaseOrder, id: "aaa".into() };
        source.targets = vec![purchase, target.clone(), added, target];
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

        let mut second = vec![row(ResourceKind::PurchaseOrder), row(ResourceKind::SalesOrder)];
        delta.refresh(&mut second);
        assert!(!second[0].active);
        assert!(second[1].active);
        assert!(second.iter().all(|row| row.source_version == 7));
        assert!(
            delta
                .missing()
                .map(|target| (target.resource, target.id.as_str()))
                .eq([(ResourceKind::SalesOrder, "new"), (ResourceKind::PurchaseOrder, "aaa")])
        );

        source.targets.clear();
        let mut empty = GrantRows::new(&source);
        empty.refresh(&mut first);
        assert!(!first[0].active);
        assert!(empty.missing().next().is_none());
    }
}
