//! `supplier_catalog_product`(+`_revision`、`_revision_media`)（数据模型 §6.14）。
//!
//! 供应商 SPU 稳定身份只保存身份、来源类型与当前修订指针；内容全部放不可变
//! `supplier_catalog_product_revision`。来源修订先进入供应商商品库，不直接修改公司
//! SKU 修订、公司商品销售查询或商城商品（§6.14 必需约束）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::stable::StableBase;
use crate::common::time::BusinessDate;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    FileAssetId, SupplierAccountId, SupplierApiConnectionId, SupplierCatalogProductId,
    SupplierCatalogProductRevisionId, SupplierCatalogProductRevisionMediaId,
};
use crate::supplier_catalog::types::{
    normalize_attributes, CatalogItemStatus, CatalogSourceType, SourceAttribute,
};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// SPU 编码最大长度。
const SPU_CODE_MAX_LEN: usize = 128;
/// 名称最大长度。
const NAME_MAX_LEN: usize = 256;
/// 描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 2000;
/// 来源商品类型/分类/品牌最大长度。
const SOURCE_KIND_MAX_LEN: usize = 128;
/// 来源修订标识最大长度。
const REVISION_TOKEN_MAX_LEN: usize = 256;
/// 白名单 HMAC 最大长度。
const PAYLOAD_HMAC_MAX_LEN: usize = 128;
/// 来源媒体 URL 最大长度。
const SOURCE_URL_MAX_LEN: usize = 1024;
/// 结构化描述属性最大条数。
const MAX_ATTRIBUTES: usize = 100;

/// 供应商 SPU 创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogProductData {
    /// 来源供应商（必填）。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// API 连接；仅 `source_type = API` 可填写。
    pub source_connection_id: Option<SupplierApiConnectionId>,
    /// 供应商 SPU 编码；供应商未提供时由 ERP 生成来源内稳定代码。
    pub supplier_spu_code: String,
}

/// 供应商 SPU 稳定身份实体（数据模型 §6.14）。
///
/// `StableBase` 未派生 `PartialEq`，因此本实体手工实现全字段语义相等。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierCatalogProduct {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<CatalogItemStatus>,
    /// 来源供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// API 连接；仅 `source_type = API` 可填写。
    pub source_connection_id: Option<SupplierApiConnectionId>,
    /// 供应商 SPU 编码（同一供应商内唯一，创建后不可修改）。
    pub supplier_spu_code: String,
}

impl PartialEq for SupplierCatalogProduct {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.supplier_id == other.supplier_id
            && self.source_type == other.source_type
            && self.source_connection_id == other.source_connection_id
            && self.supplier_spu_code == other.supplier_spu_code
    }
}

impl Eq for SupplierCatalogProduct {}

impl SupplierCatalogProduct {
    /// 创建供应商 SPU。
    ///
    /// 完成 SPU 编码校验与规范化，并强制「非 API 来源不得携带连接」不变式
    /// （§6.14：`source_type <> API` 时 `source_connection_id IS NULL`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogProductId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的 SPU 实体（初始状态 `Active`）。
    ///
    /// # 错误
    /// SPU 编码为空/超长，或非 API 来源携带连接时返回错误。
    pub fn new(
        id: SupplierCatalogProductId,
        data: SupplierCatalogProductData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let supplier_spu_code = normalize_required_text(
            data.supplier_spu_code,
            "供应商 SPU 编码不能为空",
            SPU_CODE_MAX_LEN,
            "供应商 SPU 编码过长",
        )?;
        ensure_connection_ownership(data.source_type, data.source_connection_id.is_some())?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(CatalogItemStatus::Active, created_by),
            supplier_id: data.supplier_id,
            source_type: data.source_type,
            source_connection_id: data.source_connection_id,
            supplier_spu_code,
        })
    }

    /// 更新供应商 SPU 状态（正常/停止供应/异常）。
    ///
    /// `supplier_id`/`source_type`/`source_connection_id`/`supplier_spu_code`
    /// 是来源身份关键字段，不允许在通用更新中修改；内容变化走新修订。
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

