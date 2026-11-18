//! 准备批次商品池成员。册内客户售价允许发布前调整；重生成沿用该批次，重新取商品池必须创建新批次。

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
    /// 冻结商品池时建立批次成员，供本批次生成和重生成陈列使用。
    /// 返回成员实体。
    ///
    /// 输入约定：
    /// - `id`：成员身份
    /// - `booklet_id`：选品册
    /// - `batch_id`：批次
    /// - `sku`：已规范化快照
    pub fn new(
        id: SalesSelectionPoolMemberId,
        booklet_id: SalesSelectionBookletId,
        batch_id: String,
        sku: SkuSnapshot,
    ) -> Self {
        Self { base: BaseModel::new(id.to_string()), booklet_id, batch_id, sku }
    }
}
