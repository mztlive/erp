//! `sku` SKU 稳定销售项身份（数据模型 §6.3，稳定主表）。
//!
//! `sku_no` 全局唯一、`(product_id, specification_signature)` 在全部生命周期记录上
//! 永久唯一（唯一约束跨行，属 P3/索引校验）；`specification_signature` 与
//! `base_unit_id` 创建后不可变，规格属性变化代表另一个 SKU。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::sku_revision::SkuRevision;
use crate::catalog::specification::validate_specification_signature;
use crate::catalog::status::{EnableStatus, ListingStatus};
use crate::common::stable::StableBase;
use crate::errors::Result;
use crate::ids::{ProductId, SkuId, SkuRevisionId, UnitOfMeasureId};
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

/// 规格编辑对 SKU 稳定身份的提交快照。
#[derive(Debug, Clone, Copy)]
pub struct SkuEditIdentity<'a> {
    /// 客户端提交的稳定 SKU ID；新增规格必须为空。
    pub sku_id: Option<&'a SkuId>,
    /// 客户端读取时看到的当前 SKU 修订 ID；新增规格必须为空。
    pub expected_revision_id: Option<&'a SkuRevisionId>,
    /// 客户端提交的稳定 SKU 编号。
    pub sku_no: &'a str,
    /// 客户端提交的稳定基础单位。
    pub base_unit_id: &'a UnitOfMeasureId,
    /// 是否明确请求重新启用历史停用 SKU。
    pub reenable: bool,
    /// 重新启用原因；仅重新启用时必填。
    pub change_reason: Option<&'a str>,
}

impl SkuEditIdentity<'_> {
    /// 校验该身份快照可用于创建全新规格 SKU。
    ///
    /// # 参数
    /// 无；使用值对象中携带的客户端提交字段。
    ///
    /// # 返回
    /// 未指定既有身份、期望修订和重新启用意图时返回 `Ok(())`。
    ///
    /// # 错误
    /// 新规格猜测既有 SKU 身份或提交重新启用意图时返回身份规则错误。
    pub fn ensure_new(&self) -> std::result::Result<(), SkuEditIdentityError> {
        if self.sku_id.is_some() || self.expected_revision_id.is_some() || self.reenable {
            return Err(SkuEditIdentityError::NewIdentitySpecified);
        }
        Ok(())
    }
}

/// 规格编辑对单个 SKU 的生命周期动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkuEditAction {
    /// 全新规格签名，创建新的稳定 SKU。
    Create,
    /// 保留当前启用 SKU 并追加修订。
    Keep,
    /// 复用历史停用 SKU 并重新启用。
    Reactivate,
}

