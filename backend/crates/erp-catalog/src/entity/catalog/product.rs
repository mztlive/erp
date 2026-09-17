//! `product` 商品 SPU 稳定身份（数据模型 §6.3，稳定主表）。
//!
//! `product_kind` 是必填的独立稳定业务属性，创建后不可变、不得由分类派生；
//! `product_no` 全局唯一（唯一约束跨行，属 P3/索引校验）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::Result;
use erp_core::common::stable::StableBase;
use erp_core::ids::ProductId;
use erp_core::validation::normalize_required_text;
use serde::{Deserialize, Serialize};

use crate::entity::catalog::product_kind::ProductKind;
use crate::entity::catalog::product_revision::ProductRevision;
use crate::entity::catalog::status::EnableStatus;

/// 商品编号最大长度。
const PRODUCT_NO_MAX_LEN: usize = 64;

/// 商品创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductData {
    /// 商品编号（全局唯一，创建后不可修改）。
    pub product_no: String,
    /// 商品业务类型（创建后不可变，不得由分类派生）。
    pub product_kind: ProductKind,
    /// 启停状态。
    pub status: EnableStatus,
    /// 商品维护人；禁止用创建人兜底。
    pub maintainer_user_id: String,
    /// 维护人主属内部组织；缺主属组织不得创建。
    pub business_org_unit_id: String,
}

/// 商品更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ProductUpdate {
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 商品 SPU 实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Product {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 商品编号（创建后不可修改）。
    pub product_no: String,
    /// 商品业务类型（创建后不可变）。
    pub product_kind: ProductKind,
    /// 当前维护人；交接前不得为空。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 当前业务组织；交接前不得为空。
    #[serde(default)]
    pub business_org_unit_id: String,
}

impl PartialEq for Product {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.product_no == other.product_no
            && self.product_kind == other.product_kind
            && self.maintainer_user_id == other.maintainer_user_id
            && self.business_org_unit_id == other.business_org_unit_id
    }
}

impl Eq for Product {}

