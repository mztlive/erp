//! `supplier_rating_revision`：供应商评估版本（数据模型 §6.2，页面：W14）。
//!
//! 期初评分只在首次合作版本填写；合作中评分与评级按周期追加新版本，
//! 不原位覆盖（§6.2）。同一供应商评估版本有效期不得重叠（跨行约束由
//! P3 事务校验，§6.2）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

pub use crate::ids::{SupplierAccountId, SupplierRatingRevisionId};

/// 变更原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 500;
/// 评分上限（百分制）。
const SCORE_MAX: u8 = 100;

/// 供应商评级（§6.2：A–D 级；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupplierRating {
    /// A 级。
    A,
    /// B 级。
    B,
    /// C 级。
    C,
    /// D 级。
    D,
}

impl SupplierRating {
    /// 返回评级的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::A => "A 级",
            Self::B => "B 级",
            Self::C => "C 级",
            Self::D => "D 级",
        }
    }

    /// 返回评级的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
        }
    }
}

/// 评估版本创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierRatingRevisionData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 同一稳定对象内从 1 递增的修订序号。
    pub revision_no: u32,
    /// 合作期初评分（百分制；只在首次合作版本填写）。
    pub initial_score: Option<u8>,
    /// 供应商评级（A–D 级）。
    pub rating: SupplierRating,
    /// 合作中评分（百分制；随合作过程定期更新）。
    pub current_score: u8,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

/// 评估版本实体（不可变修订，§6.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierRatingRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 合作期初评分。
    pub initial_score: Option<u8>,
    /// 供应商评级。
    pub rating: SupplierRating,
    /// 合作中评分。
    pub current_score: u8,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

impl SupplierRatingRevision {
    /// 创建评估版本。
    ///
    /// 完成变更原因必填校验与规范化；评分必须在 `[0, 100]` 百分制区间；
    /// 期初评分只在首次合作版本（`revision_no == 1`）允许填写；
    /// 强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierRatingRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的评估版本实体。
    ///
    /// # 错误
    /// 当原因为空/超长、评分越界、期初评分出现在非首次版本或生效区间
    /// 倒挂时返回错误。
    pub fn new(id: SupplierRatingRevisionId, data: SupplierRatingRevisionData) -> Result<Self> {
        let change_reason = normalize_required_text(
            data.change_reason,
            "变更原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "变更原因过长",
        )?;
        ensure_scores_valid(data.revision_no, data.initial_score, data.current_score)?;
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_id: data.supplier_id,
            initial_score: data.initial_score,
            rating: data.rating,
            current_score: data.current_score,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            change_reason,
        })
    }

    /// 在追加下一评估版本前结束当前开放区间。
    ///
    /// # Errors
    /// 新版本日期不晚于当前版本开始日时返回错误。
    pub fn close_before(&mut self, next_valid_from: BusinessDate) -> Result<()> {
        if next_valid_from <= self.valid_from {
            return Err(Error::from("新评估版本生效日期必须晚于当前版本生效日期"));
        }
        let previous = next_valid_from
            .as_naive_date()
            .pred_opt()
            .ok_or_else(|| Error::from("评估版本生效日期无法计算前一日"))?;
        self.valid_to = BusinessDate::from_ymd(
            chrono::Datelike::year(&previous),
            chrono::Datelike::month(&previous),
            chrono::Datelike::day(&previous),
        );
        Ok(())
    }
}

