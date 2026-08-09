//! `sku` SKU 稳定销售项身份（数据模型 §6.3，稳定主表）。
//!
//! `sku_no` 全局唯一、`(product_id, specification_signature)` 在全部生命周期记录上
//! 永久唯一（唯一约束跨行，属 P3/索引校验）；`specification_signature` 与
//! `base_unit_id` 创建后不可变，规格属性变化代表另一个 SKU。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::specification::validate_specification_signature;
use crate::catalog::status::{EnableStatus, ListingStatus};
use crate::common::stable::StableBase;
use crate::errors::Result;
use crate::ids::{ProductId, SkuId, UnitOfMeasureId};
use crate::validation::normalize_required_text;

/// SKU 编号最大长度。
const SKU_NO_MAX_LEN: usize = 64;

/// 上架概念引入前的 SKU 均视为延续原有可售行为，读取时兼容为已上架。
fn legacy_listing_status() -> ListingStatus {
    ListingStatus::Listed
}

/// SKU 创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuData {
    /// SKU 编号（业务编码，全局唯一；创建后不可修改）。
    pub sku_no: String,
    /// 所属 SPU。
    pub product_id: ProductId,
    /// 唯一基础单位。
    pub base_unit_id: UnitOfMeasureId,
    /// 规范化规格签名（由 [`crate::catalog::specification`] 计算，创建后不可变）。
    pub specification_signature: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 上架状态；新建 SKU 缺省应明确传入 `Unlisted`。
    pub listing_status: ListingStatus,
}

/// SKU 更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SkuUpdate {
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// SKU 实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Sku {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// SKU 编号（创建后不可修改）。
    pub sku_no: String,
    /// 所属 SPU。
    pub product_id: ProductId,
    /// 唯一基础单位（创建后不可修改）。
    pub base_unit_id: UnitOfMeasureId,
    /// 规范化规格签名（创建后不可变）。
    pub specification_signature: String,
    /// 上架状态；旧数据缺少字段时按原有可售行为兼容为已上架。
    #[serde(default = "legacy_listing_status")]
    pub listing_status: ListingStatus,
}

impl PartialEq for Sku {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sku_no == other.sku_no
            && self.product_id == other.product_id
            && self.base_unit_id == other.base_unit_id
            && self.specification_signature == other.specification_signature
            && self.listing_status == other.listing_status
    }
}

impl Eq for Sku {}

