//! `supplier_catalog_sku` / `supplier_catalog_sku_revision`（数据模型 §6.14）。
//!
//! 供应商 SKU 稳定身份只保存所属 SPU、SKU 编码与当前修订指针；来源 SKU 观察事实
//! 全部放不可变 `supplier_catalog_sku_revision`。目录观察价（一件代发底价/集采底价）
//! 未确认前不是采购成本；供应商目录不保存统一含税报价、进项税率、运费等字段
//! （§6.14 UI 同构约定）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::stable::StableBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{FileAssetId, SupplierCatalogProductId, SupplierCatalogSkuId, SupplierCatalogSkuRevisionId};
use crate::money::{Amount, Quantity};
use crate::supplier_catalog::product::ArchiveStatus;
use crate::supplier_catalog::types::{normalize_attributes, CatalogItemStatus, SourceAttribute};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// SKU 编码最大长度。
const SKU_CODE_MAX_LEN: usize = 128;
/// 名称最大长度。
const NAME_MAX_LEN: usize = 256;
/// 规格最大长度。
const SPECIFICATION_MAX_LEN: usize = 512;
/// 单位快照最大长度。
const BASE_UNIT_MAX_LEN: usize = 64;
/// 条码最大长度。
const BARCODE_MAX_LEN: usize = 64;
/// 来源 SKU 主图取回地址最大长度。
const SOURCE_MAIN_IMAGE_URL_MAX_LEN: usize = 1024;
/// 来源修订标识最大长度。
const REVISION_TOKEN_MAX_LEN: usize = 256;
/// 白名单 HMAC 最大长度。
const PAYLOAD_HMAC_MAX_LEN: usize = 128;
/// 结构化规格属性最大条数。
const MAX_ATTRIBUTES: usize = 100;

/// 供应商 SKU 创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuData {
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: SupplierCatalogProductId,
    /// 供应商 SKU 编码（同一供应商内唯一）。
    pub supplier_sku_code: String,
}

/// 供应商 SKU 稳定身份实体（数据模型 §6.14）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierCatalogSku {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CatalogItemStatus>,
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: SupplierCatalogProductId,
    /// 供应商 SKU 编码（创建后不可修改）。
    pub supplier_sku_code: String,
}

impl PartialEq for SupplierCatalogSku {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.supplier_catalog_product_id == other.supplier_catalog_product_id
            && self.supplier_sku_code == other.supplier_sku_code
    }
}

impl Eq for SupplierCatalogSku {}

impl SupplierCatalogSku {
    /// 创建供应商 SKU。
    ///
    /// 完成 SKU 编码校验与规范化；初始状态为 `Active`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogSkuId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的 SKU 实体。
    ///
    /// # 错误
    /// SKU 编码为空或超长时返回错误。
    pub fn new(
        id: SupplierCatalogSkuId,
        data: SupplierCatalogSkuData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let supplier_sku_code = normalize_required_text(
            data.supplier_sku_code,
            "供应商 SKU 编码不能为空",
            SKU_CODE_MAX_LEN,
            "供应商 SKU 编码过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(CatalogItemStatus::Active, created_by),
            supplier_catalog_product_id: data.supplier_catalog_product_id,
            supplier_sku_code,
        })
    }

    /// 更新供应商 SKU 状态（正常/停止供应/异常）。
    ///
    /// `supplier_catalog_product_id`/`supplier_sku_code` 是来源身份关键字段，
    /// 不允许在通用更新中修改；内容变化走新修订。
    ///
    /// # 参数
    /// * `status` - 新状态
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, status: CatalogItemStatus, updated_by: impl Into<String>) -> Result<()> {
        self.stable.status = status;
        self.stable.touch(updated_by);
        Ok(())
    }
}

/// 来源 SKU 可供状态（§6.14：可供、不可供、停止供应、来源陈旧）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AvailabilityStatus {
    /// 可供。
    Available,
    /// 不可供。
    Unavailable,
    /// 停止供应。
    Stopped,
    /// 来源陈旧（新鲜度超时）。
    Stale,
}

impl AvailabilityStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Available => "可供",
            Self::Unavailable => "不可供",
            Self::Stopped => "停止供应",
            Self::Stale => "来源陈旧",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Available => "AVAILABLE",
            Self::Unavailable => "UNAVAILABLE",
            Self::Stopped => "STOPPED",
            Self::Stale => "STALE",
        }
    }
}