impl Product {
    /// 创建商品 SPU。
    ///
    /// 完成 product_no 的校验与规范化（去首尾空白、非空、长度上限）；
    /// `product_kind` 必须显式提交并永久保持不变（数据模型 §6.3）。
    /// 维护人与主属组织创建必填，禁止用 `created_by` 回填。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::ProductId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的商品实体。
    ///
    /// # 错误
    /// 当 product_no、维护人或业务组织为空或超长时返回错误。
    pub fn new(id: ProductId, data: ProductData, created_by: impl Into<String>) -> Result<Self> {
        let product_no =
            normalize_required_text(data.product_no, "商品编号不能为空", PRODUCT_NO_MAX_LEN, "商品编号过长")?;
        let maintainer_user_id =
            normalize_required_text(data.maintainer_user_id, "商品维护人不能为空", 128, "商品维护人过长")?;
        let business_org_unit_id = normalize_required_text(
            data.business_org_unit_id,
            "维护人缺少有效主属组织，请先维护组织成员关系",
            128,
            "商品业务组织过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            product_no,
            product_kind: data.product_kind,
            maintainer_user_id,
            business_org_unit_id,
        })
    }

    /// 更新商品。
    ///
    /// `product_no` 与 `product_kind` 是创建后不可变的稳定身份，
    /// 通用更新只允许修改启停状态。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, update: ProductUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断商品是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 返回商品的业务类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回创建时固定的 `ProductKind`。
    ///
    /// # 错误
    /// 无。
    pub fn product_kind(&self) -> ProductKind {
        self.product_kind
    }

    /// 判断当前乐观锁版本是否与客户端期望一致。
    ///
    /// # 参数
    /// * `expected` - 客户端读取商品时看到的版本
    ///
    /// # 返回
    /// 当前版本与期望版本完全一致时返回 `true`。
    ///
    /// # 错误
    /// 无；Service 根据 `false` 映射为稳定的并发冲突语义。
    pub fn has_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 关联一份属于本商品的新当前修订。
    ///
    /// # 参数
    /// * `revision` - 待设为当前版本的不可变商品修订
    /// * `updated_by` - 本次关联操作人
    ///
    /// # 返回
    /// 关联成功返回 `Ok(())`，并同步商品状态与当前修订指针。
    ///
    /// # 错误
    /// 修订属于其他商品时返回领域错误，阻止跨商品挂接修订。
    pub fn attach_revision(
        &mut self,
        revision: &ProductRevision,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if revision.product_id.as_ref() != self.base.id.as_str() {
            return Err("商品修订不属于目标商品".into());
        }
        self.stable.current_revision_id = Some(revision.base.id.clone());
        self.stable.status = revision.status;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 停用当前处于启用状态的商品。
    ///
    /// # 参数
    /// * `updated_by` - 本次停用操作人
    ///
    /// # 返回
    /// 状态由启用切换为停用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 商品已经停用时返回领域错误，避免重复生成停用修订。
    pub fn disable(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if !self.is_active() {
            return Err("商品已经停用".into());
        }
        self.update(ProductUpdate { status: Some(EnableStatus::Disabled) }, updated_by)
    }

    /// 判断商品是否已具备可查询、可交接的维护责任事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 维护人与业务组织均非空时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 空维护人不得用创建人顶替；写命令须在交接前阻断。
    pub fn has_responsibility(&self) -> bool {
        !self.maintainer_user_id.trim().is_empty() && !self.business_org_unit_id.trim().is_empty()
    }

    /// 显式交接商品维护人与可选业务组织。
    ///
    /// 业务组织不随接收人部门隐式变化；`None` 表示保留原组织。
    ///
    /// # 参数
    /// * `target_user_id` - 目标维护人
    /// * `target_org_unit_id` - 显式目标组织；`None` 保留原组织
    /// * `updated_by` - 本次交接执行人
    ///
    /// # 返回
    /// 责任确有变化时返回 `true`。
    ///
    /// # 错误
    /// 目标为空或与当前完全一致时拒绝。
    ///
    /// # 关键业务约束
    /// 只改维护人与业务组织，不改创建人、SKU 或审批任务。
    pub fn handover(
        &mut self,
        target_user_id: String,
        target_org_unit_id: Option<String>,
        updated_by: impl Into<String>,
    ) -> Result<bool> {
        let target = normalize_required_text(target_user_id, "目标维护人不能为空", 128, "目标维护人过长")?;
        let next_org = match target_org_unit_id {
            Some(org) => normalize_required_text(org, "目标业务组织不能为空", 128, "目标业务组织过长")?,
            None => self.business_org_unit_id.clone(),
        };
        if next_org.is_empty() {
            return Err("维护人缺少有效主属组织，请先维护组织成员关系".into());
        }
        if target == self.maintainer_user_id && next_org == self.business_org_unit_id {
            return Err("目标已是当前维护人，无需交接".into());
        }
        self.maintainer_user_id = target;
        self.business_org_unit_id = next_org;
        self.stable.touch(updated_by);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};
    use erp_core::ids::ProductId;

    use super::*;

    fn data() -> ProductData {
        ProductData {
            product_no: " P-2025-001 ".to_string(),
            product_kind: ProductKind::Physical,
            status: EnableStatus::Active,
            maintainer_user_id: "user-1".to_string(),
            business_org_unit_id: "org-1".to_string(),
        }
    }

    /// happy path：编号 trim 规范化，商品类型与状态落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let product = Product::new(ProductId::new("prod-1"), data(), "admin-1").unwrap();

        assert_eq!(product.product_no, "P-2025-001");
        assert_eq!(product.product_kind(), ProductKind::Physical);
        assert!(product.is_active());
        assert_eq!(product.stable.created_by, "admin-1");
        assert_eq!(product.maintainer_user_id, "user-1");
        assert_eq!(product.business_org_unit_id, "org-1");
        assert!(product.has_responsibility());
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_product_no() {
        let empty = ProductData { product_no: "  ".to_string(), ..data() };
        assert!(Product::new(ProductId::new("prod-1"), empty, "admin-1").is_err());

        let overlong = ProductData { product_no: "p".repeat(65), ..data() };
        assert!(Product::new(ProductId::new("prod-1"), overlong, "admin-1").is_err());
    }

    /// 创建缺维护人或主属组织必须阻断，不得用创建人兜底。
    #[test]
    fn new_rejects_missing_maintainer_and_org() {
        let missing_owner = ProductData { maintainer_user_id: "  ".to_string(), ..data() };
        assert!(Product::new(ProductId::new("prod-1"), missing_owner, "admin-1").is_err());
        let missing_org = ProductData { business_org_unit_id: String::new(), ..data() };
        assert!(Product::new(ProductId::new("prod-1"), missing_org, "admin-1").is_err());
    }

    /// 历史空维护人不得用创建人顶替，须通过显式交接补齐责任。
    #[test]
    fn handover_repairs_empty_responsibility_without_created_by() {
        let mut product: Product = serde_json::from_value(serde_json::json!({
            "id": "prod-legacy",
            "created_at": 1,
            "updated_at": 1,
            "deleted_at": 0,
            "version": 1,
            "status": "active",
            "current_revision_id": null,
            "created_by": "admin-1",
            "updated_by": "admin-1",
            "product_no": "P-LEGACY",
            "product_kind": "PHYSICAL",
            "maintainer_user_id": "",
            "business_org_unit_id": ""
        }))
        .unwrap();
        assert!(!product.has_responsibility());
        assert_eq!(product.stable.created_by, "admin-1");
        assert!(product.handover("user-2".into(), None, "admin-2").is_err());
        assert!(product.handover("user-2".into(), Some("org-2".into()), "admin-2").unwrap());
        assert_eq!(product.maintainer_user_id, "user-2");
        assert_eq!(product.business_org_unit_id, "org-2");
        assert_ne!(product.maintainer_user_id, product.stable.created_by);
        assert!(product.has_responsibility());
    }

    /// 交接推进维护人；省略组织时保留原业务组织，同人同组织拒绝。
    #[test]
    fn handover_updates_maintainer_and_optional_org() {
        let mut product = Product::new(ProductId::new("prod-1"), data(), "admin-1").unwrap();
        assert!(product.handover("user-2".into(), None, "admin-2").unwrap());
        assert_eq!(product.maintainer_user_id, "user-2");
        assert_eq!(product.business_org_unit_id, "org-1");
        assert_eq!(product.stable.updated_by, "admin-2");
        assert!(product.handover("user-2".into(), None, "admin-3").is_err());
        assert!(product.handover("user-3".into(), Some("org-2".into()), "admin-4").unwrap());
        assert_eq!(product.business_org_unit_id, "org-2");
        assert!(product.handover("user-3".into(), Some("  ".into()), "admin-5").is_err());
    }

    /// update 只允许改状态：编号与商品类型保持不变。
    #[test]
    fn update_only_changes_status_and_preserves_identity() {
        let mut product = Product::new(ProductId::new("prod-1"), data(), "admin-1").unwrap();

        product.update(ProductUpdate { status: Some(EnableStatus::Disabled) }, "admin-2").unwrap();

        assert!(!product.is_active());
        assert_eq!(product.product_no, "P-2025-001");
        assert_eq!(product.product_kind(), ProductKind::Physical);
        assert_eq!(product.stable.updated_by, "admin-2");
    }

    /// 当前版本匹配、修订挂接和停用均由商品实体保护。
    #[test]
    fn version_revision_and_disable_rules_are_enforced() {
        let mut product = Product::new(ProductId::new("prod-1"), data(), "admin-1").unwrap();
        let revision = ProductRevision::new(
            erp_core::ids::ProductRevisionId::new("rev-1"),
            crate::entity::catalog::product_revision::ProductRevisionData {
                product_id: ProductId::new("prod-1"),
                revision_no: 1,
                name: "商品".to_string(),
                description: None,
                specification: None,
                category_id: erp_core::ids::ProductCategoryId::new("cat-1"),
                brand_id: erp_core::ids::ProductBrandId::new("brand-1"),
                status: EnableStatus::Active,
                effective_from: erp_core::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();

        assert!(product.has_version(1));
        assert!(!product.has_version(2));
        product.attach_revision(&revision, "admin-2").unwrap();
        assert_eq!(product.stable.current_revision_id.as_deref(), Some("rev-1"));
        product.disable("admin-3").unwrap();
        assert!(!product.is_active());
        assert!(product.disable("admin-4").is_err());
    }

    /// 商品拒绝挂接属于其他稳定身份的修订。
    #[test]
    fn attach_revision_rejects_foreign_product_revision() {
        let mut product = Product::new(ProductId::new("prod-1"), data(), "admin-1").unwrap();
        let revision = ProductRevision::new(
            erp_core::ids::ProductRevisionId::new("rev-1"),
            crate::entity::catalog::product_revision::ProductRevisionData {
                product_id: ProductId::new("prod-2"),
                revision_no: 1,
                name: "商品".to_string(),
                description: None,
                specification: None,
                category_id: erp_core::ids::ProductCategoryId::new("cat-1"),
                brand_id: erp_core::ids::ProductBrandId::new("brand-1"),
                status: EnableStatus::Active,
                effective_from: erp_core::common::time::BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();

        assert!(product.attach_revision(&revision, "admin-2").is_err());
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