/// 校验评分数值与期初评分归属。
///
/// 评分必须在 `[0, 100]` 百分制区间（合作评估口径，W14）；
/// 期初评分只在首次合作版本填写（§6.2）。
///
/// # 参数
/// * `revision_no` - 修订序号
/// * `initial_score` - 期初评分（可空）
/// * `current_score` - 合作中评分
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 评分越界或期初评分出现在非首次版本时返回错误。
fn ensure_scores_valid(revision_no: u32, initial_score: Option<u8>, current_score: u8) -> Result<()> {
    if current_score > SCORE_MAX {
        return Err(Error::from("合作中评分必须在 0–100 百分制区间内"));
    }
    if let Some(initial_score) = initial_score {
        if revision_no != 1 {
            return Err(Error::from("期初评分只在首次合作版本填写"));
        }
        if initial_score > SCORE_MAX {
            return Err(Error::from("期初评分必须在 0–100 百分制区间内"));
        }
    }
    Ok(())
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SupplierRating, SupplierRatingRevision, SupplierRatingRevisionData};
    use crate::common::time::BusinessDate;
    use crate::ids::{SupplierAccountId, SupplierRatingRevisionId};

    fn rating_data() -> SupplierRatingRevisionData {
        SupplierRatingRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_no: 1,
            initial_score: Some(80),
            rating: SupplierRating::B,
            current_score: 85,
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            change_reason: " 首次合作评估 ".to_string(),
        }
    }

    /// happy path：原因去空白，评分与评级落库。
    #[test]
    fn new_trims_and_normalizes() {
        let revision =
            SupplierRatingRevision::new(SupplierRatingRevisionId::new("rating-rev-1"), rating_data())
                .unwrap();
        assert_eq!(revision.change_reason, "首次合作评估");
        assert_eq!(revision.initial_score, Some(80));
        assert_eq!(revision.current_score, 85);
        assert_eq!(revision.rating, SupplierRating::B);
        assert_eq!(revision.revision.revision_no, 1);
    }

    /// 失败路径：原因为空/超长、评分越界、期初评分出现在非首次版本、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_reason = SupplierRatingRevisionData {
            change_reason: "   ".to_string(),
            ..rating_data()
        };
        assert!(SupplierRatingRevision::new(SupplierRatingRevisionId::new("r"), blank_reason).is_err());

        let overlong_reason = SupplierRatingRevisionData {
            change_reason: "x".repeat(501),
            ..rating_data()
        };
        assert!(SupplierRatingRevision::new(SupplierRatingRevisionId::new("r"), overlong_reason).is_err());

        let score_out_of_range = SupplierRatingRevisionData {
            current_score: 101,
            ..rating_data()
        };
        assert!(SupplierRatingRevision::new(SupplierRatingRevisionId::new("r"), score_out_of_range).is_err());

        let initial_on_later_revision = SupplierRatingRevisionData {
            revision_no: 2,
            ..rating_data()
        };
        assert!(
            SupplierRatingRevision::new(SupplierRatingRevisionId::new("r"), initial_on_later_revision)
                .is_err()
        );

        let reversed = SupplierRatingRevisionData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..rating_data()
        };
        assert!(SupplierRatingRevision::new(SupplierRatingRevisionId::new("r"), reversed).is_err());
    }

    /// 后续版本：期初评分省略、合作中评分随周期追加。
    #[test]
    fn later_revision_drops_initial_score() {
        let data = SupplierRatingRevisionData {
            revision_no: 2,
            initial_score: None,
            current_score: 88,
            change_reason: " 周期复评 ".to_string(),
            ..rating_data()
        };
        let revision =
            SupplierRatingRevision::new(SupplierRatingRevisionId::new("rating-rev-2"), data).unwrap();
        assert_eq!(revision.initial_score, None);
        assert_eq!(revision.current_score, 88);
    }

    /// 追加下一版本前，上一开放区间结束到新版本前一日。
    #[test]
    fn close_before_ends_previous_day() {
        let mut revision = SupplierRatingRevision::new(
            SupplierRatingRevisionId::new("rating-rev-close"),
            SupplierRatingRevisionData {
                valid_to: None,
                ..rating_data()
            },
        )
        .unwrap();
        revision
            .close_before(BusinessDate::from_ymd(2026, 2, 1).unwrap())
            .unwrap();
        assert_eq!(revision.valid_to, BusinessDate::from_ymd(2026, 1, 31));
        assert!(revision
            .close_before(BusinessDate::from_ymd(2026, 1, 1).unwrap())
            .is_err());
    }

    /// 评级稳定代码与中文标签。
    #[test]
    fn rating_codes_and_labels() {
        assert_eq!(serde_json::to_string(&SupplierRating::A).unwrap(), "\"A\"");
        assert_eq!(SupplierRating::D.as_str(), "D");
        assert_eq!(SupplierRating::C.label(), "C 级");
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let revision =
            SupplierRatingRevision::new(SupplierRatingRevisionId::new("rating-rev-3"), rating_data())
                .unwrap();
        let roundtrip: SupplierRatingRevision =
            bson::deserialize_from_document(bson::serialize_to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);
    }
}
