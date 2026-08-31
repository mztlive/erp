//! 卡实例基线身份、关系与首次快照聚合工厂。

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::{
    ExternalIdentityMapId, MallBalanceSnapshotId, MallCardInstanceId, SalesOrderId, SalesOrderRevisionId,
};
use crate::money::Amount;

use super::{
    CardSourceType, MallBalanceSnapshot, MallBalanceSnapshotData, MallCardInstance, MallCardInstanceData,
};

/// 唯一索引仲裁的卡基线身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardBaselineIdentity {
    /// 来源商城。
    pub mall_id: String,
    /// 商城不可逆稳定引用。
    pub opaque_instance_ref: String,
}

/// 卡基线的全部不可变关系字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardBaselineRelation {
    /// 来源销售单外部身份。
    pub origin_sales_order_source_identity_id: ExternalIdentityMapId,
    /// ERP 销售单。
    pub origin_sales_order_id: SalesOrderId,
    /// 基线时销售单版本。
    pub origin_sales_order_revision_id: SalesOrderRevisionId,
    /// 商城基线版本。
    pub source_baseline_version: Option<String>,
    /// 初始余额。
    pub initial_balance: Amount,
    /// 基线时点。
    pub baseline_at: Instant,
    /// 来源类型。
    pub source_type: CardSourceType,
}

/// 卡实例基线及首次余额快照聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MallCardBaselineAggregate {
    instance: MallCardInstance,
    initial_snapshot: MallBalanceSnapshot,
}

impl MallCardBaselineAggregate {
    /// 以调用方注入的两个 ID 构造基线与唯一首次快照。
    ///
    /// # 错误
    /// 基线或快照字段违反实体不变量时返回错误。
    pub fn new(
        instance_id: MallCardInstanceId,
        snapshot_id: MallBalanceSnapshotId,
        data: MallCardInstanceData,
    ) -> Result<Self> {
        let instance = MallCardInstance::new(instance_id, data)?;
        let initial_snapshot = MallBalanceSnapshot::new(
            snapshot_id,
            MallBalanceSnapshotData {
                mall_card_instance_id: MallCardInstanceId::new(instance.base.id.clone()),
                snapshot_at: instance.baseline_at,
                balance: instance.initial_balance,
                source_snapshot_version: None,
                source_event_id: format!("baseline:{}:{}", instance.mall_id, instance.opaque_instance_ref),
            },
        )?;
        Ok(Self {
            instance,
            initial_snapshot,
        })
    }

    /// 返回卡基线实体。
    pub fn instance(&self) -> &MallCardInstance {
        &self.instance
    }

    /// 消费聚合并返回卡基线与首次快照。
    pub fn into_parts(self) -> (MallCardInstance, MallBalanceSnapshot) {
        (self.instance, self.initial_snapshot)
    }
}

impl MallCardInstance {
    /// 返回唯一索引使用的稳定身份。
    pub fn baseline_identity(&self) -> CardBaselineIdentity {
        CardBaselineIdentity {
            mall_id: self.mall_id.clone(),
            opaque_instance_ref: self.opaque_instance_ref.clone(),
        }
    }

    /// 返回全部不可变基线关系。
    pub fn baseline_relation(&self) -> CardBaselineRelation {
        CardBaselineRelation {
            origin_sales_order_source_identity_id: self.origin_sales_order_source_identity_id.clone(),
            origin_sales_order_id: self.origin_sales_order_id.clone(),
            origin_sales_order_revision_id: self.origin_sales_order_revision_id.clone(),
            source_baseline_version: self.source_baseline_version.clone(),
            initial_balance: self.initial_balance,
            baseline_at: self.baseline_at,
            source_type: self.source_type,
        }
    }

    /// 判断另一基线是否是同一身份及完全相同的不可变关系。
    pub fn same_baseline_as(&self, requested: &Self) -> bool {
        self.baseline_identity() == requested.baseline_identity()
            && self.baseline_relation() == requested.baseline_relation()
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::MallCardBaselineAggregate;
    use crate::card_instance::{CardSourceType, MallCardInstanceData};
    use crate::common::time::Instant;
    use crate::ids::{
        ExternalIdentityMapId, MallBalanceSnapshotId, MallCardInstanceId, SalesOrderId, SalesOrderRevisionId,
    };
    use crate::money::Amount;

    fn data() -> MallCardInstanceData {
        MallCardInstanceData {
            mall_id: " mall ".to_string(),
            opaque_instance_ref: " card-ref ".to_string(),
            origin_sales_order_source_identity_id: ExternalIdentityMapId::new("external-1"),
            origin_sales_order_id: SalesOrderId::new("so-1"),
            origin_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
            source_baseline_version: Some(" v1 ".to_string()),
            initial_balance: Amount::from_str("10.00").unwrap(),
            baseline_at: Instant::from_unix_secs(100),
            source_type: CardSourceType::Realtime,
        }
    }

    #[test]
    fn aggregate_forms_exactly_one_consistent_initial_snapshot() {
        let aggregate = MallCardBaselineAggregate::new(
            MallCardInstanceId::new("card-1"),
            MallBalanceSnapshotId::new("snapshot-1"),
            data(),
        )
        .unwrap();
        let (instance, snapshot) = aggregate.into_parts();
        assert_eq!(snapshot.mall_card_instance_id.to_string(), instance.base.id);
        assert_eq!(snapshot.snapshot_at, instance.baseline_at);
        assert_eq!(snapshot.balance, instance.initial_balance);
        assert_eq!(snapshot.source_event_id, "baseline:mall:card-ref");
    }

    #[test]
    fn every_relation_field_participates_in_replay_equality() {
        let left = MallCardBaselineAggregate::new(
            MallCardInstanceId::new("card-1"),
            MallBalanceSnapshotId::new("snapshot-1"),
            data(),
        )
        .unwrap()
        .into_parts()
        .0;
        let mut changed = data();
        changed.origin_sales_order_revision_id = SalesOrderRevisionId::new("sor-2");
        let right = MallCardBaselineAggregate::new(
            MallCardInstanceId::new("card-2"),
            MallBalanceSnapshotId::new("snapshot-2"),
            changed,
        )
        .unwrap()
        .into_parts()
        .0;
        assert!(!left.same_baseline_as(&right));
    }
}
