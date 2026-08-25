//! `product_revision_media` 商品（SPU）级媒体（数据模型 §6.3）。
//!
//! 媒体角色为轮播图、详情图、附件等受控用途（**主图不在 SPU**，主图归属
//! `sku_revision`，字段名待数据模型补充，见域报告偏差说明）；
//! `(product_revision_id, media_role, sort_order)` 唯一（唯一约束跨行，
//! 属 P3/索引校验）。媒体行随所属修订一并不可变。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, ProductRevisionId, ProductRevisionMediaId};
use crate::validation::normalize_optional_text;

/// 无障碍替代文本最大长度。
const ALT_TEXT_MAX_LEN: usize = 256;

/// 媒体用途（数据模型 §6.3：轮播图、详情图、附件等受控用途；主图不在 SPU）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum MediaRole {
    /// 轮播图。
    Carousel,
    /// 详情图。
    Detail,
    /// 附件。
    Attachment,
}

impl MediaRole {
    /// 返回用途的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Carousel => "轮播图",
            Self::Detail => "详情图",
            Self::Attachment => "附件",
        }
    }

    /// 返回用途的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Carousel => "carousel",
            Self::Detail => "detail",
            Self::Attachment => "attachment",
        }
    }
}

/// 商品修订媒体创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRevisionMediaData {
    /// 所属商品（SPU）修订。
    pub product_revision_id: ProductRevisionId,
    /// 合规媒体文件（`file_asset`，D05）。
    pub file_asset_id: FileAssetId,
    /// 媒体用途。
    pub media_role: MediaRole,
    /// 版本内展示顺序。
    pub sort_order: i32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// 商品修订媒体实体（数据模型 §6.3 关系行表，只用 `BaseModel` 持久化元数据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductRevisionMedia {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属商品（SPU）修订。
    pub product_revision_id: ProductRevisionId,
    /// 合规媒体文件（`file_asset`，D05）。
    pub file_asset_id: FileAssetId,
    /// 媒体用途。
    pub media_role: MediaRole,
    /// 版本内展示顺序。
    pub sort_order: i32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

impl ProductRevisionMedia {
    /// 创建商品修订媒体。
    ///
    /// 完成 alt_text 的可选校验与规范化，并要求 `sort_order` 非负。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductRevisionMediaId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的媒体实体。
    ///
    /// # 错误
    /// 当 alt_text 超长或 sort_order 为负数时返回错误。
    pub fn new(id: ProductRevisionMediaId, data: ProductRevisionMediaData) -> Result<Self> {
        let alt_text = normalize_optional_text(data.alt_text, "替代文本", ALT_TEXT_MAX_LEN)?;
        ensure_non_negative_sort_order(data.sort_order)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            product_revision_id: data.product_revision_id,
            file_asset_id: data.file_asset_id,
            media_role: data.media_role,
            sort_order: data.sort_order,
            alt_text,
        })
    }

    /// 把当前媒体快照复制到新的商品修订。
    ///
    /// # 参数
    /// * `id` - 新媒体行主键
    /// * `product_revision_id` - 目标商品修订 ID
    ///
    /// # 返回
    /// 返回文件、用途、顺序和替代文本保持不变的新媒体行。
    ///
    /// # 错误
    /// 复制后的字段违反媒体实体不变式时返回领域错误。
    pub fn copy_to_revision(
        &self,
        id: ProductRevisionMediaId,
        product_revision_id: ProductRevisionId,
    ) -> Result<Self> {
        Self::new(
            id,
            ProductRevisionMediaData {
                product_revision_id,
                file_asset_id: self.file_asset_id.clone(),
                media_role: self.media_role,
                sort_order: self.sort_order,
                alt_text: self.alt_text.clone(),
            },
        )
    }
}

