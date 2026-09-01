//! `product_publication_revision` 与 `product_publication_revision_media`：
//! 不可变商城发布版本及其受控媒体行（数据模型 §6.15，页面 W22）。
//!
//! 发布版本组合 [`crate::common::revision::RevisionBase`]，内联结构化快照
//! （§4.4）：商城展示名称/规格/销售说明、最小购买量、含税销售价与销项税率、
//! 计量单位、可销售区域、上架状态、商品级能力与生效区间。快照字段由 P3 在
//! 形成版本时填充，本层只定义并校验。版本一经形成不可修改（§4.5）。

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::content_identity::{PublicationContentFingerprint, PublicationContentSnapshot};
use crate::common::revision::RevisionBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, ProductCategoryId, ProductPublicationId, ProductPublicationRevisionId,
    ProductPublicationRevisionMediaId, SkuRevisionId, SupplierOfferingRevisionId,
};
use crate::money::{Amount, Quantity, Rate};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 发布名称最大长度。
const NAME_MAX_LEN: usize = 256;
/// 规格快照最大长度。
const SPECIFICATION_MAX_LEN: usize = 1024;
/// 销售说明最大长度。
const DESCRIPTION_MAX_LEN: usize = 4096;
/// 计量单位代码最大长度。
const BASE_UNIT_CODE_MAX_LEN: usize = 32;
/// 可销售区域最大长度。
const SALES_REGION_MAX_LEN: usize = 256;
/// 媒体替代文本最大长度。
const ALT_TEXT_MAX_LEN: usize = 256;

/// 上架状态（数据模型 §6.15：上架、下架、暂停下单；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaleStatus {
    /// 上架。
    OnSale,
    /// 下架。
    OffSale,
    /// 暂停下单。
    PauseOrder,
}

impl SaleStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::OnSale => "上架",
            Self::OffSale => "下架",
            Self::PauseOrder => "暂停下单",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OnSale => "on_sale",
            Self::OffSale => "off_sale",
            Self::PauseOrder => "pause_order",
        }
    }

    /// 判断发布修订是否允许商城下单。
    ///
    /// # 返回
    /// 状态为上架时返回 `true`。
    pub fn is_on_sale(self) -> bool {
        self == Self::OnSale
    }
}

/// 商品级能力（数据模型 §6.15：商品级取消、退款、物流等能力；固定枚举，
/// 与连接能力声明 `SupplierApiCapabilityCode` 是两种不同的事实）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductCapability {
    /// 取消。
    Cancel,
    /// 退款。
    Refund,
    /// 物流查询。
    Logistics,
}

impl ProductCapability {
    /// 返回能力的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cancel => "取消",
            Self::Refund => "退款",
            Self::Logistics => "物流",
        }
    }

    /// 返回能力的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Refund => "refund",
            Self::Logistics => "logistics",
        }
    }
}

/// 媒体角色（数据模型 §6.15：主图、轮播图或详情图；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRole {
    /// 主图（同一发布版本只能有一张，跨行约束在 P3，§6.15）。
    Main,
    /// 轮播图。
    Carousel,
    /// 详情图。
    Detail,
}

impl MediaRole {
    /// 返回角色的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Main => "主图",
            Self::Carousel => "轮播图",
            Self::Detail => "详情图",
        }
    }

    /// 返回角色的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Carousel => "carousel",
            Self::Detail => "detail",
        }
    }

    /// 校验发布媒体集合至少包含一张主图。
    ///
    /// # 参数
    /// * `roles` - 待提交或复制的媒体角色集合
    ///
    /// # 返回
    /// 至少存在一张主图时返回 `Ok(())`。
    ///
    /// # 错误
    /// 集合为空或不包含主图时返回错误。
    pub fn ensure_main_present(roles: impl IntoIterator<Item = Self>) -> Result<()> {
        if roles.into_iter().any(|role| role == Self::Main) {
            return Ok(());
        }
        Err(Error::from("提交发布必须至少包含一张主图"))
    }
}