/// 供应商 SPU 来源修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogProductRevisionData {
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: SupplierCatalogProductId,
    /// 修订号（同一 SPU 内从 1 递增）。
    pub revision_no: u32,
    /// SPU 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 来源商品类型声明；手工来源必填（P1 校验，P3 按父 SPU 来源类型传入）。
    pub source_product_kind: Option<String>,
    /// 来源分类。
    pub source_category: Option<String>,
    /// 来源品牌。
    pub source_brand: Option<String>,
    /// 结构化描述属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源修订标识（API/文件版本标识）。
    pub source_revision_token: Option<String>,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// 规范化白名单字段的 keyed HMAC（幂等与内容指纹）。
    pub payload_hmac: String,
    /// 有效期开始。
    pub valid_from: Option<BusinessDate>,
    /// 有效期结束（必须晚于 `valid_from`）。
    pub valid_to: Option<BusinessDate>,
}

/// 供应商 SPU 来源修订实体（不可变修订，数据模型 §6.14/§4.4）。
///
/// 修订一经形成不得修改；「详情即编辑」保存只追加来源修订（§6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCatalogProductRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: SupplierCatalogProductId,
    /// SPU 名称。
    pub name: String,
    /// 描述。
    pub description: Option<String>,
    /// 来源商品类型声明。
    pub source_product_kind: Option<String>,
    /// 来源分类。
    pub source_category: Option<String>,
    /// 来源品牌。
    pub source_brand: Option<String>,
    /// 结构化描述属性。
    pub structured_attributes: Vec<SourceAttribute>,
    /// 来源修订标识。
    pub source_revision_token: Option<String>,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// 规范化白名单字段的 keyed HMAC。
    pub payload_hmac: String,
    /// 有效期开始。
    pub valid_from: Option<BusinessDate>,
    /// 有效期结束。
    pub valid_to: Option<BusinessDate>,
}

impl SupplierCatalogProductRevision {
    /// 创建供应商 SPU 来源修订。
    ///
    /// 完成名称/描述/来源分类/来源品牌/修订标识/白名单 HMAC 的校验与规范化，
    /// 并按来源类型强制「手工来源 `source_product_kind` 必填」（§6.14）与
    /// 有效期窗口校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogProductRevisionId`）
    /// * `data` - 创建数据
    /// * `parent_source_type` - 所属 SPU 的来源类型（判定手工来源必填）
    ///
    /// # 返回
    /// 返回新建的来源修订实体。
    ///
    /// # 错误
    /// 修订号为零、必填字段为空/超长、属性超限、手工来源缺少
    /// `source_product_kind`，或有效期窗口倒挂时返回错误。
    pub fn new(
        id: SupplierCatalogProductRevisionId,
        data: SupplierCatalogProductRevisionData,
        parent_source_type: CatalogSourceType,
    ) -> Result<Self> {
        ensure_revision_no(data.revision_no)?;
        let texts = normalize_revision_texts(&data, parent_source_type)?;
        let structured_attributes = normalize_attributes(data.structured_attributes, MAX_ATTRIBUTES)?;
        let source_revision_token =
            normalize_optional_text(data.source_revision_token, "来源修订标识", REVISION_TOKEN_MAX_LEN)?;
        let payload_hmac = normalize_payload_hmac(data.payload_hmac)?;
        ensure_validity_window(data.valid_from, data.valid_to)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_catalog_product_id: data.supplier_catalog_product_id,
            name: texts.name,
            description: texts.description,
            source_product_kind: texts.source_product_kind,
            source_category: texts.source_category,
            source_brand: texts.source_brand,
            structured_attributes,
            source_revision_token,
            source_updated_at: data.source_updated_at,
            payload_hmac,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
        })
    }
}

/// 来源 SPU 媒体用途（§6.14：`SPU_CAROUSEL`、`SPU_DETAIL`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MediaUsage {
    /// SPU 轮播图。
    #[serde(rename = "SPU_CAROUSEL")]
    SpuCarousel,
    /// SPU 详情图。
    #[serde(rename = "SPU_DETAIL")]
    SpuDetail,
}

impl MediaUsage {
    /// 返回用途的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::SpuCarousel => "轮播图",
            Self::SpuDetail => "详情图",
        }
    }

    /// 返回用途的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SpuCarousel => "SPU_CAROUSEL",
            Self::SpuDetail => "SPU_DETAIL",
        }
    }
}

/// 来源媒体归档状态（§6.14：待导入、已归档、失败）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ArchiveStatus {
    /// 待导入。
    #[serde(rename = "PENDING_IMPORT")]
    PendingImport,
    /// 已归档。
    Archived,
    /// 失败。
    Failed,
}