/// 来源 SKU 修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuRevisionData {
    /// 所属供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// 修订号（同一 SKU 内从 1 递增）。
    pub revision_no: u32,
    /// 来源修订标识（API/文件版本标识）。
    pub source_revision_token: Option<String>,
    /// 供应商商品名称。
    pub name: String,
    /// 供应商规格。
    pub specification: String,
    /// 供应商单位快照（只用于匹配和预填）。
    pub source_base_unit: Option<String>,
    /// 条码（只用于匹配和预填）。
    pub barcode: Option<String>,
    /// 已规范化的来源规格属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源 SKU 主图（已归档受控文件）。
    pub source_main_image_asset_id: Option<FileAssetId>,
    /// 来源 SKU 主图取回地址（归档前快照；不得作为公司商品长期媒体值）。
    pub source_main_image_url_snapshot: Option<String>,
    /// 来源 SKU 主图归档状态。
    pub main_image_archive_status: Option<ArchiveStatus>,
    /// 一件代发底价（含税运）；目录观察价，未确认前不是采购成本。
    pub dropship_floor_price_gross: Option<Amount>,
    /// 集采底价（含税）。
    pub bulk_floor_price_gross: Option<Amount>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<Quantity>,
    /// 来源库存或可供数量。
    pub available_quantity: Option<Quantity>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// ERP 接收时间（手工来源可与来源更新时间相同）。
    pub received_at: Instant,
    /// 规范化白名单字段的 keyed HMAC（无版本号时的幂等键）。
    pub source_payload_hmac: Option<String>,
}

/// 来源 SKU 修订实体（不可变修订，数据模型 §6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCatalogSkuRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// 来源修订标识。
    pub source_revision_token: Option<String>,
    /// 供应商商品名称。
    pub name: String,
    /// 供应商规格。
    pub specification: String,
    /// 供应商单位快照。
    pub source_base_unit: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 已规范化的来源规格属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源 SKU 主图（已归档受控文件）。
    pub source_main_image_asset_id: Option<FileAssetId>,
    /// 来源 SKU 主图取回地址（归档前快照；不得作为公司商品长期媒体值）。
    pub source_main_image_url_snapshot: Option<String>,
    /// 来源 SKU 主图归档状态。
    pub main_image_archive_status: Option<ArchiveStatus>,
    /// 一件代发底价（含税运）。
    pub dropship_floor_price_gross: Option<Amount>,
    /// 集采底价（含税）。
    pub bulk_floor_price_gross: Option<Amount>,
    /// 集采起订量。
    pub bulk_minimum_order_quantity: Option<Quantity>,
    /// 来源库存或可供数量。
    pub available_quantity: Option<Quantity>,
    /// 可供状态。
    pub availability_status: AvailabilityStatus,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 规范化白名单字段的 keyed HMAC。
    pub source_payload_hmac: Option<String>,
}

impl SupplierCatalogSkuRevision {
    /// 创建来源 SKU 修订。
    ///
    /// 完成名称/规格/单位/条码/修订标识的校验与规范化，并强制：
    /// - 底价/起订量/可供数量非负（§4.2 数值规则）；
    /// - 幂等键存在性：`source_revision_token` 与 `source_payload_hmac`
    ///   至少其一非空（§6.14 幂等约定，P3 填充并计算）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogSkuRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的来源修订实体。
    ///
    /// # 错误
    /// 修订号为零、必填字段为空/超长、数值越界或幂等键缺失时返回错误。
    pub fn new(id: SupplierCatalogSkuRevisionId, data: SupplierCatalogSkuRevisionData) -> Result<Self> {
        ensure_revision_no(data.revision_no)?;
        let texts = normalize_sku_revision_texts(&data)?;
        let structured_attributes = normalize_attributes(data.structured_attributes, MAX_ATTRIBUTES)?;
        ensure_amount_non_negative(data.dropship_floor_price_gross, "一件代发底价")?;
        ensure_amount_non_negative(data.bulk_floor_price_gross, "集采底价")?;
        ensure_quantity_non_negative(data.bulk_minimum_order_quantity, "集采起订量")?;
        ensure_quantity_non_negative(data.available_quantity, "可供数量")?;
        let source_main_image_url_snapshot = normalize_optional_text(
            data.source_main_image_url_snapshot,
            "来源主图取回地址",
            SOURCE_MAIN_IMAGE_URL_MAX_LEN,
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_catalog_sku_id: data.supplier_catalog_sku_id,
            source_revision_token: texts.source_revision_token,
            name: texts.name,
            specification: texts.specification,
            source_base_unit: texts.source_base_unit,
            barcode: texts.barcode,
            structured_attributes,
            source_main_image_asset_id: data.source_main_image_asset_id,
            source_main_image_url_snapshot,
            main_image_archive_status: data.main_image_archive_status,
            dropship_floor_price_gross: data.dropship_floor_price_gross,
            bulk_floor_price_gross: data.bulk_floor_price_gross,
            bulk_minimum_order_quantity: data.bulk_minimum_order_quantity,
            available_quantity: data.available_quantity,
            availability_status: data.availability_status,
            source_updated_at: data.source_updated_at,
            received_at: data.received_at,
            source_payload_hmac: texts.source_payload_hmac,
        })
    }
}