/// 发布版本创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionData {
    /// 稳定发布。
    pub product_publication_id: ProductPublicationId,
    /// 发布的商品版本。
    pub sku_revision_id: SkuRevisionId,
    /// 本发布版本唯一固定的供给修订（§6.15「每个发布修订恰好绑定一个供给修订」）。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 商城发布类目（提交发布的必填快照，§6.15）。
    pub category_id: ProductCategoryId,
    /// 商城展示名称快照。
    pub name: String,
    /// 规格快照。
    pub specification: Option<String>,
    /// 商城展示销售说明快照（提交发布的必填快照，§6.15）。
    pub sales_description: String,
    /// 商城端最小购买量，按 `base_unit_code`（必须大于零，§6.15）。
    pub minimum_purchase_quantity: Quantity,
    /// 含税销售价（与供货价分开，§6.15）。
    pub sales_price_gross: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 计量单位代码。
    pub base_unit_code: String,
    /// 可销售区域快照。
    pub sales_region: Option<String>,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 商品级能力清单。
    pub product_capabilities: Vec<ProductCapability>,
    /// 生效区间开始。
    pub valid_from: Instant,
    /// 生效区间结束；必须晚于 `valid_from`。
    pub valid_to: Option<Instant>,
}

/// 发布版本实体（不可变版本，数据模型 §6.15）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductPublicationRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 稳定发布。
    pub product_publication_id: ProductPublicationId,
    /// 发布的商品版本。
    pub sku_revision_id: SkuRevisionId,
    /// 本发布版本唯一固定的供给修订。
    pub supplier_offering_revision_id: SupplierOfferingRevisionId,
    /// 商城发布类目。
    pub category_id: ProductCategoryId,
    /// 商城展示名称快照。
    pub name: String,
    /// 规格快照。
    pub specification: Option<String>,
    /// 商城展示销售说明快照。
    pub sales_description: String,
    /// 商城端最小购买量，按 `base_unit_code`。
    pub minimum_purchase_quantity: Quantity,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 销项税率。
    pub sales_tax_rate: Rate,
    /// 计量单位代码。
    pub base_unit_code: String,
    /// 可销售区域快照。
    pub sales_region: Option<String>,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 商品级能力清单。
    pub product_capabilities: Vec<ProductCapability>,
    /// 生效区间开始。
    pub valid_from: Instant,
    /// 生效区间结束。
    pub valid_to: Option<Instant>,
    /// 发布内容指纹。
    pub content_hash: String,
}

impl ProductPublicationRevision {
    /// 创建发布版本。
    ///
    /// 完成展示名称/销售说明/计量单位的校验与规范化，校验销售不变式，对商品
    /// 级能力清单去重，并一次派生真实内容指纹。禁止任何占位指纹。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductPublicationRevisionId`）
    /// * `revision_no` - 修订序号（同一发布内从 1 递增）
    /// * `data` - 创建数据；不含内容指纹
    ///
    /// # 返回
    /// 返回新建的发布版本实体。
    ///
    /// # 错误
    /// 当必填快照为空或超长、最小购买量不为正、销售价或税率为负、
    /// 生效区间倒挂时返回错误。
    pub fn new(
        id: ProductPublicationRevisionId,
        revision_no: u32,
        data: ProductPublicationRevisionData,
    ) -> Result<Self> {
        let texts = normalize_revision_texts(
            data.name.clone(),
            data.specification.clone(),
            data.sales_description.clone(),
            data.base_unit_code.clone(),
            data.sales_region.clone(),
        )?;
        validate_sales_invariants(
            data.minimum_purchase_quantity,
            data.sales_price_gross,
            data.sales_tax_rate,
            data.valid_from,
            data.valid_to,
        )?;
        let product_capabilities = dedup_capabilities(data.product_capabilities.clone());
        let content_hash = content_hash_from(&texts, &data, &product_capabilities);
        Ok(assemble_revision(
            id,
            revision_no,
            data,
            texts,
            product_capabilities,
            content_hash,
        ))
    }

    /// 由仓储返回的最新修订号计算下一发布修订序号。
    ///
    /// # 参数
    /// * `latest` - 当前发布的最大修订号；没有历史时为 `None`
    ///
    /// # 返回
    /// 无历史返回 `1`；否则返回最大序号加一。
    ///
    /// # 错误
    /// 当前最大序号已为 `u32::MAX` 时返回错误，禁止 panic 或回绕。
    ///
    /// # 关键业务约束
    /// 普通创建与事务内安全暂停必须复用本方法；并发唯一性仍由
    /// `(product_publication_id, revision_no)` 唯一索引保证。
    pub fn next_revision_no(latest: Option<u32>) -> Result<u32> {
        RevisionBase::next_revision_no(latest)
    }