impl ArchiveStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingImport => "待导入",
            Self::Archived => "已归档",
            Self::Failed => "失败",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingImport => "PENDING_IMPORT",
            Self::Archived => "ARCHIVED",
            Self::Failed => "FAILED",
        }
    }
}

/// 来源 SPU 图文创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogProductRevisionMediaData {
    /// 所属供应商 SPU 来源修订。
    pub supplier_catalog_product_revision_id: SupplierCatalogProductRevisionId,
    /// 媒体用途。
    pub media_usage: MediaUsage,
    /// 已归档受控文件；归档完成后必填。
    pub file_asset_id: Option<FileAssetId>,
    /// 来源取回地址（不得作为公司商品长期媒体值）。
    pub source_url_snapshot: Option<String>,
    /// 归档状态。
    pub archive_status: ArchiveStatus,
    /// 同用途展示顺序。
    pub sort_order: u32,
}

/// 来源 SPU 图文实体（数据模型 §6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCatalogProductRevisionMedia {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属供应商 SPU 来源修订。
    pub supplier_catalog_product_revision_id: SupplierCatalogProductRevisionId,
    /// 媒体用途。
    pub media_usage: MediaUsage,
    /// 已归档受控文件。
    pub file_asset_id: Option<FileAssetId>,
    /// 来源取回地址。
    pub source_url_snapshot: Option<String>,
    /// 归档状态。
    pub archive_status: ArchiveStatus,
    /// 同用途展示顺序。
    pub sort_order: u32,
}

