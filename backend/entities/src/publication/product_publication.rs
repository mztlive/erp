//! `product_publication`：商品向商城发布的稳定身份（数据模型 §6.15，页面 W22）。
//!
//! 发布主表组合 [`crate::common::stable::StableBase`]：`status`（草稿、待发布、
//! 商城生效、暂停、失效）与 `current_revision_id`（当前商城生效版本）由字典定义；
//! `sku_id`/`target_mall_id` 为稳定键，创建后不可修改。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::ids::{ProductPublicationId, SkuId, SourceSystemId};
use crate::validation::normalize_required_text;

/// 当前生效版本引用最大长度。
const REVISION_ID_MAX_LEN: usize = 128;

/// 发布状态（数据模型 §6.15：草稿、待发布、商城生效、暂停、失效；
/// 固定枚举，无文档状态机，禁止运行时扩展）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductPublicationStatus {
    /// 草稿。
    #[default]
    Draft,
    /// 待发布。
    PendingPublish,
    /// 商城生效。
    MallEffective,
    /// 暂停。
    Paused,
    /// 失效。
    Expired,
}

impl ProductPublicationStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::PendingPublish => "待发布",
            Self::MallEffective => "商城生效",
            Self::Paused => "暂停",
            Self::Expired => "失效",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingPublish => "pending_publish",
            Self::MallEffective => "mall_effective",
            Self::Paused => "paused",
            Self::Expired => "expired",
        }
    }

    /// 返回安全暂停需要扫描的发布状态集合。
    ///
    /// # 返回
    /// 返回商城已生效与待发布状态；其它状态不属于当前在售影响集。
    pub fn safety_pause_candidates() -> &'static [Self] {
        &[Self::MallEffective, Self::PendingPublish]
    }

    /// 计算形成新发布修订后的稳定发布状态。
    ///
    /// # 参数
    /// * `has_safety_pause` - 是否已有不可逆安全暂停证据
    ///
    /// # 返回
    /// 有安全暂停证据时保持暂停，否则进入待发布。
    pub fn after_revision_submission(has_safety_pause: bool) -> Self {
        if has_safety_pause {
            Self::Paused
        } else {
            Self::PendingPublish
        }
    }
}

/// 商品发布创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationData {
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
    /// 发布状态。
    pub status: ProductPublicationStatus,
}

/// 商品发布更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductPublicationUpdate {
    /// 发布状态；`None` 表示不修改。
    pub status: Option<ProductPublicationStatus>,
    /// 当前商城生效版本；`None` 表示不修改。
    pub current_revision_id: Option<String>,
}

/// 商品发布实体（稳定发布主表，数据模型 §6.15）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct ProductPublication {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<ProductPublicationStatus>,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 目标商城（来源系统，类型 MALL）。
    pub target_mall_id: SourceSystemId,
}

impl PartialEq for ProductPublication {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.sku_id == other.sku_id
            && self.target_mall_id == other.target_mall_id
    }
}

impl Eq for ProductPublication {}