/// SKU 规格编辑身份规则错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SkuEditIdentityError {
    /// 新规格不得携带任何既有身份信息。
    #[error("新增规格签名不得指定或猜测既有 SKU 身份")]
    NewIdentitySpecified,
    /// 既有规格必须携带匹配的稳定身份。
    #[error("既有规格行必须携带匹配的稳定 sku_id")]
    SkuIdMismatch,
    /// 当前修订已经变化。
    #[error("SKU 修订已变化，请刷新商品后重试")]
    RevisionConflict,
    /// SKU 编号或基础单位被修改。
    #[error("SKU 编码和基础单位为稳定身份字段，编辑时不得修改")]
    StableIdentityChanged,
    /// 历史停用 SKU 缺少明确重新启用意图或原因。
    #[error("重新启用历史停用 SKU 必须明确 reenable=true 并填写 change_reason")]
    ReactivationIntentRequired,
    /// 当前启用 SKU 不接受重新启用意图。
    #[error("当前启用 SKU 不得提交重新启用意图")]
    UnexpectedReactivationIntent,
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

    /// 根据稳定身份、当前修订和重新启用意图分类既有 SKU 编辑动作。
    ///
    /// # 参数
    /// * `identity` - 客户端提交的 SKU 稳定身份快照
    ///
    /// # 返回
    /// 当前启用 SKU 返回 `Keep`，历史停用且明确重新启用返回 `Reactivate`。
    ///
    /// # 错误
    /// SKU ID 或期望修订不匹配、稳定字段变化、重新启用意图缺失或多余时返回
    /// [`SkuEditIdentityError`]。
    pub fn classify_edit(
        &self,
        identity: &SkuEditIdentity<'_>,
    ) -> std::result::Result<SkuEditAction, SkuEditIdentityError> {
        if identity.sku_id.map(|id| id.as_ref()) != Some(self.base.id.as_str()) {
            return Err(SkuEditIdentityError::SkuIdMismatch);
        }
        if identity.expected_revision_id.map(|id| id.as_ref()) != self.stable.current_revision_id.as_deref() {
            return Err(SkuEditIdentityError::RevisionConflict);
        }
        if identity.sku_no.trim() != self.sku_no || identity.base_unit_id != &self.base_unit_id {
            return Err(SkuEditIdentityError::StableIdentityChanged);
        }
        if self.is_active() {
            if identity.reenable {
                return Err(SkuEditIdentityError::UnexpectedReactivationIntent);
            }
            return Ok(SkuEditAction::Keep);
        }
        if !identity.reenable || identity.change_reason.map(str::trim).is_none_or(str::is_empty) {
            return Err(SkuEditIdentityError::ReactivationIntentRequired);
        }
        Ok(SkuEditAction::Reactivate)
    }

    /// 关联一份属于本 SKU 的新当前修订。
    ///
    /// # 参数
    /// * `revision` - 待设为当前版本的不可变 SKU 修订
    /// * `updated_by` - 本次关联操作人
    ///
    /// # 返回
    /// 关联成功返回 `Ok(())`，并同步启停状态、下架约束与当前修订指针。
    ///
    /// # 错误
    /// 修订属于其他 SKU，或同步后的状态违反 SKU 不变式时返回领域错误。
    pub fn attach_revision(&mut self, revision: &SkuRevision, updated_by: impl Into<String>) -> Result<()> {
        if revision.sku_id.as_ref() != self.base.id.as_str() {
            return Err("SKU 修订不属于目标 SKU".into());
        }
        self.update(
            SkuUpdate {
                status: Some(revision.status),
            },
            updated_by,
        )?;
        self.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(())
    }

    /// 停用 SKU 并自动下架。
    ///
    /// # 参数
    /// * `updated_by` - 本次停用操作人
    ///
    /// # 返回
    /// 返回 `Ok(())`；重复停用保持幂等状态并刷新操作人。
    ///
    /// # 错误
    /// 当前更新规则失败时返回领域错误。
    pub fn disable(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.update(
            SkuUpdate {
                status: Some(EnableStatus::Disabled),
            },
            updated_by,
        )
    }

    /// 重新启用历史停用 SKU，保持下架状态等待显式上架。
    ///
    /// # 参数
    /// * `updated_by` - 本次重新启用操作人
    ///
    /// # 返回
    /// SKU 从停用切回启用时返回 `Ok(())`。
    ///
    /// # 错误
    /// SKU 已经启用时返回领域错误，防止把普通编辑误标为重新启用。
    pub fn reactivate(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.is_active() {
            return Err("当前 SKU 已经启用".into());
        }
        self.update(
            SkuUpdate {
                status: Some(EnableStatus::Active),
            },
            updated_by,
        )
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

    /// 新规格不得猜测既有身份，既有 SKU 按状态和意图分类编辑动作。
    #[test]
    fn edit_identity_rules_classify_keep_and_reactivate() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();
        sku.stable.current_revision_id = Some("rev-1".to_string());
        let sku_id = SkuId::new("sku-1");
        let revision_id = SkuRevisionId::new("rev-1");
        let base_unit_id = sku.base_unit_id.clone();
        let identity = SkuEditIdentity {
            sku_id: Some(&sku_id),
            expected_revision_id: Some(&revision_id),
            sku_no: " SKU-2025-001 ",
            base_unit_id: &base_unit_id,
            reenable: false,
            change_reason: None,
        };

        assert_eq!(sku.classify_edit(&identity).unwrap(), SkuEditAction::Keep);
        assert!(identity.ensure_new().is_err());
        let new_identity = SkuEditIdentity {
            sku_id: None,
            expected_revision_id: None,
            reenable: false,
            ..identity
        };
        assert!(new_identity.ensure_new().is_ok());

        sku.disable("admin-2").unwrap();
        let reactivation = SkuEditIdentity {
            reenable: true,
            change_reason: Some(" 恢复销售 "),
            ..identity
        };
        assert_eq!(
            sku.classify_edit(&reactivation).unwrap(),
            SkuEditAction::Reactivate
        );
    }

    /// 既有 SKU 修订冲突与缺失重新启用原因均被拒绝。
    #[test]
    fn edit_identity_rules_reject_stale_or_ambiguous_intent() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();
        sku.stable.current_revision_id = Some("rev-2".to_string());
        let sku_id = SkuId::new("sku-1");
        let stale_revision = SkuRevisionId::new("rev-1");
        let sku_no = sku.sku_no.clone();
        let base_unit_id = sku.base_unit_id.clone();
        let stale = SkuEditIdentity {
            sku_id: Some(&sku_id),
            expected_revision_id: Some(&stale_revision),
            sku_no: &sku_no,
            base_unit_id: &base_unit_id,
            reenable: false,
            change_reason: None,
        };
        assert_eq!(
            sku.classify_edit(&stale).unwrap_err(),
            SkuEditIdentityError::RevisionConflict
        );

        sku.disable("admin-2").unwrap();
        let current_revision = SkuRevisionId::new("rev-2");
        let missing_reason = SkuEditIdentity {
            expected_revision_id: Some(&current_revision),
            reenable: true,
            change_reason: Some("   "),
            ..stale
        };
        assert_eq!(
            sku.classify_edit(&missing_reason).unwrap_err(),
            SkuEditIdentityError::ReactivationIntentRequired
        );
    }

    /// SKU 只接受自身修订并同步当前修订指针。
    #[test]
    fn attach_revision_enforces_relationship() {
        let mut sku = Sku::new(SkuId::new("sku-1"), data(), "admin-1").unwrap();
        let revision = SkuRevision::new(
            SkuRevisionId::new("rev-1"),
            crate::catalog::sku_revision::SkuRevisionData {
                sku_id: SkuId::new("sku-1"),
                revision_no: 1,
                name: "SKU".to_string(),
                description: None,
                specification: None,
                barcode: None,
                source_main_image_asset_id: None,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: None,
                market_price: None,
                status: EnableStatus::Active,
                effective_from: crate::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();
        sku.attach_revision(&revision, "admin-2").unwrap();
        assert_eq!(sku.stable.current_revision_id.as_deref(), Some("rev-1"));

        let foreign = SkuRevision::new(
            SkuRevisionId::new("rev-2"),
            crate::catalog::sku_revision::SkuRevisionData {
                sku_id: SkuId::new("sku-2"),
                revision_no: 1,
                name: "SKU".to_string(),
                description: None,
                specification: None,
                barcode: None,
                source_main_image_asset_id: None,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: None,
                market_price: None,
                status: EnableStatus::Active,
                effective_from: crate::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();
        assert!(sku.attach_revision(&foreign, "admin-3").is_err());
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
