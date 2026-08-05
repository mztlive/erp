//! `supplier_product_mapping`（数据模型 §6.14）。
//!
//! `supplier_catalog_sku_id → sku_id` 是唯一正式映射粒度；供应商 SPU 只作为页面容器，
//! 不得写入映射关系替代供应商 SKU。映射是审核型关系表，不套用 StableBase/FactBase，
//! 按 §6.14 字典精确建模（`status`、`approved_by`/`approved_at`、`reason`）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SkuId, SupplierCatalogSkuId, SupplierProductMappingId};
use crate::validation::normalize_optional_text;

/// 映射依据最大长度。
const REASON_MAX_LEN: usize = 500;
/// 操作人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 映射状态（§6.14：待审核、已生效、冲突、停用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MappingStatus {
    /// 待审核。
    Pending,
    /// 已生效。
    Active,
    /// 冲突。
    Conflict,
    /// 停用。
    Disabled,
}

impl MappingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待审核",
            Self::Active => "已生效",
            Self::Conflict => "冲突",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Active => "ACTIVE",
            Self::Conflict => "CONFLICT",
            Self::Disabled => "DISABLED",
        }
    }
}

/// 供应商 SKU → 公司 SKU 映射创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierProductMappingData {
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 映射状态。
    pub status: MappingStatus,
    /// 审核人；与 `approved_at` 成对出现，`Active` 状态必填。
    pub approved_by: Option<String>,
    /// 审核时间；与 `approved_by` 成对出现，`Active` 状态必填。
    pub approved_at: Option<Instant>,
    /// 映射依据。
    pub reason: Option<String>,
}

/// 供应商 SKU → 公司 SKU 映射实体（数据模型 §6.14）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierProductMapping {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 映射状态。
    pub status: MappingStatus,
    /// 审核人。
    pub approved_by: Option<String>,
    /// 审核时间。
    pub approved_at: Option<Instant>,
    /// 映射依据。
    pub reason: Option<String>,
}

impl SupplierProductMapping {
    /// 创建供应商 SKU → 公司 SKU 映射。
    ///
    /// 完成审核人/映射依据的校验与规范化，并强制两条不变式：
    /// `approved_at` 与 `approved_by` 成对；`Active` 状态必须携带完整审核信息。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierProductMappingId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的映射实体。
    ///
    /// # 错误
    /// 审核信息不完整或映射依据超长时返回错误。
    ///
    /// # 说明
    /// 「同一供应商 SKU 同一时点只能映射一个公司 SKU」（§6.14）依赖按状态聚合
    /// 查询，留 P3 校验。
    pub fn new(id: SupplierProductMappingId, data: SupplierProductMappingData) -> Result<Self> {
        let approved_by = normalize_optional_text(data.approved_by, "审核人", ACTOR_MAX_LEN)?;
        let reason = normalize_optional_text(data.reason, "映射依据", REASON_MAX_LEN)?;
        ensure_approval_pair(data.status, data.approved_at, approved_by.is_some())?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_catalog_sku_id: data.supplier_catalog_sku_id,
            sku_id: data.sku_id,
            status: data.status,
            approved_by,
            approved_at: data.approved_at,
            reason,
        })
    }

    /// 更新映射状态与审核信息。
    ///
    /// `supplier_catalog_sku_id`/`sku_id` 是映射键，不允许修改；状态改为 `Active`
    /// 时必须携带完整审核信息（与 `new` 同一条校验）。
    ///
    /// # 参数
    /// * `status` - 新状态
    /// * `approved_by` - 审核人（可空）
    /// * `approved_at` - 审核时间（可空）
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 审核信息不完整时返回错误。
    pub fn update(
        &mut self,
        status: MappingStatus,
        approved_by: Option<String>,
        approved_at: Option<Instant>,
    ) -> Result<()> {
        let approved_by = normalize_optional_text(approved_by, "审核人", ACTOR_MAX_LEN)?;
        ensure_approval_pair(status, approved_at, approved_by.is_some())?;
        self.status = status;
        self.approved_by = approved_by;
        self.approved_at = approved_at;
        Ok(())
    }
}

