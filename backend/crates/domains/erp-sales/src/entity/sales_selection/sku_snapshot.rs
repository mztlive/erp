//! 准备批次冻结的 SKU 资料。不写入商品池，也不创建正式 SKU。

use erp_core::ids::{ProductId, SkuId, SkuRevisionId};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

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

/// 准备时冻结的公司含税参考价；缺失价格不推算。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionReferencePrices {
    /// 一件代发价。
    pub dropship_price_gross: Option<Amount>,
    /// 集采价。
    pub bulk_price_gross: Option<Amount>,
    /// 市场价。
    pub market_price: Option<Amount>,
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
    /// 准备时参考价；旧快照缺失时仅允许按精确修订读取。
    #[serde(default)]
    pub reference_prices: Option<SelectionReferencePrices>,
    /// 本册客户含税售价；准备时取商品销售可见价，待发布允许独立改价。
    pub sales_visible_price_gross: Amount,
}

impl SkuSnapshot {
    /// 规范化并校验一份 SKU 快照。
    /// 返回身份、名称、单位与价格均有效的快照。
    /// 名称、单位或商品类型为空或超长，或销售可见含税价不大于 0 时拒绝。
    ///
    /// 输入约定：
    /// - `self`：待规范化快照
    pub fn normalize(self) -> Result<Self> {
        let name = normalize_required_text(self.name, "SKU 名称不能为空", 256, "SKU 名称过长")?;
        let unit = normalize_required_text(self.unit, "SKU 单位不能为空", 64, "SKU 单位过长")?;
        let product_kind =
            normalize_required_text(self.product_kind, "商品类型不能为空", 32, "商品类型过长")?;
        if self.sales_visible_price_gross <= Amount::zero() {
            return Err(Error::from("销售可见含税价必须大于 0"));
        }
        let category_id =
            self.category_id.map(|value| value.trim().to_string()).filter(|value| !value.is_empty());

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
            reference_prices: self.reference_prices,
            sales_visible_price_gross: self.sales_visible_price_gross,
        })
    }

    /// 返回稳定 SKU 身份字符串。
    /// 返回 `sku_id` 文本。
    pub fn sku_id_str(&self) -> &str {
        self.sku_id.as_ref()
    }

    /// 构造供生成端口使用的图片 URL 占位。
    /// 结果：有图时返回受控 `asset:` URL；无图返回 `None`。
    pub fn image_port_url(&self) -> Option<String> {
        self.image.as_ref().map(|image| format!("asset:{}", image.file_asset_id))
    }
}

/// 按稳定 SKU 身份升序排列快照。
/// 结果：原地按 `sku_id` 升序排列。
///
/// 输入约定：
/// - `items`：待排序快照
pub fn sort_by_sku_id(items: &mut [SkuSnapshot]) {
    items.sort_by(|left, right| left.sku_id_str().cmp(right.sku_id_str()));
}

/// 由成员快照计算套餐售价。
/// 返回销售可见含税价之和。
/// 成员为空或金额溢出时拒绝。
///
/// 输入约定：
/// - `members`：有序成员
pub fn package_price(members: &[SkuSnapshot]) -> Result<Amount> {
    if members.is_empty() {
        return Err(Error::from("套餐必须包含成员 SKU"));
    }
    super::pricing::try_sum(members.iter().map(|item| item.sales_visible_price_gross))
}