impl ProductPublication {
    /// 创建商品发布。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ProductPublicationId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的发布实体。
    pub fn new(
        id: ProductPublicationId,
        data: ProductPublicationData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            sku_id: data.sku_id,
            target_mall_id: data.target_mall_id,
        })
    }

    /// 更新商品发布。
    ///
    /// `sku_id`/`target_mall_id` 是稳定键（§6.15 `(sku_id, target_mall_id)` 唯一），
    /// 不允许在通用更新中修改。商城生效状态必须关联当前商城生效版本
    /// （§6.15「商城成功确认前不得把该版标记为商城已生效」，确认动作在 P3 校验）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当置为商城生效但缺少当前生效版本，或版本引用为空/超长时返回错误。
    pub fn update(&mut self, update: ProductPublicationUpdate, updated_by: impl Into<String>) -> Result<()> {
        self.apply_current_revision_id(update.current_revision_id)?;
        self.apply_status(update.status)?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断发布是否已在商城生效。
    ///
    /// # 返回
    /// 状态为 `MallEffective` 时返回 `true`。
    pub fn is_mall_effective(&self) -> bool {
        self.stable.status() == ProductPublicationStatus::MallEffective
    }

    /// 返回当前商城生效版本 ID。
    ///
    /// # 返回
    /// 已关联版本时返回其稳定 ID。
    ///
    /// # 错误
    /// 当前发布缺少商城生效版本时返回错误。
    pub fn current_revision_id(&self) -> Result<&str> {
        self.stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::from("发布缺少商城当前生效版本"))
    }

    /// 校验安全暂停后的人工更新不会恢复可下单状态或改写生效版本。
    ///
    /// # 参数
    /// * `update` - 待执行的发布更新
    ///
    /// # 返回
    /// 更新保持暂停且不改写生效版本时返回 `Ok(())`。
    ///
    /// # 错误
    /// 尝试离开暂停状态或改写当前生效版本时返回固定恢复责任错误。
    pub fn ensure_safety_pause_update_allowed(&self, update: &ProductPublicationUpdate) -> Result<()> {
        if update
            .status
            .is_some_and(|status| status != ProductPublicationStatus::Paused)
        {
            return Err(Error::from(
                "RECOVERY_RESPONSIBILITY_UNCONFIRMED：系统安全暂停发布禁止恢复为可下单状态",
            ));
        }
        if update.current_revision_id.is_some() {
            return Err(Error::from(
                "RECOVERY_RESPONSIBILITY_UNCONFIRMED：系统安全暂停发布禁止改写商城当前生效版本",
            ));
        }
        Ok(())
    }

    /// 把稳定发布推进为系统安全暂停状态。
    ///
    /// # 参数
    /// * `updated_by` - 固定系统操作人
    ///
    /// # 返回
    /// 状态更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 操作人或稳定发布数据非法时返回错误。
    pub fn mark_safety_paused(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.update(
            ProductPublicationUpdate {
                status: Some(ProductPublicationStatus::Paused),
                current_revision_id: None,
            },
            updated_by,
        )
    }

    /// 在形成新发布修订后推进稳定发布状态。
    ///
    /// # 参数
    /// * `has_safety_pause` - 是否已有系统安全暂停证据
    /// * `updated_by` - 本次提交操作人
    ///
    /// # 返回
    /// 安全暂停发布保持暂停，其余发布进入待发布。
    ///
    /// # 错误
    /// 操作人或稳定发布数据非法时返回错误。
    pub fn mark_revision_submitted(
        &mut self,
        has_safety_pause: bool,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        self.update(
            ProductPublicationUpdate {
                status: Some(ProductPublicationStatus::after_revision_submission(
                    has_safety_pause,
                )),
                current_revision_id: None,
            },
            updated_by,
        )
    }

    /// 应用当前生效版本更新。
    ///
    /// # 参数
    /// * `current_revision_id` - 可选当前商城生效版本
    ///
    /// # 错误
    /// 当版本引用为空或超长时返回错误。
    fn apply_current_revision_id(&mut self, current_revision_id: Option<String>) -> Result<()> {
        if let Some(current_revision_id) = current_revision_id {
            self.stable.current_revision_id = Some(normalize_required_text(
                current_revision_id,
                "当前生效版本不能为空",
                REVISION_ID_MAX_LEN,
                "当前生效版本过长",
            )?);
        }
        Ok(())
    }

    /// 应用发布状态更新。
    ///
    /// # 参数
    /// * `status` - 可选发布状态
    ///
    /// # 错误
    /// 当置为商城生效但尚未关联当前生效版本时返回错误。
    fn apply_status(&mut self, status: Option<ProductPublicationStatus>) -> Result<()> {
        if let Some(status) = status {
            if status == ProductPublicationStatus::MallEffective && self.stable.current_revision_id.is_none()
            {
                return Err(Error::from("商城生效状态必须关联当前商城生效版本"));
            }
            self.stable.status = status;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProductPublication, ProductPublicationData, ProductPublicationStatus, ProductPublicationUpdate,
    };
    use crate::ids::{ProductPublicationId, SkuId, SourceSystemId};

    fn publication_data() -> ProductPublicationData {
        ProductPublicationData {
            sku_id: SkuId::new("sku-1"),
            target_mall_id: SourceSystemId::new("mall-1"),
            status: ProductPublicationStatus::Draft,
        }
    }

    #[test]
    fn publication_new_builds_stable_base() {
        let publication =
            ProductPublication::new(ProductPublicationId::new("pub-1"), publication_data(), "admin-1")
                .unwrap();

        assert_eq!(publication.sku_id, SkuId::new("sku-1"));
        assert_eq!(publication.target_mall_id, SourceSystemId::new("mall-1"));
        assert_eq!(publication.stable.status(), ProductPublicationStatus::Draft);
        assert!(publication.stable.current_revision_id.is_none());
        assert_eq!(publication.stable.created_by, "admin-1");
        assert!(!publication.is_mall_effective());
    }

    #[test]
    fn publication_update_switches_status_and_keeps_stable_keys() {
        let mut publication =
            ProductPublication::new(ProductPublicationId::new("pub-1"), publication_data(), "admin-1")
                .unwrap();

        publication
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::PendingPublish),
                    current_revision_id: None,
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(
            publication.stable.status(),
            ProductPublicationStatus::PendingPublish
        );

        publication
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::MallEffective),
                    current_revision_id: Some("rev-1".to_string()),
                },
                "admin-2",
            )
            .unwrap();
        assert!(publication.is_mall_effective());
        assert_eq!(publication.stable.current_revision_id.as_deref(), Some("rev-1"));
        assert_eq!(publication.sku_id, SkuId::new("sku-1"), "稳定键不可修改");
        assert_eq!(publication.stable.updated_by, "admin-2");
        assert_eq!(publication.stable.created_by, "admin-1", "touch 不修改创建人");
    }

    #[test]
    fn publication_update_rejects_effective_without_revision() {
        let mut publication =
            ProductPublication::new(ProductPublicationId::new("pub-1"), publication_data(), "admin-1")
                .unwrap();

        let error = publication
            .update(
                ProductPublicationUpdate {
                    status: Some(ProductPublicationStatus::MallEffective),
                    current_revision_id: None,
                },
                "admin-2",
            )
            .unwrap_err();
        assert!(error.to_string().contains("商城生效状态必须关联当前商城生效版本"));
    }

    #[test]
    fn publication_update_rejects_blank_revision_reference() {
        let mut publication =
            ProductPublication::new(ProductPublicationId::new("pub-1"), publication_data(), "admin-1")
                .unwrap();

        assert!(publication
            .update(
                ProductPublicationUpdate {
                    status: None,
                    current_revision_id: Some("   ".to_string()),
                },
                "admin-2",
            )
            .is_err());
    }

    #[test]
    fn publication_status_serializes_with_stable_codes_and_exposes_labels() {
        assert_eq!(
            serde_json::to_string(&ProductPublicationStatus::MallEffective).unwrap(),
            "\"mall_effective\""
        );
        assert_eq!(ProductPublicationStatus::Expired.label(), "失效");
        assert_eq!(
            ProductPublicationStatus::PendingPublish.as_str(),
            "pending_publish"
        );
        assert_eq!(
            ProductPublicationStatus::safety_pause_candidates(),
            &[
                ProductPublicationStatus::MallEffective,
                ProductPublicationStatus::PendingPublish,
            ]
        );
        assert_eq!(
            ProductPublicationStatus::after_revision_submission(true),
            ProductPublicationStatus::Paused
        );
    }

    #[test]
    fn safety_pause_methods_keep_publication_paused_and_block_recovery() {
        let mut publication =
            ProductPublication::new(ProductPublicationId::new("pub-1"), publication_data(), "admin-1")
                .unwrap();
        publication.mark_safety_paused("system").unwrap();
        assert_eq!(publication.stable.status(), ProductPublicationStatus::Paused);
        assert!(publication
            .ensure_safety_pause_update_allowed(&ProductPublicationUpdate {
                status: Some(ProductPublicationStatus::PendingPublish),
                current_revision_id: None,
            })
            .is_err());
        assert!(publication
            .ensure_safety_pause_update_allowed(&ProductPublicationUpdate::default())
            .is_ok());

        publication.mark_revision_submitted(true, "admin-2").unwrap();
        assert_eq!(publication.stable.status(), ProductPublicationStatus::Paused);
    }
}
