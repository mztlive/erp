//! `party_revision`：企业主体不可变修订（数据模型 §6.2，P1 §2.2 快照字段）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

pub use crate::ids::{PartyId, PartyRevisionId};

/// 法定名称最大长度。
const LEGAL_NAME_MAX_LEN: usize = 200;
/// 简称最大长度。
const SHORT_NAME_MAX_LEN: usize = 100;
/// 变更原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 500;

/// 主体修订创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyRevisionData {
    /// 稳定主体 ID。
    pub party_id: PartyId,
    /// 同一稳定对象内从 1 递增的修订序号。
    pub revision_no: u32,
    /// 法定名称（结构化快照，§2.2）。
    pub legal_name: String,
    /// 简称（结构化快照）。
    pub short_name: Option<String>,
    /// 生效开始日期（自然日）。
    pub effective_from: BusinessDate,
    /// 生效结束日期（自然日）；`None` 表示长期有效。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

/// 主体修订实体（不可变修订，§4.4）。
///
/// 修订一经形成不得修改内容，只允许追加更高序号的新修订；生效区间
/// 不得重叠（跨行约束由 P3 事务校验，§6.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PartyRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 稳定主体 ID。
    pub party_id: PartyId,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 生效开始日期。
    pub effective_from: BusinessDate,
    /// 生效结束日期。
    pub effective_to: Option<BusinessDate>,
    /// 变更原因。
    pub change_reason: String,
}

impl PartyRevision {
    /// 创建主体修订。
    ///
    /// 完成 legal_name / change_reason 的必填校验与规范化（去首尾空白、
    /// 长度上限），short_name 可选；强制 `effective_to` 晚于 `effective_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的修订实体。
    ///
    /// # 错误
    /// 当必填字段为空/超长，或生效结束日期不晚于生效开始日期时返回错误。
    pub fn new(id: PartyRevisionId, data: PartyRevisionData) -> Result<Self> {
        let legal_name = normalize_required_text(
            data.legal_name,
            "法定名称不能为空",
            LEGAL_NAME_MAX_LEN,
            "法定名称过长",
        )?;
        let short_name = normalize_optional_text(data.short_name, "简称", SHORT_NAME_MAX_LEN)?;
        let change_reason = normalize_required_text(
            data.change_reason,
            "变更原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "变更原因过长",
        )?;
        ensure_window_valid(data.effective_from, data.effective_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            party_id: data.party_id,
            legal_name,
            short_name,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
            change_reason,
        })
    }
}

/// 校验生效区间：`effective_to` 必须晚于 `effective_from`。
///
/// # 参数
/// * `effective_from` - 生效开始日期
/// * `effective_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(effective_from: BusinessDate, effective_to: Option<BusinessDate>) -> Result<()> {
    if let Some(effective_to) = effective_to {
        if effective_to <= effective_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PartyRevision, PartyRevisionData};
    use crate::common::time::BusinessDate;
    use crate::ids::{PartyId, PartyRevisionId};

    fn revision_data() -> PartyRevisionData {
        PartyRevisionData {
            party_id: PartyId::new("party-1"),
            revision_no: 1,
            legal_name: " 上海示例科技有限公司 ".to_string(),
            short_name: Some(" 示例科技 ".to_string()),
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            change_reason: " 首次建档 ".to_string(),
        }
    }

    /// happy path：名称去空白，字段落库正确。
    #[test]
    fn new_trims_and_normalizes() {
        let revision = PartyRevision::new(PartyRevisionId::new("rev-1"), revision_data()).unwrap();
        assert_eq!(revision.legal_name, "上海示例科技有限公司");
        assert_eq!(revision.short_name.as_deref(), Some("示例科技"));
        assert_eq!(revision.revision.revision_no, 1);
        assert_eq!(revision.change_reason, "首次建档");
    }

    /// 失败路径：必填为空、超长、生效区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_name = PartyRevisionData {
            legal_name: "   ".to_string(),
            ..revision_data()
        };
        assert!(PartyRevision::new(PartyRevisionId::new("r"), blank_name).is_err());

        let overlong_reason = PartyRevisionData {
            change_reason: "x".repeat(501),
            ..revision_data()
        };
        assert!(PartyRevision::new(PartyRevisionId::new("r"), overlong_reason).is_err());

        let reversed = PartyRevisionData {
            effective_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..revision_data()
        };
        assert!(PartyRevision::new(PartyRevisionId::new("r"), reversed).is_err());

        let overlong_short = PartyRevisionData {
            short_name: Some("s".repeat(101)),
            ..revision_data()
        };
        assert!(PartyRevision::new(PartyRevisionId::new("r"), overlong_short).is_err());
    }

    /// 失效区间允许长期（`None`）与边界合法值。
    #[test]
    fn open_ended_window_is_allowed() {
        let open = PartyRevisionData {
            effective_to: None,
            ..revision_data()
        };
        assert!(PartyRevision::new(PartyRevisionId::new("r"), open).is_ok());

        let same_day = PartyRevisionData {
            effective_to: Some(BusinessDate::from_ymd(2026, 1, 1).unwrap()),
            ..revision_data()
        };
        assert!(
            PartyRevision::new(PartyRevisionId::new("r"), same_day).is_err(),
            "同日视为区间倒挂"
        );
    }

    /// 修订不可变：BSON 往返保持全字段一致。
    #[test]
    fn bson_roundtrip() {
        let revision = PartyRevision::new(PartyRevisionId::new("rev-2"), revision_data()).unwrap();
        let roundtrip: PartyRevision = bson::from_document(bson::to_document(&revision).unwrap()).unwrap();
        assert_eq!(roundtrip, revision);
    }
}
