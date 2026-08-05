//! `reconciliation_difference`：对账差异（数据模型 §6.21）。
//!
//! 对账不建批次台账，只持久化差异本身；差异发现时间由字典字段 `created_at` 承载
//! （≡ `BaseModel.created_at`）。本实体是正式差异事实（§4.5.1 正式事实不设业务软
//! 删除），创建后不可修改；当前处理状态由最后一条
//! [`ReconciliationDifferenceResolution`](super::ReconciliationDifferenceResolution)
//! 处理动作派生（处理记录不可更新或删除），待处理队列使用处理状态投影（P3）。
//!
//! 差异不直接修改任一系统的正式事实：处理差异需要修改业务时，必须调用相应变更、
//! 纠错或重放入口并在处理记录引用正式结果（P3 服务编排，§6.21）。
//! `(business_object_type, business_object_id, difference_type)` 唯一由唯一索引在
//! 仓储层（P2）落实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::ReconciliationDifferenceId;

/// 差异对象类型最大长度。
const BUSINESS_OBJECT_TYPE_MAX_LEN: usize = 64;
/// 差异对象 ID 最大长度。
const BUSINESS_OBJECT_ID_MAX_LEN: usize = 128;
/// 差异分类最大长度。
const DIFFERENCE_TYPE_MAX_LEN: usize = 64;
/// 不可变证据引用最大长度。
const FACT_REFERENCE_MAX_LEN: usize = 512;

/// 对账差异创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationDifferenceData {
    /// 差异对象类型（如商城订单、供应商订单、销售单等跨域对象目录，P3 固化取值）。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
}

/// 对账差异实体（数据模型 §6.21，正式差异事实，创建后不可修改）。
///
/// 差异金额不落本表：§6.21 字段字典未定义差异金额列，涉钱差异的金额保留在两侧
/// 不可变事实中（`left_fact_reference` / `right_fact_reference` 引用），对账时按
/// 引用取两侧事实核算；新增金额字段违反 §5.1 禁止新增字段规则。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReconciliationDifference {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 差异对象类型。
    pub business_object_type: String,
    /// 差异对象 ID。
    pub business_object_id: String,
    /// 差异分类。
    pub difference_type: String,
    /// 左侧不可变证据引用。
    pub left_fact_reference: Option<String>,
    /// 右侧不可变证据引用。
    pub right_fact_reference: Option<String>,
}

impl ReconciliationDifference {
    /// 创建对账差异。
    ///
    /// 完成对象类型、对象 ID 与差异分类的校验和规范化（去首尾空白、非空、长度上限），
    /// 并强制不变式：差异必须至少引用一侧不可变证据（来源缺失侧允许为空）。
    /// 差异发现时间由 `BaseModel.created_at` 承载，创建后不可修改、不设业务软删除。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReconciliationDifferenceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的对账差异实体。
    ///
    /// # 错误
    /// 当对象类型/对象 ID/差异分类为空或超长、证据引用超长、两侧证据都为空时返回错误。
    pub fn new(id: ReconciliationDifferenceId, data: ReconciliationDifferenceData) -> Result<Self> {
        let business_object_type = normalize_required_text(
            data.business_object_type,
            "差异对象类型不能为空",
            BUSINESS_OBJECT_TYPE_MAX_LEN,
            "差异对象类型过长",
        )?;
        let business_object_id = normalize_required_text(
            data.business_object_id,
            "差异对象ID不能为空",
            BUSINESS_OBJECT_ID_MAX_LEN,
            "差异对象ID过长",
        )?;
        let difference_type = normalize_required_text(
            data.difference_type,
            "差异分类不能为空",
            DIFFERENCE_TYPE_MAX_LEN,
            "差异分类过长",
        )?;
        let left_fact_reference =
            normalize_optional_text(data.left_fact_reference, "左侧证据引用", FACT_REFERENCE_MAX_LEN)?;
        let right_fact_reference =
            normalize_optional_text(data.right_fact_reference, "右侧证据引用", FACT_REFERENCE_MAX_LEN)?;
        if left_fact_reference.is_none() && right_fact_reference.is_none() {
            return Err(crate::errors::Error::from("差异必须至少提供一侧不可变证据引用"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            business_object_type,
            business_object_id,
            difference_type,
            left_fact_reference,
            right_fact_reference,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ReconciliationDifference, ReconciliationDifferenceData};
    use crate::ids::ReconciliationDifferenceId;

    fn difference_data() -> ReconciliationDifferenceData {
        ReconciliationDifferenceData {
            business_object_type: " mall_order ".to_string(),
            business_object_id: " MO-2026-001 ".to_string(),
            difference_type: " amount_mismatch ".to_string(),
            left_fact_reference: Some(" mall_order_fact://f-1001 ".to_string()),
            right_fact_reference: Some(" invoice://inv-88 ".to_string()),
        }
    }

    #[test]
    fn new_trims_and_normalizes_fields() {
        let difference =
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-1"), difference_data())
                .unwrap();

        assert_eq!(difference.business_object_type, "mall_order");
        assert_eq!(difference.business_object_id, "MO-2026-001");
        assert_eq!(difference.difference_type, "amount_mismatch");
        assert_eq!(
            difference.left_fact_reference.as_deref(),
            Some("mall_order_fact://f-1001")
        );
        assert_eq!(
            difference.right_fact_reference.as_deref(),
            Some("invoice://inv-88")
        );
        assert!(
            !difference.base.is_deleted(),
            "正式差异事实不设业务软删除（§4.5.1）"
        );
    }

    #[test]
    fn new_rejects_empty_required_fields() {
        let empty_type = ReconciliationDifferenceData {
            business_object_type: "  ".to_string(),
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-2"), empty_type).is_err()
        );

        let empty_object = ReconciliationDifferenceData {
            business_object_id: "  ".to_string(),
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-3"), empty_object).is_err()
        );

        let empty_difference = ReconciliationDifferenceData {
            difference_type: "  ".to_string(),
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-4"), empty_difference)
                .is_err()
        );
    }

    #[test]
    fn new_rejects_overlong_fields() {
        let overlong_object = ReconciliationDifferenceData {
            business_object_id: "o".repeat(129),
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-5"), overlong_object)
                .is_err()
        );

        let overlong_reference = ReconciliationDifferenceData {
            left_fact_reference: Some("r".repeat(513)),
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-6"), overlong_reference)
                .is_err()
        );
    }

    #[test]
    fn new_rejects_difference_without_any_evidence() {
        let no_evidence = ReconciliationDifferenceData {
            left_fact_reference: None,
            right_fact_reference: None,
            ..difference_data()
        };
        assert!(
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-7"), no_evidence).is_err()
        );
    }

    #[test]
    fn missing_source_side_allows_single_evidence() {
        let single_evidence = ReconciliationDifferenceData {
            left_fact_reference: Some("mall_order_fact://f-1001".to_string()),
            right_fact_reference: None,
            ..difference_data()
        };
        let difference =
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-8"), single_evidence)
                .unwrap();
        assert!(difference.right_fact_reference.is_none());
    }

    #[test]
    fn entity_roundtrip_through_bson() {
        let difference =
            ReconciliationDifference::new(ReconciliationDifferenceId::new("diff-9"), difference_data())
                .unwrap();
        let roundtrip: ReconciliationDifference =
            bson::from_document(bson::to_document(&difference).unwrap()).unwrap();
        assert_eq!(roundtrip, difference);
    }
}
