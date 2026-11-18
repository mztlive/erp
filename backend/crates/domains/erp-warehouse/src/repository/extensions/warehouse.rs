//! 域 D11 `warehouse` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as WarehouseExt>::WAREHOUSES` 等值。

use mongodb::Database;

use super::super::warehouse::{
    WarehouseDomainRepository, WarehouseFilter, WarehouseRevisionFilter, WarehouseSkuPolicyFilter,
};
use crate::repository::owned::{
    WarehouseRepository, WarehouseRevisionRepository, WarehouseSkuPolicyRepository,
};

/// 域 D11 仓储访问器。
pub trait WarehouseExt {
    /// `warehouse` 集合名。
    const WAREHOUSES: &'static str = "warehouses";
    /// `warehouse_revision` 集合名。
    const WAREHOUSE_REVISIONS: &'static str = "warehouse_revisions";
    /// `warehouse_sku_policy` 集合名。
    const WAREHOUSE_SKU_POLICIES: &'static str = "warehouse_sku_policies";

    /// 仓库列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseFilter;

    /// 仓库修订列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseRevisionFilter;

    /// 仓库-SKU 预警策略列表筛选条件类型（定义见 `repository::warehouse`）。
    type WarehouseSkuPolicyFilter;

    /// 获取仓库稳定身份的集合仓储，供资料、经办配置和启停状态的读写复用。
    fn warehouses(&self) -> WarehouseRepository<'_>;

    /// 获取仓库修订仓储，供追加资料快照并读取历史修订。
    fn warehouse_revisions(&self) -> WarehouseRevisionRepository<'_>;

    /// 获取仓库 SKU 策略仓储，供按仓库和 SKU 查询或更新预警阈值。
    fn warehouse_sku_policies(&self) -> WarehouseSkuPolicyRepository<'_>;

    /// 获取负责多集合仓库写入的领域仓储，原子性由调用方事务保证。
    fn warehouse(&self) -> WarehouseDomainRepository<'_>;
}

impl WarehouseExt for Database {
    type WarehouseFilter = WarehouseFilter;
    type WarehouseRevisionFilter = WarehouseRevisionFilter;
    type WarehouseSkuPolicyFilter = WarehouseSkuPolicyFilter;

    // 将稳定仓库访问绑定到本域声明的集合名，避免调用方自行选表。
    fn warehouses(&self) -> WarehouseRepository<'_> {
        WarehouseRepository::new(self, Self::WAREHOUSES)
    }

    // 将修订访问绑定到本域声明的修订集合，供首修订、后续追加和历史查询使用。
    fn warehouse_revisions(&self) -> WarehouseRevisionRepository<'_> {
        WarehouseRevisionRepository::new(self, Self::WAREHOUSE_REVISIONS)
    }

    // 将 SKU 策略访问绑定到本域预警策略集合，避免与库存事实混用。
    fn warehouse_sku_policies(&self) -> WarehouseSkuPolicyRepository<'_> {
        WarehouseSkuPolicyRepository::new(self, Self::WAREHOUSE_SKU_POLICIES)
    }

    // 用当前数据库构造仓库领域仓储，供稳定身份和修订的跨集合步骤共享连接。
    fn warehouse(&self) -> WarehouseDomainRepository<'_> {
        WarehouseDomainRepository::new(self)
    }
}
