//! 陈列项：单品 SKU 或套餐。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::image::PackageCoverRef;
use super::sku_snapshot::{package_price, SkuSnapshot};
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionDisplayItemId};
use erp_core::money::Amount;
use erp_core::{Error, Result};

/// 陈列项形态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE", tag = "kind")]
pub enum DisplayKind {
    /// 单品：恰好一个 SKU。
    SingleSku {
        /// SKU 快照。
        sku: SkuSnapshot,
    },
    /// 套餐：多个 SKU 的组合。
    Package {
        /// 所属档位。
        tier_id: String,
        /// 有序成员。
        members: Vec<SkuSnapshot>,
        /// 套餐主图。
        cover: PackageCoverRef,
        /// 套餐售价。
        price: Amount,
    },
}

/// 陈列项实体。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionDisplayItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 准备批次。
    pub batch_id: String,
    /// 是否为当前有效陈列。
    pub effective: bool,
    /// 待发布删除标记；发布时不得包含已删除项。
    pub removed: bool,
    /// 公开页使用的项标识，不暴露管理端 SKU 编号。
    pub public_item_id: String,
    /// 陈列内容。
    pub kind: DisplayKind,
}

impl SalesSelectionDisplayItem {
    /// 创建单品陈列项。
    ///
    /// # 参数
    /// * `id` - 项身份
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `sku` - SKU 快照
    ///
    /// # 返回
    /// 返回有效且未删除的单品陈列。
    ///
    /// # 错误
    /// SKU 快照非法时拒绝。
    pub fn single_sku(
        id: SalesSelectionDisplayItemId,
        booklet_id: SalesSelectionBookletId,
        batch_id: String,
        sku: SkuSnapshot,
    ) -> Result<Self> {
        let sku = sku.normalize()?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            booklet_id,
            batch_id,
            effective: true,
            removed: false,
            public_item_id: id.to_string(),
            kind: DisplayKind::SingleSku { sku },
        })
    }

    /// 创建套餐陈列项。
    ///
    /// # 参数
    /// * `id` - 项身份
    /// * `booklet_id` - 选品册
    /// * `batch_id` - 批次
    /// * `tier_id` - 档位
    /// * `members` - 成员
    /// * `cover` - 套餐主图
    ///
    /// # 返回
    /// 返回售价由后端加总的套餐陈列。
    ///
    /// # 错误
    /// 成员重复、无封面或金额溢出时拒绝。
    pub fn package(
        id: SalesSelectionDisplayItemId,
        booklet_id: SalesSelectionBookletId,
        batch_id: String,
        tier_id: String,
        members: Vec<SkuSnapshot>,
        cover: PackageCoverRef,
    ) -> Result<Self> {
        let members = normalize_package_members(members)?;
        let price = package_price(&members)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            booklet_id,
            batch_id,
            effective: true,
            removed: false,
            public_item_id: id.to_string(),
            kind: DisplayKind::Package {
                tier_id,
                members,
                cover,
                price,
            },
        })
    }

    /// 待发布删除陈列项。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 标记 `removed`。
    ///
    /// # 错误
    /// 已经删除时拒绝。
    pub fn remove(&mut self) -> Result<()> {
        if self.removed {
            return Err(Error::from("陈列项已删除"));
        }
        self.removed = true;
        Ok(())
    }

    /// 返回陈列售价。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 单品为 SKU 销售可见价；套餐为成员之和。
    ///
    /// # 错误
    /// 无。
    pub fn price(&self) -> Amount {
        match &self.kind {
            DisplayKind::SingleSku { sku } => sku.sales_visible_price_gross,
            DisplayKind::Package { price, .. } => *price,
        }
    }

    /// 返回封面资产身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 单品可能无图；套餐必须有主图。
    ///
    /// # 错误
    /// 无。
    pub fn cover_asset_id(&self) -> Option<&str> {
        match &self.kind {
            DisplayKind::SingleSku { sku } => sku.image.as_ref().map(|image| image.file_asset_id.as_str()),
            DisplayKind::Package { cover, .. } => Some(cover.file_asset_id.as_str()),
        }
    }

    /// 返回可授权展示的文件资产身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 封面与成员图资产。
    ///
    /// # 错误
    /// 无。
    pub fn authorized_asset_ids(&self) -> Vec<String> {
        match &self.kind {
            DisplayKind::SingleSku { sku } => sku
                .image
                .as_ref()
                .map(|image| vec![image.file_asset_id.clone()])
                .unwrap_or_default(),
            DisplayKind::Package { members, cover, .. } => {
                let mut ids = vec![cover.file_asset_id.clone()];
                for member in members {
                    if let Some(image) = &member.image {
                        ids.push(image.file_asset_id.clone());
                    }
                }
                ids
            }
        }
    }

    /// 发布复核用的精确 SKU 修订引用。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(sku_id, sku_revision_id)`。
    ///
    /// # 错误
    /// 无。
    pub fn sellable_refs(&self) -> Vec<(String, String)> {
        match &self.kind {
            DisplayKind::SingleSku { sku } => {
                vec![(sku.sku_id.to_string(), sku.sku_revision_id.to_string())]
            }
            DisplayKind::Package { members, .. } => members
                .iter()
                .map(|sku| (sku.sku_id.to_string(), sku.sku_revision_id.to_string()))
                .collect(),
        }
    }

    /// 判断是否为可发布陈列。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 有效且未删除返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_publishable(&self) -> bool {
        self.effective && !self.removed
    }
}

/// 规范化套餐成员：互异、每成员一件、按 sku_id 升序。
///
/// # 参数
/// * `members` - 原始成员
///
/// # 返回
/// 返回规范化成员。
///
/// # 错误
/// 重复 SKU 或成员非法时拒绝。
fn normalize_package_members(members: Vec<SkuSnapshot>) -> Result<Vec<SkuSnapshot>> {
    let mut normalized = Vec::with_capacity(members.len());
    let mut seen = std::collections::BTreeSet::new();
    for member in members {
        let member = member.normalize()?;
        if !seen.insert(member.sku_id_str().to_string()) {
            return Err(Error::from("同一套餐内 SKU 不得重复"));
        }
        normalized.push(member);
    }
    super::sku_snapshot::sort_by_sku_id(&mut normalized);
    Ok(normalized)
}

/// 发布前校验陈列非空且套餐每档至少一套。
///
/// # 参数
/// * `form_is_package` - 是否套餐形态
/// * `tier_ids` - 档位身份
/// * `items` - 当前有效陈列
///
/// # 返回
/// 通过时返回可发布项。
///
/// # 错误
/// 单品不足 1 个或任一套餐档为 0 时拒绝。
pub fn ensure_publishable_display<'a>(
    form_is_package: bool,
    tier_ids: &[String],
    items: &'a [SalesSelectionDisplayItem],
) -> Result<Vec<&'a SalesSelectionDisplayItem>> {
    let publishable: Vec<&SalesSelectionDisplayItem> =
        items.iter().filter(|item| item.is_publishable()).collect();
    if publishable.is_empty() {
        return Err(Error::from("没有可发布的陈列项"));
    }
    if !form_is_package {
        return Ok(publishable);
    }
    for tier_id in tier_ids {
        let count = publishable
            .iter()
            .filter(|item| match &item.kind {
                DisplayKind::Package {
                    tier_id: item_tier, ..
                } => item_tier == tier_id,
                DisplayKind::SingleSku { .. } => false,
            })
            .count();
        if count == 0 {
            return Err(Error::from("每个档位至少保留 1 个套餐才可发布"));
        }
    }
    Ok(publishable)
}
