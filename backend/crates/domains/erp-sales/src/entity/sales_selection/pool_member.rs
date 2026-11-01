//! 准备批次商品池成员。重生成沿用该批次，重新取商品池必须创建新批次。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionPoolMemberId};
use serde::{Deserialize, Serialize};

use super::sku_snapshot::SkuSnapshot;

/// 批次内一个冻结 SKU。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionPoolMember {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 准备批次。
    pub batch_id: String,
    /// 冻结 SKU 事实。
    pub sku: SkuSnapshot,
}

impl SalesSelectionPoolMember {
    /// 创建批次成员。
    ///
    /// # 参数
    /// * `id` - 成员身份
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `sku` - 已规范化快照
    ///
    /// # 返回
    /// 返回成员实体。
    ///
    /// # 错误
    /// 无。
    pub fn new(
        id: SalesSelectionPoolMemberId,
        booklet_id: SalesSelectionBookletId,
        batch_id: String,
        sku: SkuSnapshot,
    ) -> Self {
        Self { base: BaseModel::new(id.to_string()), booklet_id, batch_id, sku }
    }
}