/// 校验同一商品修订内每种媒体用途的展示顺序唯一。
///
/// # 参数
/// * `rows` - 同一商品修订下待写入的全部媒体行
///
/// # 返回
/// 每个 `(media_role, sort_order)` 组合唯一时返回 `Ok(())`。
///
/// # 错误
/// 同一媒体用途出现重复展示顺序时返回领域错误。
pub fn ensure_unique_media_sort_orders(rows: &[ProductRevisionMedia]) -> Result<()> {
    let mut seen = HashSet::with_capacity(rows.len());
    for row in rows {
        if !seen.insert((row.media_role, row.sort_order)) {
            return Err(Error::from("媒体展示顺序不能重复"));
        }
    }
    Ok(())
}

/// 校验展示顺序为非负整数。
///
/// # 参数
/// * `sort_order` - 展示顺序
///
/// # 返回
/// 非负时返回 `Ok(())`。
///
/// # 错误
/// 为负数时返回错误。
fn ensure_non_negative_sort_order(sort_order: i32) -> Result<()> {
    if sort_order < 0 {
        return Err(Error::from("展示顺序不能为负数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ProductRevisionId;

    fn data() -> ProductRevisionMediaData {
        ProductRevisionMediaData {
            product_revision_id: ProductRevisionId::new("rev-1"),
            file_asset_id: FileAssetId::new("asset-1"),
            media_role: MediaRole::Carousel,
            sort_order: 0,
            alt_text: Some(" 礼盒轮播图 ".to_string()),
        }
    }

    /// happy path：alt_text trim 规范化，媒体角色与归属落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let media = ProductRevisionMedia::new(ProductRevisionMediaId::new("media-1"), data()).unwrap();

        assert_eq!(media.alt_text.as_deref(), Some("礼盒轮播图"));
        assert_eq!(media.media_role, MediaRole::Carousel);
        assert_eq!(media.product_revision_id, ProductRevisionId::new("rev-1"));
        assert_eq!(media.sort_order, 0);
    }

    /// 失败路径：越界（负排序）与超长（alt_text）各一条。
    #[test]
    fn new_rejects_negative_sort_and_overlong_alt_text() {
        let negative_sort = ProductRevisionMediaData {
            sort_order: -1,
            ..data()
        };
        assert!(ProductRevisionMedia::new(ProductRevisionMediaId::new("media-1"), negative_sort).is_err());

        let overlong_alt = ProductRevisionMediaData {
            alt_text: Some("a".repeat(257)),
            ..data()
        };
        assert!(ProductRevisionMedia::new(ProductRevisionMediaId::new("media-1"), overlong_alt).is_err());
    }

    /// 媒体复制保持内容快照并切换所属商品修订。
    #[test]
    fn copy_to_revision_preserves_media_snapshot() {
        let media = ProductRevisionMedia::new(ProductRevisionMediaId::new("media-1"), data()).unwrap();
        let copied = media
            .copy_to_revision(
                ProductRevisionMediaId::new("media-2"),
                ProductRevisionId::new("rev-2"),
            )
            .unwrap();

        assert_eq!(copied.product_revision_id, ProductRevisionId::new("rev-2"));
        assert_eq!(copied.file_asset_id, media.file_asset_id);
        assert_eq!(copied.media_role, media.media_role);
        assert_eq!(copied.sort_order, media.sort_order);
    }

    /// 同一用途的重复顺序被拒绝，不同用途可复用相同顺序。
    #[test]
    fn media_sort_orders_are_unique_per_role() {
        let carousel = ProductRevisionMedia::new(ProductRevisionMediaId::new("media-1"), data()).unwrap();
        let detail = ProductRevisionMedia::new(
            ProductRevisionMediaId::new("media-2"),
            ProductRevisionMediaData {
                media_role: MediaRole::Detail,
                ..data()
            },
        )
        .unwrap();
        assert!(ensure_unique_media_sort_orders(&[carousel.clone(), detail]).is_ok());
        assert!(ensure_unique_media_sort_orders(&[carousel.clone(), carousel]).is_err());
    }

    /// 媒体用途 serde 形态与中文标签。
    #[test]
    fn media_role_exposes_labels_and_codes() {
        assert_eq!(serde_json::to_string(&MediaRole::Detail).unwrap(), "\"detail\"");
        assert_eq!(MediaRole::Carousel.label(), "轮播图");
        assert_eq!(MediaRole::Attachment.label(), "附件");
    }
}