/// 校验审核信息成对且生效状态必填。
///
/// # 参数
/// * `status` - 映射状态
/// * `approved_at` - 审核时间
/// * `has_approved_by` - 审核人是否已提供
///
/// # 错误
/// 审核人/时间不同时出现，或 `Active` 状态缺少审核信息时返回错误。
fn ensure_approval_pair(
    status: MappingStatus,
    approved_at: Option<Instant>,
    has_approved_by: bool,
) -> Result<()> {
    if approved_at.is_some() != has_approved_by {
        return Err(Error::from("审核人与审核时间必须同时提供或同时省略"));
    }
    if status == MappingStatus::Active && !has_approved_by {
        return Err(Error::from("已生效映射必须填写审核人与审核时间"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MappingStatus, SupplierProductMapping, SupplierProductMappingData};
    use crate::common::time::Instant;
    use crate::ids::{SkuId, SupplierCatalogSkuId, SupplierProductMappingId};

    fn mapping_data() -> SupplierProductMappingData {
        SupplierProductMappingData {
            supplier_catalog_sku_id: SupplierCatalogSkuId::new("scs-1"),
            sku_id: SkuId::new("sku-1"),
            status: MappingStatus::Pending,
            approved_by: None,
            approved_at: None,
            reason: Some(" 同款同规格 ".to_string()),
        }
    }

    #[test]
    fn mapping_happy_path_and_active_requires_approval() {
        let mapping =
            SupplierProductMapping::new(SupplierProductMappingId::new("spm-1"), mapping_data()).unwrap();
        assert_eq!(mapping.status, MappingStatus::Pending);
        assert_eq!(mapping.reason.as_deref(), Some("同款同规格"));

        let active = SupplierProductMappingData {
            status: MappingStatus::Active,
            approved_by: Some(" buyer-1 ".to_string()),
            approved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..mapping_data()
        };
        let mapping = SupplierProductMapping::new(SupplierProductMappingId::new("spm-2"), active).unwrap();
        assert_eq!(mapping.approved_by.as_deref(), Some("buyer-1"));

        let active_without_approval = SupplierProductMappingData {
            status: MappingStatus::Active,
            ..mapping_data()
        };
        assert!(
            SupplierProductMapping::new(SupplierProductMappingId::new("spm-3"), active_without_approval)
                .is_err()
        );

        let half_pair = SupplierProductMappingData {
            status: MappingStatus::Conflict,
            approved_by: None,
            approved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..mapping_data()
        };
        assert!(SupplierProductMapping::new(SupplierProductMappingId::new("spm-4"), half_pair).is_err());
    }

    #[test]
    fn mapping_update_approves_and_disables() {
        let mut mapping =
            SupplierProductMapping::new(SupplierProductMappingId::new("spm-1"), mapping_data()).unwrap();
        mapping
            .update(
                MappingStatus::Active,
                Some("buyer-1".to_string()),
                Some(Instant::from_unix_secs(1_700_000_000)),
            )
            .unwrap();
        assert_eq!(mapping.status, MappingStatus::Active);

        assert!(mapping.update(MappingStatus::Active, None, None).is_err());

        mapping.update(MappingStatus::Disabled, None, None).unwrap();
        assert_eq!(mapping.status, MappingStatus::Disabled);
    }

    #[test]
    fn mapping_status_labels_and_codes() {
        assert_eq!(MappingStatus::Active.label(), "已生效");
        assert_eq!(MappingStatus::Conflict.as_str(), "CONFLICT");
        assert_eq!(
            serde_json::to_string(&MappingStatus::Disabled).unwrap(),
            "\"DISABLED\""
        );
    }
}