impl Sku {
    /// 创建 SKU。
    ///
    /// 完成 sku_no 的校验与规范化（去首尾空白、非空、长度上限），并校验
    /// `specification_signature` 是规范化形态（空签名必须为固定空规格签名）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SkuId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的 SKU 实体。
    ///
    /// # 错误
    /// 当 sku_no 为空/超长、规格签名不是规范化形态，或停用 SKU 被标记为
    /// 已上架时返回错误。
    pub fn new(id: SkuId, data: SkuData, created_by: impl Into<String>) -> Result<Self> {
        let sku_no = normalize_required_text(data.sku_no, "SKU编号不能为空", SKU_NO_MAX_LEN, "SKU编号过长")?;
        let signature = data.specification_signature.trim().to_string();
        validate_specification_signature(&signature)?;
        if data.listing_status.is_listed() && !data.status.is_active() {
            return Err("停用的 SKU 不能上架".into());
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            sku_no,
            product_id: data.product_id,
            base_unit_id: data.base_unit_id,
            specification_signature: signature,
            listing_status: data.listing_status,
        })
    }

    /// 更新 SKU。
    ///
    /// `sku_no`、`product_id`、`base_unit_id` 与 `specification_signature` 是
    /// 创建后不可变的稳定身份，通用更新只允许修改启停状态。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, update: SkuUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(status) = update.status {
            self.stable.status = status;
            if !status.is_active() {
                self.listing_status = ListingStatus::Unlisted;
            }
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断 SKU 是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 切换 SKU 上架状态。
    ///
    /// 停用 SKU 不允许上架；幂等提交不会增加实体版本。
    ///
    /// # 参数
    /// * `listing_status` - 目标上架状态
    /// * `updated_by` - 本次操作人
    ///
    /// # 返回
    /// 状态发生变化时返回 `true`，幂等提交返回 `false`。
    ///
    /// # 错误
    /// 停用 SKU 尝试上架时返回领域错误。
    pub fn set_listing_status(
        &mut self,
        listing_status: ListingStatus,
        updated_by: impl Into<String>,
    ) -> Result<bool> {
        if listing_status.is_listed() && !self.is_active() {
            return Err("停用的 SKU 不能上架".into());
        }
        if self.listing_status == listing_status {
            return Ok(false);
        }
        self.listing_status = listing_status;
        self.stable.touch(updated_by);
        Ok(true)
    }

    /// 判断是否为无规格 SKU。
    ///
    /// # 返回
    /// 规格签名为固定空规格签名时返回 `true`。
    pub fn is_no_spec(&self) -> bool {
        self.specification_signature == crate::catalog::specification::EMPTY_SPEC_SIGNATURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::specification::EMPTY_SPEC_SIGNATURE;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::SkuId;

    fn data() -> SkuData {
        SkuData {
            sku_no: " SKU-2025-001 ".to_string(),
            product_id: ProductId::new("prod-1"),
            base_unit_id: UnitOfMeasureId::new("uom-1"),
            specification_signature: "size=L|color=红色".to_string(),
            status: EnableStatus::Active,
            listing_status: ListingStatus::Unlisted,
        }
    }

    /// happy path：编号 trim 规范化，签名与归属落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();

        assert_eq!(sku.sku_no, "SKU-2025-001");
        assert_eq!(sku.product_id, ProductId::new("prod-1"));
        assert_eq!(sku.base_unit_id, UnitOfMeasureId::new("uom-1"));
        assert_eq!(sku.specification_signature, "size=L|color=红色");
        assert!(!sku.is_no_spec());
        assert!(sku.is_active());
        assert_eq!(sku.listing_status, ListingStatus::Unlisted);
    }

    /// happy path：无规格 SKU 使用固定空规格签名，空白输入规范化为空签名。
    #[test]
    fn new_accepts_empty_spec_signature() {
        let no_spec = SkuData {
            specification_signature: EMPTY_SPEC_SIGNATURE.to_string(),
            ..data()
        };
        let sku = Sku::new(SkuId::new("sku-1"), no_spec, "admin-1").unwrap();
        assert!(sku.is_no_spec());

        let blank = SkuData {
            specification_signature: "   ".to_string(),
            ..data()
        };
        let sku = Sku::new(SkuId::new("sku-2"), blank, "admin-1").unwrap();
        assert!(sku.is_no_spec());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_sku_no() {
        let empty = SkuData {
            sku_no: "  ".to_string(),
            ..data()
        };
        assert!(Sku::new(SkuId::new("sku-1"), empty, "admin-1").is_err());

        let overlong = SkuData {
            sku_no: "s".repeat(65),
            ..data()
        };
        assert!(Sku::new(SkuId::new("sku-1"), overlong, "admin-1").is_err());
    }

    /// 失败路径：签名超长被拒绝。
    #[test]
    fn new_rejects_overlong_signature() {
        let overlong_signature = SkuData {
            specification_signature: format!("{}={}", "a".repeat(100), "b".repeat(500)),
            ..data()
        };
        assert!(Sku::new(SkuId::new("sku-1"), overlong_signature, "admin-1").is_err());
    }

    /// update 只允许改状态：编号、签名与基础单位保持不变。
    #[test]
    fn update_only_changes_status_and_preserves_identity() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();

        sku.update(
            SkuUpdate {
                status: Some(EnableStatus::Disabled),
            },
            "admin-2",
        )
        .unwrap();

        assert!(!sku.is_active());
        assert_eq!(sku.listing_status, ListingStatus::Unlisted);
        assert_eq!(sku.sku_no, "SKU-2025-001");
        assert_eq!(sku.base_unit_id, UnitOfMeasureId::new("uom-1"));
        assert_eq!(sku.specification_signature, "size=L|color=红色");
        assert_eq!(sku.stable.updated_by, "admin-2");
    }

    /// 上架状态独立切换，幂等提交不触碰版本。
    #[test]
    fn listing_status_changes_independently_and_idempotently() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();

        assert!(sku.set_listing_status(ListingStatus::Listed, "admin-2").unwrap());
        let version = sku.base.version;
        assert!(sku.listing_status.is_listed());
        assert!(!sku.set_listing_status(ListingStatus::Listed, "admin-3").unwrap());
        assert_eq!(sku.base.version, version);
        assert_eq!(sku.stable.updated_by, "admin-2");
    }

    /// 停用 SKU 自动下架，且不能在停用状态下重新上架。
    #[test]
    fn disabled_sku_is_unlisted_and_cannot_be_listed() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();
        sku.set_listing_status(ListingStatus::Listed, "admin-2").unwrap();
        sku.update(
            SkuUpdate {
                status: Some(EnableStatus::Disabled),
            },
            "admin-3",
        )
        .unwrap();

        assert_eq!(sku.listing_status, ListingStatus::Unlisted);
        assert!(sku.set_listing_status(ListingStatus::Listed, "admin-4").is_err());
    }

    /// 构造阶段拒绝“停用但已上架”的非法组合。
    #[test]
    fn new_rejects_disabled_but_listed_sku() {
        let invalid = SkuData {
            status: EnableStatus::Disabled,
            listing_status: ListingStatus::Listed,
            ..data()
        };

        assert!(Sku::new(SkuId::new("sku-1"), invalid, "admin-1").is_err());
    }

    /// 上架概念引入前的旧文档缺字段时保持原有可售行为。
    #[test]
    fn legacy_document_without_listing_status_is_treated_as_listed() {
        let sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();
        let mut value = serde_json::to_value(sku).unwrap();
        value.as_object_mut().unwrap().remove("listing_status");

        let restored: Sku = serde_json::from_value(value).unwrap();

        assert_eq!(restored.listing_status, ListingStatus::Listed);
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
