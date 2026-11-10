//! 来源授权行的分页差异计算，内存只保留本次目标和当前页。
use std::collections::BTreeSet;

use super::{GrantSource, GrantTarget, ResourceGrant};

pub(crate) struct GrantRows {
    targets: BTreeSet<GrantTarget>,
    missing: BTreeSet<GrantTarget>,
    version: u64,
}
impl GrantRows {
    /// 冻结来源目标集合与本次版本。
    /// # 参数
    /// source 为已验证来源。
    /// # 返回
    /// 只保存当前目标的分页差异状态。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(source: &GrantSource) -> Self {
        let targets = source.targets.iter().cloned().collect::<BTreeSet<_>>();
        Self { missing: targets.clone(), targets, version: source.base.version }
    }

    /// 更新当前页，所有历史行推进来源版本，移除已存在的待新增目标。
    /// # 参数
    /// rows 为当前来源的一页未删除记录。
    /// # 返回
    /// 原地更新状态及来源版本。
    /// # 错误
    /// 不返回错误；持久化由仓储执行。
    pub(crate) fn refresh(&mut self, rows: &mut [ResourceGrant]) {
        for row in rows {
            let target = GrantTarget { resource: row.resource, id: row.resource_id.clone() };
            row.active = self.targets.contains(&target);
            row.source_version = self.version;
            self.missing.remove(&target);
        }
    }

    /// 历史页全部完成后返回仍需创建的目标。
    /// # 参数
    /// self 为已经处理全部历史页的差异状态。
    /// # 返回
    /// 唯一且有序的新增目标。
    /// # 错误
    /// 不返回错误。
    pub(crate) fn missing(self) -> BTreeSet<GrantTarget> {
        self.missing
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::entity::resource_access::{ResourceKind, SourceKey};

    /// 不同资源同名 ID 保持独立；移除目标失效，保留目标推进版本，新目标只创建一次。
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
