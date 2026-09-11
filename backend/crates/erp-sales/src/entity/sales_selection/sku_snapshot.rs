//! 准备批次冻结的 SKU 资料。不写入商品池，也不创建正式 SKU。

use serde::{Deserialize, Serialize};

use erp_core::ids::{ProductId, SkuId, SkuRevisionId};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};

/// 结构化规格属性快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecificationAttributeSnapshot {
    /// 规格属性名。
    pub name: String,
    /// 规格属性值。
    pub value: String,
}

/// SKU 主图快照引用。无主图时为 `None`，单品仍可进入陈列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageAssetSnapshot {
    /// 文件资产身份。
    pub file_asset_id: String,
    /// 内容校验信息。
    pub content_checksum: String,
    /// 快照对象键。
    pub storage_object_key: String,
}

/// 准备批次内一份 SKU 事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkuSnapshot {
    /// 稳定 SKU 身份。
    pub sku_id: SkuId,
    /// SKU 修订。
    pub sku_revision_id: SkuRevisionId,
    /// 所属 SPU。
    pub product_id: ProductId,
    /// 商品类型稳定代码。
    pub product_kind: String,
    /// 分类身份；未分类为 `None`，不得虚构。
    pub category_id: Option<String>,
    /// 名称。
    pub name: String,
    /// 结构化规格属性。
    pub specification_attributes: Vec<SpecificationAttributeSnapshot>,
    /// 单位名称或编码，供展示。
    pub unit: String,
    /// 图片资产引用。
    pub image: Option<ImageAssetSnapshot>,
    /// 销售可见含税价。
    pub sales_visible_price_gross: Amount,
}

impl SkuSnapshot {
    /// 规范化并校验一份 SKU 快照。
    ///
    /// # 参数
    /// * `self` - 待规范化快照
    ///
    /// # 返回
    /// 返回身份、名称、单位与价格均有效的快照。
    ///
    /// # 错误
    /// 必填字段为空时拒绝。
    pub fn normalize(self) -> Result<Self> {
        let name = normalize_required_text(self.name, "SKU 名称不能为空", 256, "SKU 名称过长")?;
        let unit = normalize_required_text(self.unit, "SKU 单位不能为空", 64, "SKU 单位过长")?;
        let product_kind =
            normalize_required_text(self.product_kind, "商品类型不能为空", 32, "商品类型过长")?;
        if self.sales_visible_price_gross <= Amount::zero() {
            return Err(Error::from("销售可见含税价必须大于 0"));
        }
        let category_id = self
            .category_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        Ok(Self {
            sku_id: self.sku_id,
            sku_revision_id: self.sku_revision_id,
            product_id: self.product_id,
            product_kind,
            category_id,
            name,
            specification_attributes: self.specification_attributes,
            unit,
            image: self.image,
            sales_visible_price_gross: self.sales_visible_price_gross,
        })
    }

    /// 返回稳定 SKU 身份字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `sku_id` 文本。
    ///
    /// # 错误
    /// 无。
    pub fn sku_id_str(&self) -> &str {
        self.sku_id.as_ref()
    }

    /// 构造供生成端口使用的图片 URL 占位。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 有图时返回受控 `asset:` URL；无图返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn image_port_url(&self) -> Option<String> {
        self.image
            .as_ref()
            .map(|image| format!("asset:{}", image.file_asset_id))
    }
}

/// 按稳定 SKU 身份升序排列快照。
///
/// # 参数
/// * `items` - 待排序快照
///
/// # 返回
/// 原地按 `sku_id` 升序排列。
///
/// # 错误
/// 无。
pub fn sort_by_sku_id(items: &mut [SkuSnapshot]) {
    items.sort_by(|left, right| left.sku_id_str().cmp(right.sku_id_str()));
}

/// 由成员快照计算套餐售价。
///
/// # 参数
/// * `members` - 有序成员
///
/// # 返回
/// 返回销售可见含税价之和。
///
/// # 错误
/// 成员为空或金额溢出时拒绝。
pub fn package_price(members: &[SkuSnapshot]) -> Result<Amount> {
    if members.is_empty() {
        return Err(Error::from("套餐必须包含成员 SKU"));
    }
    super::pricing::try_sum(members.iter().map(|item| item.sales_visible_price_gross))
}
