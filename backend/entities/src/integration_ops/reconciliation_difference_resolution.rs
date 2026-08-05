//! `reconciliation_difference_resolution`：对账差异的解决动作记录（数据模型 §6.21）。
//!
//! 每次领取、处理中、创建纠错、已解决、确认无误或关闭重复动作追加一条记录；
//! 处理记录不可更新或删除（§6.21），当前处理状态由最后一条处理动作派生
//! （`resulting_status` 是动作后的派生状态，由 [`ResolutionAction`] 固定映射，
//! 禁止运行时扩展）。`(reconciliation_difference_id, resolution_no)` 唯一由唯一
//! 索引在仓储层（P2）落实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{IntegrationErrorTaskId, ReconciliationDifferenceId, ReconciliationDifferenceResolutionId};

/// 终态证据引用最大长度。
const EVIDENCE_REFERENCE_MAX_LEN: usize = 512;
/// 处理人标识最大长度。
const HANDLED_BY_MAX_LEN: usize = 128;

/// 解决动作（数据模型 §6.21：领取、处理中、创建纠错、已解决、确认无误、关闭重复）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    /// 领取。
    Claim,
    /// 处理中。
    Processing,
    /// 创建纠错。
    CreateCorrection,
    /// 已解决。
    Resolved,
    /// 确认无误。
    Confirmed,
    /// 关闭重复。
    CloseDuplicate,
}

impl ResolutionAction {
    /// 返回动作的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Claim => "领取",
            Self::Processing => "处理中",
            Self::CreateCorrection => "创建纠错",
            Self::Resolved => "已解决",
            Self::Confirmed => "确认无误",
            Self::CloseDuplicate => "关闭重复",
        }
    }

    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Claim => "claim",
            Self::Processing => "processing",
            Self::CreateCorrection => "create_correction",
            Self::Resolved => "resolved",
            Self::Confirmed => "confirmed",
            Self::CloseDuplicate => "close_duplicate",
        }
    }

    /// 返回动作后的派生状态（固定映射，数据模型 §6.21「动作后的派生状态」）。
    ///
    /// # 返回
    /// 领取/处理中/创建纠错派生为处理中，已解决/确认无误派生为已解决，
    /// 关闭重复派生为已关闭。
    pub fn derived_status(self) -> ResultingStatus {
        match self {
            Self::Claim | Self::Processing | Self::CreateCorrection => ResultingStatus::InProgress,
            Self::Resolved | Self::Confirmed => ResultingStatus::Resolved,
            Self::CloseDuplicate => ResultingStatus::Closed,
        }
    }
}

/// 处理状态（数据模型 §6.21：当前处理状态由最后一条处理动作派生的状态值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultingStatus {
    /// 处理中。
    InProgress,
    /// 已解决。
    Resolved,
    /// 已关闭。
    Closed,
}

impl ResultingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::InProgress => "处理中",
            Self::Resolved => "已解决",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Resolved => "resolved",
            Self::Closed => "closed",
        }
    }
}

/// 差异解决记录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationDifferenceResolutionData {
    /// 所属对账差异 ID。
    pub reconciliation_difference_id: ReconciliationDifferenceId,
    /// 递增处理序号（从 1 开始）。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态（必须与动作的固定映射一致）。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 替代任务 ID（关闭重复时关联）。
    pub replacement_task_id: Option<IntegrationErrorTaskId>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间。
    pub handled_at: Instant,
}

/// 对账差异解决记录实体（数据模型 §6.21，不可变：只追加，不更新、不删除）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReconciliationDifferenceResolution {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属对账差异 ID。
    pub reconciliation_difference_id: ReconciliationDifferenceId,
    /// 递增处理序号。
    pub resolution_no: u32,
    /// 解决动作。
    pub resolution_action: ResolutionAction,
    /// 动作后的派生状态。
    pub resulting_status: ResultingStatus,
    /// 终态证据引用。
    pub evidence_reference: Option<String>,
    /// 替代任务 ID。
    pub replacement_task_id: Option<IntegrationErrorTaskId>,
    /// 处理人。
    pub handled_by: String,
    /// 处理时间。
    pub handled_at: Instant,
}