    /// 复制当前商城内容形成不可变安全暂停修订。
    ///
    /// # 参数
    /// * `id` - 新暂停修订 ID
    /// * `revision_no` - 同一发布内的新修订序号
    /// * `committed_at` - 安全暂停统一业务时间
    ///
    /// # 返回
    /// 返回内容快照一致、状态为暂停下单且重新开始生效区间的新修订。
    ///
    /// # 错误
    /// 原修订快照违反当前发布校验规则时返回错误。
    pub fn safety_pause_copy(
        &self,
        id: ProductPublicationRevisionId,
        revision_no: u32,
        committed_at: Instant,
    ) -> Result<Self> {
        Self::new(
            id,
            revision_no,
            ProductPublicationRevisionData {
                product_publication_id: self.product_publication_id.clone(),
                sku_revision_id: self.sku_revision_id.clone(),
                supplier_offering_revision_id: self.supplier_offering_revision_id.clone(),
                category_id: self.category_id.clone(),
                name: self.name.clone(),
                specification: self.specification.clone(),
                sales_description: self.sales_description.clone(),
                minimum_purchase_quantity: self.minimum_purchase_quantity,
                sales_price_gross: self.sales_price_gross,
                sales_tax_rate: self.sales_tax_rate,
                base_unit_code: self.base_unit_code.clone(),
                sales_region: self.sales_region.clone(),
                sale_status: SaleStatus::PauseOrder,
                product_capabilities: self.product_capabilities.clone(),
                valid_from: committed_at,
                valid_to: None,
            },
        )
    }
}

/// 发布修订已规范化的展示文本。
struct RevisionTexts {
    /// 商城展示名称。
    name: String,
    /// 规格快照。
    specification: Option<String>,
    /// 销售说明。
    sales_description: String,
    /// 计量单位代码。
    base_unit_code: String,
    /// 可销售区域。
    sales_region: Option<String>,
}

/// 规范化发布修订的必填与可选展示文本。
///
/// # 参数
/// * `name` - 商城展示名称
/// * `specification` - 规格快照
/// * `sales_description` - 销售说明
/// * `base_unit_code` - 计量单位代码
/// * `sales_region` - 可销售区域
///
/// # 返回
/// 返回去首尾空白后的展示文本。
///
/// # 错误
/// 必填文本为空或任一文本超长时返回错误。
fn normalize_revision_texts(
    name: String,
    specification: Option<String>,
    sales_description: String,
    base_unit_code: String,
    sales_region: Option<String>,
) -> Result<RevisionTexts> {
    Ok(RevisionTexts {
        name: normalize_required_text(name, "发布名称不能为空", NAME_MAX_LEN, "发布名称过长")?,
        specification: normalize_optional_text(specification, "规格快照", SPECIFICATION_MAX_LEN)?,
        sales_description: normalize_required_text(
            sales_description,
            "销售说明不能为空",
            DESCRIPTION_MAX_LEN,
            "销售说明过长",
        )?,
        base_unit_code: normalize_required_text(
            base_unit_code,
            "计量单位不能为空",
            BASE_UNIT_CODE_MAX_LEN,
            "计量单位过长",
        )?,
        sales_region: normalize_optional_text(sales_region, "可销售区域", SALES_REGION_MAX_LEN)?,
    })
}

/// 由规范化快照一次派生真实内容指纹。
///
/// # 参数
/// * `texts` - 已规范化展示文本
/// * `data` - 原始创建数据中的销售、状态与时间字段
/// * `product_capabilities` - 已去重的能力清单
///
/// # 返回
/// 返回 v1 FNV 十六进制指纹，不含占位值。
///
/// # 错误
/// 无。指纹派生是确定性纯函数。
fn content_hash_from(
    texts: &RevisionTexts,
    data: &ProductPublicationRevisionData,
    product_capabilities: &[ProductCapability],
) -> String {
    PublicationContentFingerprint::from_snapshot(&PublicationContentSnapshot {
        name: &texts.name,
        specification: texts.specification.as_deref(),
        sales_description: &texts.sales_description,
        minimum_purchase_quantity: data.minimum_purchase_quantity,
        sales_price_gross: data.sales_price_gross,
        sales_tax_rate: data.sales_tax_rate,
        base_unit_code: &texts.base_unit_code,
        sales_region: texts.sales_region.as_deref(),
        sale_status: data.sale_status,
        product_capabilities,
        valid_from: data.valid_from,
        valid_to: data.valid_to,
    })
    .into_wire()
}

