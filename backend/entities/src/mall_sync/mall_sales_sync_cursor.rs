//! `mall_sales_sync_cursor`：商城卡券销售单同步水位游标（数据模型 §6.13）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{MallSalesSyncJobId, SourceSystemId};

/// 商城卡券销售单同步水位游标实体（数据模型 §6.13）。
///
/// 每个来源商城一个当前水位；同一来源商城只允许一个有效增量任务推进水位
/// （并发由 `BaseModel.version` ≡ `lock_version` 乐观锁保证）。水位只前进：
/// 本实体不提供通用 `update`，只提供 [`MallSalesSyncCursor::move_forward`]
/// 单调推进（`high_water_updated_at` 不得回退，任一分页未持久化完成或请求
/// 失败时水位不前移——§8.4 第 2 条，事务职责在 P3）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MallSalesSyncCursor {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 已安全处理的商城更新时间高水位。
    pub high_water_updated_at: Instant,
    /// 最近成功任务。
    pub last_success_job_id: Option<MallSalesSyncJobId>,
}

impl MallSalesSyncCursor {
    /// 创建同步水位游标。
    ///
    /// 期初基线完成后的水位初值必须取基线拉取**开始**时间，禁止取结束时间
    /// （erp-phase-1 §8.4；数据模型 §6.13 同刻多单整批处理完再前移水位）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallSalesSyncCursorId`）
    /// * `source_system_id` - 来源商城
    /// * `initial_water` - 初始高水位（基线拉取开始时间）
    ///
    /// # 返回
    /// 返回新建的水位游标实体。
    pub fn new(
        id: crate::ids::MallSalesSyncCursorId,
        source_system_id: SourceSystemId,
        initial_water: Instant,
    ) -> Self {
        Self {
            base: BaseModel::new(id.to_string()),
            source_system_id,
            high_water_updated_at: initial_water,
            last_success_job_id: None,
        }
    }

    /// 单调前移高水位。
    ///
    /// 只有全部分页安全持久化完成后才允许前移（§8.4 第 2 条）；新水位
    /// 必须不早于当前水位，相等视为无进展的幂等操作。水位按重叠区间前移
    /// （§6.13），重叠窗口语义由接口契约固定，不在实体层计算。
    ///
    /// # 参数
    /// * `new_water` - 新高水位（已安全处理的商城更新时间）
    /// * `success_job_id` - 本次成功的同步任务
    ///
    /// # 返回
    /// 前移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 新水位早于当前水位（回退）时返回错误。
    pub fn move_forward(&mut self, new_water: Instant, success_job_id: MallSalesSyncJobId) -> Result<()> {
        if new_water < self.high_water_updated_at {
            return Err(Error::from("同步水位不得回退"));
        }
        self.high_water_updated_at = new_water;
        self.last_success_job_id = Some(success_job_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::MallSalesSyncCursorId;

    #[test]
    fn new_uses_baseline_start_water() {
        let cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-1"),
            SourceSystemId::new("sys-mall"),
            Instant::from_unix_secs(1_700_000_000),
        );

        assert_eq!(cursor.high_water_updated_at.unix_secs(), 1_700_000_000);
        assert!(cursor.last_success_job_id.is_none());
    }

    #[test]
    fn move_forward_advances_water_and_job() {
        let mut cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-2"),
            SourceSystemId::new("sys-mall"),
            Instant::from_unix_secs(1_700_000_000),
        );

        cursor
            .move_forward(
                Instant::from_unix_secs(1_700_000_300),
                MallSalesSyncJobId::new("j-1"),
            )
            .unwrap();
        assert_eq!(cursor.high_water_updated_at.unix_secs(), 1_700_000_300);
        assert_eq!(cursor.last_success_job_id, Some(MallSalesSyncJobId::new("j-1")));
    }

    #[test]
    fn move_forward_accepts_equal_water_as_idempotent() {
        let mut cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-3"),
            SourceSystemId::new("sys-mall"),
            Instant::from_unix_secs(1_700_000_000),
        );

        cursor
            .move_forward(
                Instant::from_unix_secs(1_700_000_000),
                MallSalesSyncJobId::new("j-1"),
            )
            .unwrap();
        assert_eq!(cursor.high_water_updated_at.unix_secs(), 1_700_000_000);
    }

    #[test]
    fn move_forward_rejects_backwards_water() {
        let mut cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-4"),
            SourceSystemId::new("sys-mall"),
            Instant::from_unix_secs(1_700_000_000),
        );

        assert!(
            cursor
                .move_forward(
                    Instant::from_unix_secs(1_699_900_000),
                    MallSalesSyncJobId::new("j-1")
                )
                .is_err(),
            "水位不得回退"
        );
        assert_eq!(
            cursor.high_water_updated_at.unix_secs(),
            1_700_000_000,
            "失败不改水位"
        );
        assert!(cursor.last_success_job_id.is_none());
    }

    #[test]
    fn bson_roundtrip_preserves_entity() {
        let cursor = MallSalesSyncCursor::new(
            MallSalesSyncCursorId::new("cur-5"),
            SourceSystemId::new("sys-mall"),
            Instant::from_unix_secs(1_700_000_000),
        );
        let roundtrip: MallSalesSyncCursor =
            bson::from_document(bson::to_document(&cursor).unwrap()).unwrap();
        assert_eq!(roundtrip, cursor);
    }
}
