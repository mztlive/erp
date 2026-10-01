//! 库存预占与预占分录的有序批量写入。

use persistence_core::{Executor, Result, mongo_ops};

use super::InventoryRepository;
use crate::{InventoryExt, StockReservation, StockReservationEntry};

impl InventoryRepository<'_> {
    /// 按输入顺序批量保存预占；不另开事务。
    ///
    /// # 参数
    /// * `reservations` - 已构造并校验的预占实体
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 空输入直接成功，否则保存全部预占。
    ///
    /// # 错误
    /// 唯一键冲突或数据库写入失败时返回仓储错误。
    pub async fn create_reservations(
        &self,
        reservations: &[StockReservation],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(&self.db.stock_reservations().collection(), reservations.to_vec(), executor)
            .await
    }

    /// 按输入顺序批量保存预占分录；与预占写入复用同一执行器。
    ///
    /// # 参数
    /// * `entries` - 与预占对应的已校验分录
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 空输入直接成功，否则保存全部分录。
    ///
    /// # 错误
    /// 唯一键冲突或数据库写入失败时返回仓储错误。
    pub async fn create_reservation_entries(
        &self,
        entries: &[StockReservationEntry],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(&self.db.stock_reservation_entries().collection(), entries.to_vec(), executor)
            .await
    }
}