/// 组装已通过校验的发布修订实体。
///
/// # 参数
/// * `id` - 实体主键
/// * `revision_no` - 修订序号
/// * `data` - 已通过销售不变式校验的创建数据
/// * `texts` - 已规范化展示文本
/// * `product_capabilities` - 已去重能力清单
/// * `content_hash` - 一次派生的真实指纹
///
/// # 返回
/// 返回不可变发布修订。
///
/// # 错误
/// 无。调用前必须已完成校验。
fn assemble_revision(
    id: ProductPublicationRevisionId,
    revision_no: u32,
    data: ProductPublicationRevisionData,
    texts: RevisionTexts,
    product_capabilities: Vec<ProductCapability>,
    content_hash: String,
) -> ProductPublicationRevision {
    ProductPublicationRevision {
        base: BaseModel::new(id.to_string()),
        revision: RevisionBase::new(revision_no),
        product_publication_id: data.product_publication_id,
        sku_revision_id: data.sku_revision_id,
        supplier_offering_revision_id: data.supplier_offering_revision_id,
        category_id: data.category_id,
        name: texts.name,
        specification: texts.specification,
        sales_description: texts.sales_description,
        minimum_purchase_quantity: data.minimum_purchase_quantity,
        sales_price_gross: data.sales_price_gross,
        sales_tax_rate: data.sales_tax_rate,
        base_unit_code: texts.base_unit_code,
        sales_region: texts.sales_region,
        sale_status: data.sale_status,
        product_capabilities,
        valid_from: data.valid_from,
        valid_to: data.valid_to,
        content_hash,
    }
}

/// 校验发布销售不变式。
///
/// # 参数
/// * `minimum_purchase_quantity` - 最小购买量
/// * `sales_price_gross` - 含税销售价
/// * `sales_tax_rate` - 销项税率
/// * `valid_from` - 生效区间开始
/// * `valid_to` - 生效区间结束
///
/// # 错误
/// 当最小购买量不为正、销售价或税率为负、生效区间倒挂时返回错误。
fn validate_sales_invariants(
    minimum_purchase_quantity: Quantity,
    sales_price_gross: Amount,
    sales_tax_rate: Rate,
    valid_from: Instant,
    valid_to: Option<Instant>,
) -> Result<()> {
    if minimum_purchase_quantity.to_decimal() <= Decimal::ZERO {
        return Err(Error::from("最小购买量必须大于零"));
    }
    if sales_price_gross.to_decimal() < Decimal::ZERO {
        return Err(Error::from("含税销售价不能为负"));
    }
    if sales_tax_rate.to_decimal() < Decimal::ZERO {
        return Err(Error::from("销项税率不能为负"));
    }
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("失效时间必须晚于生效时间"));
        }
    }
    Ok(())
}

/// 对商品级能力清单去重（保留首次出现顺序）。
///
/// # 参数
/// * `capabilities` - 原始能力清单
///
/// # 返回
/// 返回去重后的能力清单。
fn dedup_capabilities(capabilities: Vec<ProductCapability>) -> Vec<ProductCapability> {
    let mut seen = Vec::with_capacity(capabilities.len());
    for capability in capabilities {
        if !seen.contains(&capability) {
            seen.push(capability);
        }
    }
    seen
}

/// 发布版本媒体创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionMediaData {
    /// 受控文件资产。
    pub file_asset_id: FileAssetId,
    /// 媒体角色。
    pub media_role: MediaRole,
    /// 同角色内展示顺序。
    pub sort_no: u32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

