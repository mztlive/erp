//! 域 D28 `card_instance` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 两侧统一取
//! `<mongodb::Database as CardInstanceExt>::MALL_CARD_INSTANCES` 等值，禁止字面量重复。
//! 事实/纠错/快照类集合（`mall_balance_snapshot`、`mall_card_instance_correction`）
//! 是不可变追加事实（§4.5），不暴露通用 `Repository`（其带软删除方法），
//! 只暴露只读追加仓储。

use entities::card_instance::{MallCardInstance, MallConsumptionCutover};
use mongodb::Database;

use super::super::card_instance::{
    BalanceSnapshotRepository, CardInstanceCorrectionRepository, CardInstanceRepository,
    MallCardInstanceFilter, MallConsumptionCutoverFilter,
};
use crate::Repository;

/// 域 D28 仓储访问器。
pub trait CardInstanceExt {
    /// `mall_consumption_cutover` 集合名。
    const MALL_CONSUMPTION_CUTOVERS: &'static str = "mall_consumption_cutovers";
    /// `mall_card_instance` 集合名。
    const MALL_CARD_INSTANCES: &'static str = "mall_card_instances";
    /// `mall_card_instance_correction` 集合名。
    const MALL_CARD_INSTANCE_CORRECTIONS: &'static str = "mall_card_instance_corrections";
    /// `mall_balance_snapshot` 集合名。
    const MALL_BALANCE_SNAPSHOTS: &'static str = "mall_balance_snapshots";

    /// 切换记录列表筛选条件类型（定义见 `repository::card_instance`）。
    type MallConsumptionCutoverFilter;

    /// 卡实例列表筛选条件类型（定义见 `repository::card_instance`）。
    type MallCardInstanceFilter;

    /// 获取 `mall_consumption_cutover` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::card_instance::MallConsumptionCutover>`。
    fn mall_consumption_cutovers(&self) -> Repository<'_, MallConsumptionCutover>;

    /// 获取 `mall_card_instance` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::card_instance::MallCardInstance>`。
    fn mall_card_instances(&self) -> Repository<'_, MallCardInstance>;

    /// 获取 `mall_balance_snapshot` 集合的只读追加仓储。
    ///
    /// 余额快照是不可变事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `BalanceSnapshotRepository` 实例。
    fn balance_snapshots(&self) -> BalanceSnapshotRepository<'_>;

    /// 获取 `mall_card_instance_correction` 集合的只读追加仓储。
    ///
    /// 纠错是不可变追加事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `CardInstanceCorrectionRepository` 实例。
    fn card_instance_corrections(&self) -> CardInstanceCorrectionRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `CardInstanceRepository` 实例。
    fn card_instance(&self) -> CardInstanceRepository<'_>;
}

impl CardInstanceExt for Database {
    type MallConsumptionCutoverFilter = MallConsumptionCutoverFilter;
    type MallCardInstanceFilter = MallCardInstanceFilter;

    fn mall_consumption_cutovers(&self) -> Repository<'_, MallConsumptionCutover> {
        Repository::new(self, Self::MALL_CONSUMPTION_CUTOVERS)
    }

    fn mall_card_instances(&self) -> Repository<'_, MallCardInstance> {
        Repository::new(self, Self::MALL_CARD_INSTANCES)
    }

    fn balance_snapshots(&self) -> BalanceSnapshotRepository<'_> {
        BalanceSnapshotRepository::new(self)
    }

    fn card_instance_corrections(&self) -> CardInstanceCorrectionRepository<'_> {
        CardInstanceCorrectionRepository::new(self)
    }

    fn card_instance(&self) -> CardInstanceRepository<'_> {
        CardInstanceRepository::new(self)
    }
}