/// SKU 修订文本字段的规范化结果（名称/规格/单位/条码/幂等键）。
struct SkuRevisionTexts {
    name: String,
    specification: String,
    source_base_unit: Option<String>,
    barcode: Option<String>,
    source_revision_token: Option<String>,
    source_payload_hmac: Option<String>,
}

/// 规范化 SKU 修订文本字段（名称/规格/单位/条码/修订标识/白名单 HMAC）。
///
/// # 参数
/// * `data` - 来源 SKU 修订创建数据
///
/// # 返回
/// 返回规范化后的文本字段。
///
/// # 错误
/// 名称/规格为空或超长，或幂等键（修订标识/白名单 HMAC）缺失时返回错误。
fn normalize_sku_revision_texts(data: &SupplierCatalogSkuRevisionData) -> Result<SkuRevisionTexts> {
    let name = normalize_required_text(
        data.name.clone(),
        "供应商商品名称不能为空",
        NAME_MAX_LEN,
        "供应商商品名称过长",
    )?;
    let specification = normalize_required_text(
        data.specification.clone(),
        "供应商规格不能为空",
        SPECIFICATION_MAX_LEN,
        "供应商规格过长",
    )?;
    let source_base_unit =
        normalize_optional_text(data.source_base_unit.clone(), "供应商单位", BASE_UNIT_MAX_LEN)?;
    let barcode = normalize_optional_text(data.barcode.clone(), "条码", BARCODE_MAX_LEN)?;
    let source_revision_token = normalize_optional_text(
        data.source_revision_token.clone(),
        "来源修订标识",
        REVISION_TOKEN_MAX_LEN,
    )?;
    let source_payload_hmac = normalize_optional_text(
        data.source_payload_hmac.clone(),
        "白名单 HMAC",
        PAYLOAD_HMAC_MAX_LEN,
    )?;
    if source_revision_token.is_none() && source_payload_hmac.is_none() {
        return Err(Error::from("来源修订标识与白名单 HMAC 必须至少提供其一"));
    }
    Ok(SkuRevisionTexts {
        name,
        specification,
        source_base_unit,
        barcode,
        source_revision_token,
        source_payload_hmac,
    })
}

/// 校验修订号从 1 开始。
///
/// # 参数
/// * `revision_no` - 修订号
///
/// # 错误
/// 修订号为零时返回错误。
fn ensure_revision_no(revision_no: u32) -> Result<()> {
    if revision_no == 0 {
        return Err(Error::from("修订号必须从 1 开始"));
    }
    Ok(())
}

/// 校验金额非负。
///
/// # 参数
/// * `value` - 可选金额
/// * `label` - 字段说明
///
/// # 错误
/// 金额存在且为负时返回错误。
fn ensure_amount_non_negative(value: Option<Amount>, label: &str) -> Result<()> {
    if let Some(value) = value {
        if value.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from(format!("{label}不能为负")));
        }
    }
    Ok(())
}