/// 发布版本媒体实体（数据模型 §6.15）。
///
/// 媒体引用发布后不可原位替换，变更图片必须形成新发布修订（跨行约束在 P3，
/// §6.15）；`(product_publication_revision_id, media_role, sort_no)` 唯一由唯一
/// 索引保证。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ProductPublicationRevisionMedia {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属商城发布版本。
    pub product_publication_revision_id: ProductPublicationRevisionId,
    /// 受控文件资产。
    pub file_asset_id: FileAssetId,
    /// 媒体角色。
    pub media_role: MediaRole,
    /// 同角色内展示顺序。
    pub sort_no: u32,
    /// 无障碍替代文本。
    pub alt_text: Option<String>,
}

impl ProductPublicationRevisionMedia {
    /// 创建发布版本媒体。
    ///
    /// 完成 alt_text 的校验与规范化（去首尾空白、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductPublicationRevisionMediaId`）
    /// * `product_publication_revision_id` - 所属商城发布版本
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的媒体实体。
    ///
    /// # 错误
    /// 当替代文本超长时返回错误。
    pub fn new(
        id: ProductPublicationRevisionMediaId,
        product_publication_revision_id: ProductPublicationRevisionId,
        data: ProductPublicationRevisionMediaData,
    ) -> Result<Self> {
        let alt_text = normalize_optional_text(data.alt_text, "替代文本", ALT_TEXT_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            product_publication_revision_id,
            file_asset_id: data.file_asset_id,
            media_role: data.media_role,
            sort_no: data.sort_no,
            alt_text,
        })
    }

    /// 复制媒体快照并绑定到新的发布修订。
    ///
    /// # 参数
    /// * `id` - 新媒体行 ID
    /// * `revision_id` - 新发布修订 ID
    ///
    /// # 返回
    /// 返回文件、角色、顺序和替代文本不变的新媒体行。
    ///
    /// # 错误
    /// 原替代文本违反当前媒体校验规则时返回错误。
    pub fn copy_to_revision(
        &self,
        id: ProductPublicationRevisionMediaId,
        revision_id: ProductPublicationRevisionId,
    ) -> Result<Self> {
        Self::new(
            id,
            revision_id,
            ProductPublicationRevisionMediaData {
                file_asset_id: self.file_asset_id.clone(),
                media_role: self.media_role,
                sort_no: self.sort_no,
                alt_text: self.alt_text.clone(),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MediaRole, ProductCapability, ProductPublicationRevision, ProductPublicationRevisionData,
        ProductPublicationRevisionMedia, ProductPublicationRevisionMediaData, SaleStatus,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        FileAssetId, ProductCategoryId, ProductPublicationId, ProductPublicationRevisionId,
        ProductPublicationRevisionMediaId, SkuRevisionId, SupplierOfferingRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate};
    use std::str::FromStr;

    fn revision_data() -> ProductPublicationRevisionData {
        ProductPublicationRevisionData {
            product_publication_id: ProductPublicationId::new("pub-1"),
            sku_revision_id: SkuRevisionId::new("sku-rev-1"),
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("offer-rev-1"),
            category_id: ProductCategoryId::new("cat-1"),
            name: " 福利商城卡 ".to_string(),
            specification: Some(" 100 元面额 ".to_string()),
            sales_description: " 员工福利采购 ".to_string(),
            minimum_purchase_quantity: Quantity::from_str("1.000000").unwrap(),
            sales_price_gross: Amount::from_str("100.00").unwrap(),
            sales_tax_rate: Rate::from_str("0.130000").unwrap(),
            base_unit_code: " 张 ".to_string(),
            sales_region: Some(" 全国 ".to_string()),
            sale_status: SaleStatus::OnSale,
            product_capabilities: vec![
                ProductCapability::Cancel,
                ProductCapability::Refund,
                ProductCapability::Cancel,
            ],
            valid_from: Instant::from_unix_secs(1_700_000_000),
            valid_to: Some(Instant::from_unix_secs(1_800_000_000)),
        }
    }

    #[test]
    fn revision_new_trims_snapshots_dedups_capabilities_and_keeps_amounts() {
        let revision = ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-1"),
            1,
            revision_data(),
        )
        .unwrap();

        assert_eq!(revision.name, "福利商城卡");
        assert_eq!(revision.specification.as_deref(), Some("100 元面额"));
        assert_eq!(revision.sales_description, "员工福利采购");
        assert_eq!(revision.base_unit_code, "张");
        assert_eq!(revision.content_hash.len(), 16);
        assert!(revision.content_hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(revision.content_hash, "placeholder");
        assert_ne!(revision.content_hash, "pending-safety-pause-hash");
        assert_eq!(revision.content_hash, expected_content_hash(&revision));
        assert_eq!(
            revision.product_capabilities,
            vec![ProductCapability::Cancel, ProductCapability::Refund],
            "能力清单去重且保留首次顺序"
        );
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(
            revision.minimum_purchase_quantity,
            Quantity::from_str("1.000000").unwrap()
        );
        assert_eq!(revision.sales_price_gross, Amount::from_str("100.00").unwrap());
        assert_eq!(
            revision.supplier_offering_revision_id,
            SupplierOfferingRevisionId::new("offer-rev-1")
        );
    }

    #[test]
    fn revision_new_rejects_empty_required_snapshots() {
        let blank_name = ProductPublicationRevisionData {
            name: "   ".to_string(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-2"),
            1,
            blank_name
        )
        .is_err());

        let blank_description = ProductPublicationRevisionData {
            sales_description: "   ".to_string(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-3"),
            1,
            blank_description
        )
        .is_err());
    }

    #[test]
    fn revision_new_rejects_overlong_snapshot_fields() {
        let overlong_name = ProductPublicationRevisionData {
            name: "n".repeat(257),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-5"),
            1,
            overlong_name
        )
        .is_err());

        let overlong_description = ProductPublicationRevisionData {
            sales_description: "d".repeat(4097),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-6"),
            1,
            overlong_description
        )
        .is_err());
    }

    #[test]
    fn revision_new_rejects_out_of_bounds_money_and_quantity() {
        let zero_minimum = ProductPublicationRevisionData {
            minimum_purchase_quantity: Quantity::from_str("0.000000").unwrap(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-8"),
            1,
            zero_minimum
        )
        .is_err());

        let negative_price = ProductPublicationRevisionData {
            sales_price_gross: Amount::from_str("-1.00").unwrap(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-9"),
            1,
            negative_price
        )
        .is_err());

        let negative_rate = ProductPublicationRevisionData {
            sales_tax_rate: Rate::from_str("-0.010000").unwrap(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-10"),
            1,
            negative_rate
        )
        .is_err());
    }

    #[test]
    fn revision_new_rejects_reversed_validity_window() {
        let reversed = ProductPublicationRevisionData {
            valid_from: Instant::from_unix_secs(1_800_000_000),
            valid_to: Some(Instant::from_unix_secs(1_700_000_000)),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-11"),
            1,
            reversed
        )
        .is_err());
    }

    #[test]
    fn revision_amounts_persist_as_decimal128_on_wire() {
        let revision = ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-1"),
            1,
            revision_data(),
        )
        .unwrap();

        let bytes = bson::serialize_to_vec(&revision).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(
            wire_doc.get("minimum_purchase_quantity"),
            Some(bson::Bson::Decimal128(_))
        ));
        assert!(matches!(
            wire_doc.get("sales_price_gross"),
            Some(bson::Bson::Decimal128(_))
        ));
        assert!(matches!(
            wire_doc.get("sales_tax_rate"),
            Some(bson::Bson::Decimal128(_))
        ));

        let back: ProductPublicationRevision = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, revision);
    }

    #[test]
    fn media_new_trims_alt_text_and_keeps_role_and_sort() {
        let media = ProductPublicationRevisionMedia::new(
            ProductPublicationRevisionMediaId::new("media-1"),
            ProductPublicationRevisionId::new("pub-rev-1"),
            ProductPublicationRevisionMediaData {
                file_asset_id: FileAssetId::new("file-1"),
                media_role: MediaRole::Main,
                sort_no: 1,
                alt_text: Some(" 卡面主图 ".to_string()),
            },
        )
        .unwrap();

        assert_eq!(media.media_role, MediaRole::Main);
        assert_eq!(media.sort_no, 1);
        assert_eq!(media.alt_text.as_deref(), Some("卡面主图"));
        assert_eq!(
            media.product_publication_revision_id,
            ProductPublicationRevisionId::new("pub-rev-1")
        );
    }

    #[test]
    fn media_new_rejects_overlong_alt_text() {
        assert!(ProductPublicationRevisionMedia::new(
            ProductPublicationRevisionMediaId::new("media-2"),
            ProductPublicationRevisionId::new("pub-rev-1"),
            ProductPublicationRevisionMediaData {
                file_asset_id: FileAssetId::new("file-1"),
                media_role: MediaRole::Detail,
                sort_no: 2,
                alt_text: Some("a".repeat(257)),
            },
        )
        .is_err());
    }

    #[test]
    fn revision_enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&SaleStatus::PauseOrder).unwrap(),
            "\"pause_order\""
        );
        assert_eq!(
            serde_json::to_string(&ProductCapability::Refund).unwrap(),
            "\"refund\""
        );
        assert_eq!(
            serde_json::to_string(&MediaRole::Carousel).unwrap(),
            "\"carousel\""
        );
        assert_eq!(SaleStatus::OnSale.label(), "上架");
        assert!(SaleStatus::OnSale.is_on_sale());
        assert!(!SaleStatus::PauseOrder.is_on_sale());
        assert_eq!(ProductCapability::Logistics.label(), "物流");
        assert_eq!(MediaRole::Main.label(), "主图");
        assert!(MediaRole::ensure_main_present([MediaRole::Carousel]).is_err());
        assert!(MediaRole::ensure_main_present([MediaRole::Main, MediaRole::Detail]).is_ok());
    }

    #[test]
    fn safety_pause_copy_preserves_content_and_rebinds_media() {
        let current = ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-1"),
            1,
            revision_data(),
        )
        .unwrap();
        let paused = current
            .safety_pause_copy(
                ProductPublicationRevisionId::new("pub-rev-2"),
                2,
                Instant::from_unix_secs(1_750_000_000),
            )
            .unwrap();
        assert_eq!(paused.sale_status, SaleStatus::PauseOrder);
        assert_eq!(paused.name, current.name);
        assert_eq!(paused.revision.revision_no, 2);
        assert_eq!(paused.valid_to, None);
        assert_eq!(paused.content_hash.len(), 16);
        assert_ne!(paused.content_hash, current.content_hash);
        assert_ne!(paused.content_hash, "pending-safety-pause-hash");
        assert_eq!(paused.content_hash, expected_content_hash(&paused));

        let media = ProductPublicationRevisionMedia::new(
            ProductPublicationRevisionMediaId::new("media-1"),
            ProductPublicationRevisionId::new("pub-rev-1"),
            ProductPublicationRevisionMediaData {
                file_asset_id: FileAssetId::new("file-1"),
                media_role: MediaRole::Main,
                sort_no: 1,
                alt_text: Some("主图".to_string()),
            },
        )
        .unwrap();
        let copied = media
            .copy_to_revision(
                ProductPublicationRevisionMediaId::new("media-2"),
                ProductPublicationRevisionId::new("pub-rev-2"),
            )
            .unwrap();
        assert_eq!(
            copied.product_publication_revision_id,
            ProductPublicationRevisionId::new("pub-rev-2")
        );
        assert_eq!(copied.file_asset_id, media.file_asset_id);
    }

    #[test]
    fn next_revision_no_is_checked_and_shared_with_revision_base() {
        assert_eq!(ProductPublicationRevision::next_revision_no(None).unwrap(), 1);
        assert_eq!(ProductPublicationRevision::next_revision_no(Some(7)).unwrap(), 8);
        assert_eq!(
            ProductPublicationRevision::next_revision_no(Some(u32::MAX))
                .unwrap_err()
                .to_string(),
            "修订序号已达上限"
        );
    }

    fn expected_content_hash(revision: &ProductPublicationRevision) -> String {
        crate::publication::PublicationContentFingerprint::from_snapshot(
            &crate::publication::PublicationContentSnapshot {
                name: &revision.name,
                specification: revision.specification.as_deref(),
                sales_description: &revision.sales_description,
                minimum_purchase_quantity: revision.minimum_purchase_quantity,
                sales_price_gross: revision.sales_price_gross,
                sales_tax_rate: revision.sales_tax_rate,
                base_unit_code: &revision.base_unit_code,
                sales_region: revision.sales_region.as_deref(),
                sale_status: revision.sale_status,
                product_capabilities: &revision.product_capabilities,
                valid_from: revision.valid_from,
                valid_to: revision.valid_to,
            },
        )
        .into_wire()
    }
}