impl SupplierCatalogProductRevisionMedia {
    /// 创建来源 SPU 图文。
    ///
    /// 完成来源 URL 校验与规范化，并强制「已归档媒体必须绑定受控文件」
    /// 不变式（§6.14：归档完成后 `file_asset_id` 必填）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCatalogProductRevisionMediaId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的图文实体。
    ///
    /// # 错误
    /// 来源 URL 超长，或已归档媒体缺少文件引用时返回错误。
    ///
    /// # 说明
    /// 同一修订下 `(media_usage, sort_order)` 唯一（§6.14）依赖行内聚合查询，
    /// 留 P3 校验。
    pub fn new(
        id: SupplierCatalogProductRevisionMediaId,
        data: SupplierCatalogProductRevisionMediaData,
    ) -> Result<Self> {
        let source_url_snapshot =
            normalize_optional_text(data.source_url_snapshot, "来源取回地址", SOURCE_URL_MAX_LEN)?;
        if data.archive_status == ArchiveStatus::Archived && data.file_asset_id.is_none() {
            return Err(Error::from("已归档媒体必须绑定受控文件"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_catalog_product_revision_id: data.supplier_catalog_product_revision_id,
            media_usage: data.media_usage,
            file_asset_id: data.file_asset_id,
            source_url_snapshot,
            archive_status: data.archive_status,
            sort_order: data.sort_order,
        })
    }
}

/// SPU 修订文本字段的规范化结果（名称/描述/来源类型/分类/品牌）。
struct ProductRevisionTexts {
    name: String,
    description: Option<String>,
    source_product_kind: Option<String>,
    source_category: Option<String>,
    source_brand: Option<String>,
}

/// 规范化 SPU 修订文本字段（名称/描述/来源类型/分类/品牌）。
///
/// # 参数
/// * `data` - SPU 来源修订创建数据
/// * `parent_source_type` - 所属 SPU 的来源类型（判定手工来源必填）
///
/// # 返回
/// 返回规范化后的文本字段。
///
/// # 错误
/// 名称为空/超长，或手工来源缺少来源商品类型时返回错误。
fn normalize_revision_texts(
    data: &SupplierCatalogProductRevisionData,
    parent_source_type: CatalogSourceType,
) -> Result<ProductRevisionTexts> {
    let name = normalize_required_text(
        data.name.clone(),
        "SPU 名称不能为空",
        NAME_MAX_LEN,
        "SPU 名称过长",
    )?;
    let description = normalize_optional_text(data.description.clone(), "描述", DESCRIPTION_MAX_LEN)?;
    let source_product_kind = normalize_optional_text(
        data.source_product_kind.clone(),
        "来源商品类型",
        SOURCE_KIND_MAX_LEN,
    )?;
    if parent_source_type == CatalogSourceType::Manual && source_product_kind.is_none() {
        return Err(Error::from("手工来源必须填写来源商品类型"));
    }
    let source_category =
        normalize_optional_text(data.source_category.clone(), "来源分类", SOURCE_KIND_MAX_LEN)?;
    let source_brand = normalize_optional_text(data.source_brand.clone(), "来源品牌", SOURCE_KIND_MAX_LEN)?;
    Ok(ProductRevisionTexts {
        name,
        description,
        source_product_kind,
        source_category,
        source_brand,
    })
}

/// 规范化白名单 HMAC。
///
/// # 参数
/// * `value` - 原始 HMAC 文本
///
/// # 返回
/// 返回去空白后的 HMAC。
///
/// # 错误
/// HMAC 为空或超长时返回错误。
fn normalize_payload_hmac(value: String) -> Result<String> {
    normalize_required_text(
        value,
        "白名单 HMAC 不能为空",
        PAYLOAD_HMAC_MAX_LEN,
        "白名单 HMAC 过长",
    )
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

/// 校验连接归属：非 API 来源不得携带连接。
///
/// # 参数
/// * `source_type` - 来源类型
/// * `has_connection` - 是否携带 API 连接
///
/// # 错误
/// 非 API 来源携带连接时返回错误。
fn ensure_connection_ownership(source_type: CatalogSourceType, has_connection: bool) -> Result<()> {
    if source_type != CatalogSourceType::Api && has_connection {
        return Err(Error::from("只有 API 来源可以填写连接"));
    }
    Ok(())
}

/// 校验有效期窗口。
///
/// # 参数
/// * `valid_from` - 有效期开始
/// * `valid_to` - 有效期结束
///
/// # 错误
/// 有效期结束早于或等于开始时返回错误。
fn ensure_validity_window(valid_from: Option<BusinessDate>, valid_to: Option<BusinessDate>) -> Result<()> {
    if let (Some(valid_from), Some(valid_to)) = (valid_from, valid_to) {
        if valid_to <= valid_from {
            return Err(Error::from("有效期结束必须晚于开始"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ArchiveStatus, MediaUsage, SupplierCatalogProduct, SupplierCatalogProductData,
        SupplierCatalogProductRevision, SupplierCatalogProductRevisionData,
        SupplierCatalogProductRevisionMedia, SupplierCatalogProductRevisionMediaData,
    };
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        FileAssetId, SupplierAccountId, SupplierApiConnectionId, SupplierCatalogProductId,
        SupplierCatalogProductRevisionId, SupplierCatalogProductRevisionMediaId,
    };
    use crate::supplier_catalog::types::{CatalogItemStatus, CatalogSourceType};

    fn product_data() -> SupplierCatalogProductData {
        SupplierCatalogProductData {
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: CatalogSourceType::Excel,
            source_connection_id: None,
            supplier_spu_code: " SPU-001 ".to_string(),
        }
    }

    fn revision_data() -> SupplierCatalogProductRevisionData {
        SupplierCatalogProductRevisionData {
            supplier_catalog_product_id: SupplierCatalogProductId::new("scp-1"),
            revision_no: 1,
            name: " 慰问礼包 ".to_string(),
            description: Some(" 年节慰问组合 ".to_string()),
            source_product_kind: None,
            source_category: Some(" 食品 ".to_string()),
            source_brand: Some("华联".to_string()),
            structured_attributes: Vec::new(),
            source_revision_token: Some("v1".to_string()),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            payload_hmac: "abc123".to_string(),
            valid_from: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
        }
    }

    fn media_data() -> SupplierCatalogProductRevisionMediaData {
        SupplierCatalogProductRevisionMediaData {
            supplier_catalog_product_revision_id: SupplierCatalogProductRevisionId::new("scpr-1"),
            media_usage: MediaUsage::SpuCarousel,
            file_asset_id: Some(FileAssetId::new("file-1")),
            source_url_snapshot: Some(" https://src.example.com/a.jpg ".to_string()),
            archive_status: ArchiveStatus::Archived,
            sort_order: 1,
        }
    }

    #[test]
    fn product_new_trims_and_enforces_connection_ownership() {
        let product =
            SupplierCatalogProduct::new(SupplierCatalogProductId::new("scp-1"), product_data(), "admin-1")
                .unwrap();
        assert_eq!(product.supplier_spu_code, "SPU-001");
        assert_eq!(product.stable.status(), CatalogItemStatus::Active);

        let illegal = SupplierCatalogProductData {
            source_type: CatalogSourceType::Manual,
            source_connection_id: Some(SupplierApiConnectionId::new("conn-1")),
            ..product_data()
        };
        assert!(
            SupplierCatalogProduct::new(SupplierCatalogProductId::new("scp-2"), illegal, "admin-1").is_err()
        );

        let api_with_connection = SupplierCatalogProductData {
            source_type: CatalogSourceType::Api,
            source_connection_id: Some(SupplierApiConnectionId::new("conn-1")),
            ..product_data()
        };
        assert!(SupplierCatalogProduct::new(
            SupplierCatalogProductId::new("scp-3"),
            api_with_connection,
            "admin-1",
        )
        .is_ok());
    }

    #[test]
    fn product_update_changes_status() {
        let mut product =
            SupplierCatalogProduct::new(SupplierCatalogProductId::new("scp-1"), product_data(), "admin-1")
                .unwrap();
        product.update(CatalogItemStatus::Stopped, "admin-2").unwrap();
        assert_eq!(product.stable.status(), CatalogItemStatus::Stopped);
        assert_eq!(product.stable.updated_by, "admin-2");
    }

    #[test]
    fn product_revision_normalizes_and_requires_manual_kind() {
        let revision = SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-1"),
            revision_data(),
            CatalogSourceType::Excel,
        )
        .unwrap();
        assert_eq!(revision.name, "慰问礼包");
        assert_eq!(revision.source_category.as_deref(), Some("食品"));
        assert_eq!(revision.revision.revision_no, 1);

        let manual = SupplierCatalogProductRevisionData {
            source_product_kind: Some(" 实物 ".to_string()),
            ..revision_data()
        };
        let revision = SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-2"),
            manual,
            CatalogSourceType::Manual,
        )
        .unwrap();
        assert_eq!(revision.source_product_kind.as_deref(), Some("实物"));

        let manual_without_kind = SupplierCatalogProductRevisionData {
            source_product_kind: None,
            ..revision_data()
        };
        assert!(SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-3"),
            manual_without_kind,
            CatalogSourceType::Manual,
        )
        .is_err());
    }

    #[test]
    fn product_revision_rejects_zero_no_and_inverted_validity() {
        let zero = SupplierCatalogProductRevisionData {
            revision_no: 0,
            ..revision_data()
        };
        assert!(SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-4"),
            zero,
            CatalogSourceType::Excel,
        )
        .is_err());

        let inverted = SupplierCatalogProductRevisionData {
            valid_from: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            valid_to: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            ..revision_data()
        };
        assert!(SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-5"),
            inverted,
            CatalogSourceType::Excel,
        )
        .is_err());