/// 校验数量非负。
///
/// # 参数
/// * `value` - 可选数量
/// * `label` - 字段说明
///
/// # 错误
/// 数量存在且为负时返回错误。
fn ensure_quantity_non_negative(value: Option<Quantity>, label: &str) -> Result<()> {
    if let Some(value) = value {
        if value.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from(format!("{label}不能为负")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AvailabilityStatus, SupplierCatalogSku, SupplierCatalogSkuData, SupplierCatalogSkuRevision,
        SupplierCatalogSkuRevisionData,
    };
    use crate::common::time::Instant;
    use crate::ids::{SupplierCatalogProductId, SupplierCatalogSkuId, SupplierCatalogSkuRevisionId};
    use crate::money::{Amount, Quantity};
    use crate::supplier_catalog::types::CatalogItemStatus;
    use std::str::FromStr;

    fn sku_data() -> SupplierCatalogSkuData {
        SupplierCatalogSkuData {
            supplier_catalog_product_id: SupplierCatalogProductId::new("scp-1"),
            supplier_sku_code: " SKU-001 ".to_string(),
        }
    }

    fn sku_revision_data() -> SupplierCatalogSkuRevisionData {
        SupplierCatalogSkuRevisionData {
            supplier_catalog_sku_id: SupplierCatalogSkuId::new("scs-1"),
            revision_no: 1,
            source_revision_token: Some("v3".to_string()),
            name: " 慰问礼包·标准 ".to_string(),
            specification: " 500g×2 ".to_string(),
            source_base_unit: Some(" 箱 ".to_string()),
            barcode: Some("690000000001".to_string()),
            structured_attributes: Vec::new(),
            source_main_image_asset_id: None,
            source_main_image_url_snapshot: None,
            main_image_archive_status: None,
            dropship_floor_price_gross: Some(Amount::from_str("12.00").unwrap()),
            bulk_floor_price_gross: Some(Amount::from_str("10.00").unwrap()),
            bulk_minimum_order_quantity: Some(Quantity::from_str("10.000000").unwrap()),
            available_quantity: Some(Quantity::from_str("500.000000").unwrap()),
            availability_status: AvailabilityStatus::Available,
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            received_at: Instant::from_unix_secs(1_700_000_100),
            source_payload_hmac: None,
        }
    }

    #[test]
    fn sku_new_trims_code() {
        let sku = SupplierCatalogSku::new(SupplierCatalogSkuId::new("scs-1"), sku_data(), "admin-1").unwrap();
        assert_eq!(sku.supplier_sku_code, "SKU-001");
        assert_eq!(sku.stable.status(), CatalogItemStatus::Active);

        let empty = SupplierCatalogSkuData {
            supplier_sku_code: "   ".to_string(),
            ..sku_data()
        };
        assert!(SupplierCatalogSku::new(SupplierCatalogSkuId::new("scs-2"), empty, "admin-1").is_err());
    }

    #[test]
    fn sku_update_changes_status() {
        let mut sku =
            SupplierCatalogSku::new(SupplierCatalogSkuId::new("scs-1"), sku_data(), "admin-1").unwrap();
        sku.update(CatalogItemStatus::Exception, "admin-2").unwrap();
        assert_eq!(sku.stable.status(), CatalogItemStatus::Exception);
        assert_eq!(sku.stable.updated_by, "admin-2");
    }

    #[test]
    fn sku_revision_normalizes_fields() {
        let revision =
            SupplierCatalogSkuRevision::new(SupplierCatalogSkuRevisionId::new("scsr-1"), sku_revision_data())
                .unwrap();
        assert_eq!(revision.name, "慰问礼包·标准");
        assert_eq!(revision.specification, "500g×2");
        assert_eq!(revision.source_base_unit.as_deref(), Some("箱"));
        assert_eq!(revision.source_revision_token.as_deref(), Some("v3"));
    }

    #[test]
    fn sku_revision_rejects_idempotency_key_missing_and_negative() {
        let no_key = SupplierCatalogSkuRevisionData {
            source_revision_token: None,
            source_payload_hmac: None,
            ..sku_revision_data()
        };
        assert!(
            SupplierCatalogSkuRevision::new(SupplierCatalogSkuRevisionId::new("scsr-2"), no_key,).is_err()
        );

        let hmac_key = SupplierCatalogSkuRevisionData {
            source_revision_token: None,
            source_payload_hmac: Some("deadbeef".to_string()),
            ..sku_revision_data()
        };
        assert!(
            SupplierCatalogSkuRevision::new(SupplierCatalogSkuRevisionId::new("scsr-3"), hmac_key,).is_ok()
        );

        let negative_price = SupplierCatalogSkuRevisionData {
            dropship_floor_price_gross: Some(Amount::from_str("-1.00").unwrap()),
            ..sku_revision_data()
        };
        assert!(
            SupplierCatalogSkuRevision::new(SupplierCatalogSkuRevisionId::new("scsr-4"), negative_price,)
                .is_err()
        );

        let zero_no = SupplierCatalogSkuRevisionData {
            revision_no: 0,
            ..sku_revision_data()
        };
        assert!(
            SupplierCatalogSkuRevision::new(SupplierCatalogSkuRevisionId::new("scsr-5"), zero_no,).is_err()
        );
    }

    #[test]
    fn availability_status_labels_and_codes() {
        assert_eq!(AvailabilityStatus::Stale.label(), "来源陈旧");
        assert_eq!(AvailabilityStatus::Unavailable.as_str(), "UNAVAILABLE");
        assert_eq!(
            serde_json::to_string(&AvailabilityStatus::Stale).unwrap(),
            "\"STALE\""
        );
    }
}
