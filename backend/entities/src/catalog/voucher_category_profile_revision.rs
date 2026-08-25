//! `voucher_category_profile_revision` 卡券类目最小 ERP 扩展（数据模型 §6.3、§4.4）。
//!
//! 卡券类目使用 `product_kind = VOUCHER` 的 SKU 身份；本修订只保存 ERP 必需的
//! 卡券类目描述和启停信息，不保存限额、限购、绑定验证、补差限制、充值、过期展示等
//! 商城玩法（数据模型 §6.3 第 869-871 行）。修订一经形成不得修改，
//! 本实体不提供 `update()`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::revision::RevisionBase;
use crate::errors::{Error, Result};
use crate::ids::{SkuId, VoucherCategoryProfileRevisionId};
use crate::validation::normalize_required_text;

/// 卡券类目描述最大长度。
const DESCRIPTION_MAX_LEN: usize = 512;

/// 卡券类目扩展修订创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoucherCategoryProfileRevisionData {
    /// 卡券类目使用的 VOUCHER SKU 稳定身份。
    pub sku_id: SkuId,
    /// 修订序号（同一类目内从 1 递增）。
    pub revision_no: u32,
    /// 卡券类目描述。
    pub description: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 卡券类目扩展修订实体（不可变修订，数据模型 §6.3、§4.4）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct VoucherCategoryProfileRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 卡券类目使用的 VOUCHER SKU 稳定身份。
    pub sku_id: SkuId,
    /// 卡券类目描述。
    pub description: String,
    /// 启停状态。
    pub status: EnableStatus,
}

impl VoucherCategoryProfileRevision {
    /// 创建卡券类目扩展修订。
    ///
    /// 完成 description 的校验与规范化（去首尾空白、非空、长度上限），
    /// 并校验修订序号从 1 开始。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::VoucherCategoryProfileRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的卡券类目扩展修订实体。
    ///
    /// # 错误
    /// 当 description 为空/超长或 revision_no 为 0 时返回错误。
    pub fn new(
        id: VoucherCategoryProfileRevisionId,
        data: VoucherCategoryProfileRevisionData,
    ) -> Result<Self> {
        let description = normalize_required_text(
            data.description,
            "卡券类目描述不能为空",
            DESCRIPTION_MAX_LEN,
            "卡券类目描述过长",
        )?;
        if data.revision_no == 0 {
            return Err(Error::from("修订序号必须从 1 开始"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            sku_id: data.sku_id,
            description,
            status: data.status,
        })
    }

    /// 从当前快照派生一份描述变更的后继修订。
    ///
    /// SKU 稳定身份与启停状态保持不变，只替换描述并使用调用方提供的下一序号。
    ///
    /// # 参数
    /// * `id` - 新扩展修订主键
    /// * `revision_no` - 同一卡券类目内的下一修订序号
    /// * `description` - 新卡券类目描述
    ///
    /// # 返回
    /// 返回经完整实体校验的新扩展修订。
    ///
    /// # 错误
    /// 描述或修订序号违反实体不变式时返回错误。
    pub fn content_successor(
        &self,
        id: VoucherCategoryProfileRevisionId,
        revision_no: u32,
        description: String,
    ) -> Result<Self> {
        Self::new(
            id,
            VoucherCategoryProfileRevisionData {
                sku_id: self.sku_id.clone(),
                revision_no,
                description,
                status: self.status,
            },
        )
    }

    /// 判断修订是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::VoucherCategoryProfileRevisionId;

    fn data() -> VoucherCategoryProfileRevisionData {
        VoucherCategoryProfileRevisionData {
            sku_id: SkuId::new("sku-voucher-1"),
            revision_no: 1,
            description: " 中国通卡券类目 ".to_string(),
            status: EnableStatus::Active,
        }
    }

    /// happy path：描述 trim 规范化，SKU 身份与修订序号落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let revision =
            VoucherCategoryProfileRevision::new(VoucherCategoryProfileRevisionId::new("vcp-1"), data())
                .unwrap();

        assert_eq!(revision.description, "中国通卡券类目");
        assert_eq!(revision.sku_id, SkuId::new("sku-voucher-1"));
        assert_eq!(revision.revision.revision_no, 1);
        assert!(revision.is_active());
    }

    /// 失败路径：必填空与越界（修订序号为 0）各一条。
    #[test]
    fn new_rejects_empty_description_and_zero_revision_no() {
        let empty = VoucherCategoryProfileRevisionData {
            description: "  ".to_string(),
            ..data()
        };
        assert!(
            VoucherCategoryProfileRevision::new(VoucherCategoryProfileRevisionId::new("vcp-1"), empty)
                .is_err()
        );

        let zero_revision = VoucherCategoryProfileRevisionData {
            revision_no: 0,
            ..data()
        };
        assert!(VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new("vcp-1"),
            zero_revision
        )
        .is_err());
    }

    /// 后继修订只替换描述并保留 SKU 身份与状态。
    #[test]
    fn content_successor_preserves_identity_and_status() {
        let current =
            VoucherCategoryProfileRevision::new(VoucherCategoryProfileRevisionId::new("vcp-1"), data())
                .unwrap();
        let successor = current
            .content_successor(
                VoucherCategoryProfileRevisionId::new("vcp-2"),
                2,
                "新描述".to_string(),
            )
            .unwrap();

        assert_eq!(successor.revision.revision_no, 2);
        assert_eq!(successor.description, "新描述");
        assert_eq!(successor.sku_id, current.sku_id);
        assert_eq!(successor.status, current.status);
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