        let empty_hmac = SupplierCatalogProductRevisionData {
            payload_hmac: "   ".to_string(),
            ..revision_data()
        };
        assert!(SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new("scpr-6"),
            empty_hmac,
            CatalogSourceType::Excel,
        )
        .is_err());
    }

    #[test]
    fn media_requires_file_when_archived() {
        let media = SupplierCatalogProductRevisionMedia::new(
            SupplierCatalogProductRevisionMediaId::new("scprm-1"),
            media_data(),
        )
        .unwrap();
        assert_eq!(
            media.source_url_snapshot.as_deref(),
            Some("https://src.example.com/a.jpg")
        );

        let no_file = SupplierCatalogProductRevisionMediaData {
            file_asset_id: None,
            ..media_data()
        };
        assert!(SupplierCatalogProductRevisionMedia::new(
            SupplierCatalogProductRevisionMediaId::new("scprm-2"),
            no_file,
        )
        .is_err());

        let pending = SupplierCatalogProductRevisionMediaData {
            file_asset_id: None,
            archive_status: ArchiveStatus::PendingImport,
            ..media_data()
        };
        assert!(SupplierCatalogProductRevisionMedia::new(
            SupplierCatalogProductRevisionMediaId::new("scprm-3"),
            pending,
        )
        .is_ok());
    }

    #[test]
    fn media_usages_serialize_with_underscores() {
        assert_eq!(
            serde_json::to_string(&MediaUsage::SpuCarousel).unwrap(),
            "\"SPU_CAROUSEL\""
        );
        assert_eq!(
            serde_json::to_string(&ArchiveStatus::PendingImport).unwrap(),
            "\"PENDING_IMPORT\""
        );
        assert_eq!(MediaUsage::SpuDetail.label(), "详情图");
    }
}