impl ReconciliationDifferenceResolution {
    /// 创建差异解决记录。
    ///
    /// 完成处理人与证据引用的校验和规范化，并强制三条不变式（数据模型 §6.21）：
    /// - `resolution_no` 从 1 开始递增（关联一致性）；
    /// - `resulting_status` 必须等于动作的固定派生状态；
    /// - 创建纠错必须引用正式结果；关闭重复必须关联替代任务或终态证据；
    ///   处理记录不可更新或删除，本实体只提供 `new` 不提供 `update`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::ReconciliationDifferenceResolutionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的解决记录实体。
    ///
    /// # 错误
    /// 当处理序号为 0、派生状态与动作不一致、处理人为空/超长、证据引用超长、
    /// 创建纠错缺证据引用或关闭重复缺替代任务与证据时返回错误。
    pub fn new(
        id: ReconciliationDifferenceResolutionId,
        data: ReconciliationDifferenceResolutionData,
    ) -> Result<Self> {
        if data.resolution_no == 0 {
            return Err(Error::from("处理序号必须从 1 开始"));
        }
        if data.resulting_status != data.resolution_action.derived_status() {
            return Err(Error::from("动作后的派生状态与动作不一致"));
        }
        let handled_by = normalize_required_text(
            data.handled_by,
            "处理人不能为空",
            HANDLED_BY_MAX_LEN,
            "处理人标识过长",
        )?;
        let evidence_reference = normalize_optional_text(
            data.evidence_reference,
            "终态证据引用",
            EVIDENCE_REFERENCE_MAX_LEN,
        )?;
        if data.resolution_action == ResolutionAction::CreateCorrection && evidence_reference.is_none() {
            return Err(Error::from("创建纠错必须引用正式结果"));
        }
        if data.resolution_action == ResolutionAction::CloseDuplicate
            && data.replacement_task_id.is_none()
            && evidence_reference.is_none()
        {
            return Err(Error::from("关闭重复必须关联替代任务或终态证据"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            reconciliation_difference_id: data.reconciliation_difference_id,
            resolution_no: data.resolution_no,
            resolution_action: data.resolution_action,
            resulting_status: data.resulting_status,
            evidence_reference,
            replacement_task_id: data.replacement_task_id,
            handled_by,
            handled_at: data.handled_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionData, ResolutionAction,
        ResultingStatus,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        IntegrationErrorTaskId, ReconciliationDifferenceId, ReconciliationDifferenceResolutionId,
    };

    const HANDLED_AT: i64 = 1_700_000_000;

    fn resolution_data() -> ReconciliationDifferenceResolutionData {
        ReconciliationDifferenceResolutionData {
            reconciliation_difference_id: ReconciliationDifferenceId::new("diff-1"),
            resolution_no: 1,
            resolution_action: ResolutionAction::Claim,
            resulting_status: ResultingStatus::InProgress,
            evidence_reference: None,
            replacement_task_id: None,
            handled_by: "  ops-1 ".to_string(),
            handled_at: Instant::from_unix_secs(HANDLED_AT),
        }
    }

    #[test]
    fn new_trims_and_normalizes_fields() {
        let resolution = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-1"),
            resolution_data(),
        )
        .unwrap();

        assert_eq!(resolution.handled_by, "ops-1");
        assert_eq!(resolution.resolution_no, 1);
        assert_eq!(resolution.resolution_action, ResolutionAction::Claim);
        assert_eq!(resolution.resulting_status, ResultingStatus::InProgress);
        assert_eq!(
            resolution.reconciliation_difference_id,
            ReconciliationDifferenceId::new("diff-1")
        );
        assert_eq!(resolution.handled_at.unix_secs(), HANDLED_AT);
    }

    #[test]
    fn new_rejects_zero_resolution_no() {
        let zero_no = ReconciliationDifferenceResolutionData {
            resolution_no: 0,
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-2"),
            zero_no,
        )
        .is_err());
    }

    #[test]
    fn new_rejects_status_mismatch_with_action() {
        let mismatch = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::Resolved,
            resulting_status: ResultingStatus::Closed,
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-3"),
            mismatch,
        )
        .is_err());
    }

    #[test]
    fn new_rejects_empty_handled_by() {
        let empty_actor = ReconciliationDifferenceResolutionData {
            handled_by: "  ".to_string(),
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-4"),
            empty_actor,
        )
        .is_err());

        let overlong_actor = ReconciliationDifferenceResolutionData {
            handled_by: "a".repeat(129),
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-5"),
            overlong_actor,
        )
        .is_err());
    }

    #[test]
    fn create_correction_requires_formal_result_reference() {
        let correction_without_evidence = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::CreateCorrection,
            resulting_status: ResultingStatus::InProgress,
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-6"),
            correction_without_evidence,
        )
        .is_err());

        let correction_with_evidence = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::CreateCorrection,
            resulting_status: ResultingStatus::InProgress,
            evidence_reference: Some(" sales_change_order://co-7 ".to_string()),
            ..resolution_data()
        };
        let resolution = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-7"),
            correction_with_evidence,
        )
        .unwrap();
        assert_eq!(
            resolution.evidence_reference.as_deref(),
            Some("sales_change_order://co-7")
        );
    }

    #[test]
    fn close_duplicate_requires_replacement_or_evidence() {
        let close_without_any = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::CloseDuplicate,
            resulting_status: ResultingStatus::Closed,
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-8"),
            close_without_any,
        )
        .is_err());

        let close_with_replacement = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::CloseDuplicate,
            resulting_status: ResultingStatus::Closed,
            replacement_task_id: Some(IntegrationErrorTaskId::new("task-9")),
            ..resolution_data()
        };
        let resolution = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-9"),
            close_with_replacement,
        )
        .unwrap();
        assert_eq!(
            resolution.replacement_task_id,
            Some(IntegrationErrorTaskId::new("task-9"))
        );

        let close_with_evidence = ReconciliationDifferenceResolutionData {
            resolution_action: ResolutionAction::CloseDuplicate,
            resulting_status: ResultingStatus::Closed,
            evidence_reference: Some("mall_order_fact://f-1001".to_string()),
            ..resolution_data()
        };
        assert!(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-10"),
            close_with_evidence,
        )
        .is_ok());
    }

    #[test]
    fn derived_status_mapping_is_fixed() {
        assert_eq!(
            ResolutionAction::Claim.derived_status(),
            ResultingStatus::InProgress
        );
        assert_eq!(
            ResolutionAction::Processing.derived_status(),
            ResultingStatus::InProgress
        );
        assert_eq!(
            ResolutionAction::CreateCorrection.derived_status(),
            ResultingStatus::InProgress
        );
        assert_eq!(
            ResolutionAction::Resolved.derived_status(),
            ResultingStatus::Resolved
        );
        assert_eq!(
            ResolutionAction::Confirmed.derived_status(),
            ResultingStatus::Resolved
        );
        assert_eq!(
            ResolutionAction::CloseDuplicate.derived_status(),
            ResultingStatus::Closed
        );
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&ResolutionAction::CloseDuplicate).unwrap(),
            "\"close_duplicate\""
        );
        assert_eq!(
            serde_json::to_string(&ResultingStatus::InProgress).unwrap(),
            "\"in_progress\""
        );

        assert_eq!(ResolutionAction::CreateCorrection.label(), "创建纠错");
        assert_eq!(ResolutionAction::Confirmed.label(), "确认无误");
        assert_eq!(ResultingStatus::Closed.label(), "已关闭");
        assert_eq!(ResultingStatus::Resolved.as_str(), "resolved");
    }

    #[test]
    fn entity_roundtrip_through_bson() {
        let resolution = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-11"),
            resolution_data(),
        )
        .unwrap();
        let roundtrip: ReconciliationDifferenceResolution =
            bson::from_document(bson::to_document(&resolution).unwrap()).unwrap();
        assert_eq!(roundtrip, resolution);
    }
}
