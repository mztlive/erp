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
/// 发布内容指纹最大长度。
const HASH_MAX_LEN: usize = 128;
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
    /// 发布内容指纹（P3 形成版本时计算）。
    pub content_hash: String,
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
    /// 完成展示名称/销售说明/计量单位/内容指纹的校验与规范化，校验销售不变式
    /// （最小购买量大于零、销售价与税率非负、生效区间不倒挂），并对商品级能力
    /// 清单去重（保留首次出现顺序）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductPublicationRevisionId`）
    /// * `revision_no` - 修订序号（同一发布内从 1 递增）
    /// * `data` - 创建数据
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
        let name = normalize_required_text(data.name, "发布名称不能为空", NAME_MAX_LEN, "发布名称过长")?;
        let specification = normalize_optional_text(data.specification, "规格快照", SPECIFICATION_MAX_LEN)?;
        let sales_description = normalize_required_text(
            data.sales_description,
            "销售说明不能为空",
            DESCRIPTION_MAX_LEN,
            "销售说明过长",
        )?;
        let base_unit_code = normalize_required_text(
            data.base_unit_code,
            "计量单位不能为空",
            BASE_UNIT_CODE_MAX_LEN,
            "计量单位过长",
        )?;
        let sales_region = normalize_optional_text(data.sales_region, "可销售区域", SALES_REGION_MAX_LEN)?;
        let content_hash = normalize_required_text(
            data.content_hash,
            "发布内容指纹不能为空",
            HASH_MAX_LEN,
            "发布内容指纹过长",
        )?;
        validate_sales_invariants(
            data.minimum_purchase_quantity,
            data.sales_price_gross,
            data.sales_tax_rate,
            data.valid_from,
            data.valid_to,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(revision_no),
            product_publication_id: data.product_publication_id,
            sku_revision_id: data.sku_revision_id,
            supplier_offering_revision_id: data.supplier_offering_revision_id,
            category_id: data.category_id,
            name,
            specification,
            sales_description,
            minimum_purchase_quantity: data.minimum_purchase_quantity,
            sales_price_gross: data.sales_price_gross,
            sales_tax_rate: data.sales_tax_rate,
            base_unit_code,
            sales_region,
            sale_status: data.sale_status,
            product_capabilities: dedup_capabilities(data.product_capabilities),
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            content_hash,
        })
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
            content_hash: " aabbccddeeff ".to_string(),
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
        assert_eq!(revision.content_hash, "aabbccddeeff");
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

        let blank_hash = ProductPublicationRevisionData {
            content_hash: "  ".to_string(),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-4"),
            1,
            blank_hash
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

        let overlong_hash = ProductPublicationRevisionData {
            content_hash: "h".repeat(129),
            ..revision_data()
        };
        assert!(ProductPublicationRevision::new(
            ProductPublicationRevisionId::new("pub-rev-7"),
            1,
            overlong_hash
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
        assert_eq!(ProductCapability::Logistics.label(), "物流");
        assert_eq!(MediaRole::Main.label(), "主图");
    }
}
