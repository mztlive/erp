//! 集采入仓配送判定所需的启用仓库所在地区。

use erp_core::region::RegionCode;
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::{Executor, Result};

/// 一个启用仓库的当前所在地区。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarehouseRegionFact {
    /// 仓库主键。
    pub warehouse_id: String,
    /// 当前修订的所在地区。
    pub region_code: RegionCode,
}

/// 批量读取全部启用仓库的当前所在地区。
///
/// # 参数
/// * `db` - 数据库
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回按仓库主键排序的启用仓库地区；缺少当前修订或未设置地区的仓库不返回。
///
/// # 错误
/// MongoDB 查询失败时返回错误。
pub async fn active_warehouse_regions(
    db: &Database,
    executor: &mut dyn Executor,
) -> Result<Vec<WarehouseRegionFact>> {
    let warehouses = db.warehouses().list_all(executor).await?;
    let mut current = warehouses
        .iter()
        .filter(|warehouse| warehouse.is_active())
        .filter_map(|warehouse| {
            warehouse.stable.current_revision_id.clone().map(|revision| (warehouse.base.id.clone(), revision))
        })
        .collect::<Vec<_>>();
    current.sort();
    let revision_ids = current.iter().map(|(_, revision)| revision.clone()).collect::<Vec<_>>();
    let revisions = db.warehouse_revisions().list_active_by_ids(&revision_ids, executor).await?;
    Ok(current
        .into_iter()
        .filter_map(|(warehouse_id, revision_id)| {
            revisions
                .iter()
                .find(|revision| revision.base.id == revision_id)
                .and_then(|revision| revision.region_code.clone())
                .map(|region_code| WarehouseRegionFact { warehouse_id, region_code })
        })
        .collect())
}
