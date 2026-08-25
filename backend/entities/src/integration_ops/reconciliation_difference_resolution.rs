//! `reconciliation_difference_resolution`：W29 对账差异的追加式决定记录。
//!
//! 本实体只接受 W29 强类型决定：查询、重放、重新归集、关联补偿、补证、
//! 两种正式对账结论，以及不形成业务解决结论的重复/误派关闭证据。责任开始、
//! 退回与转交只进入 `work_item`。记录不可更新或删除，当前状态完全由最后一条决定固定派生。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{ReconciliationDifferenceId, ReconciliationDifferenceResolutionId};

const EVIDENCE_REFERENCE_MAX_LEN: usize = 512;
const HANDLED_BY_MAX_LEN: usize = 128;

/// 对账差异的固定决定类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionAction {
    /// 查询原动作结果。
    QueryOriginalResult,
    /// 按服务端锁定的原请求身份重放。
    ReplayOriginal,
    /// 重新归集既有业务事实。
    Reattribute,
    /// 关联已形成的正式补偿结果。
    LinkCompensation,
    /// 追加受控证据。
    AddEvidence,
    /// 确认不存在业务错误。
    ConfirmNoError,
    /// 确认差异有效。
    ConfirmValidDifference,
    /// 关闭重复任务；必须引用有效替代正式任务。
    CloseDuplicate,
    /// 关闭误派任务；必须引用误派原因证据。
    CloseMisrouted,
}

impl ResolutionAction {
    /// 返回面向用户的稳定标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::QueryOriginalResult => "查询原结果",
            Self::ReplayOriginal => "重放原动作",
            Self::Reattribute => "重新归集",
            Self::LinkCompensation => "关联正式补偿",
            Self::AddEvidence => "追加证据",
            Self::ConfirmNoError => "确认无误",
            Self::ConfirmValidDifference => "确认有效差异",
            Self::CloseDuplicate => "关闭重复任务",
            Self::CloseMisrouted => "关闭误派任务",
        }
    }

    /// 返回持久化稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryOriginalResult => "query_original_result",
            Self::ReplayOriginal => "replay_original",
            Self::Reattribute => "reattribute",
            Self::LinkCompensation => "link_compensation",
            Self::AddEvidence => "add_evidence",
            Self::ConfirmNoError => "confirm_no_error",
            Self::ConfirmValidDifference => "confirm_valid_difference",
            Self::CloseDuplicate => "close_duplicate",
            Self::CloseMisrouted => "close_misrouted",
        }
    }

    /// 返回本决定唯一允许的派生状态。
    pub fn derived_status(self) -> ResultingStatus {
        match self {
            Self::QueryOriginalResult | Self::ReplayOriginal => ResultingStatus::Open,
            Self::Reattribute | Self::LinkCompensation | Self::AddEvidence => {
                ResultingStatus::EvidencePending
            }
            Self::ConfirmNoError => ResultingStatus::ConfirmedNoError,
            Self::ConfirmValidDifference => ResultingStatus::ConfirmedValidDifference,
            Self::CloseDuplicate | Self::CloseMisrouted => ResultingStatus::Closed,
        }
    }

    /// 判断决定是否必须携带服务端可追溯的证据引用。
    fn requires_evidence(self) -> bool {
        !matches!(self, Self::QueryOriginalResult | Self::ReplayOriginal)
    }
}

/// 对账差异由最后一条决定派生的固定状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResultingStatus {
    /// 尚无可验证终态，继续处理。
    Open,
    /// 已追加证据，但尚未形成正式结论。
    EvidencePending,
    /// 已确认不存在业务错误。
    ConfirmedNoError,
    /// 已确认差异有效。
    ConfirmedValidDifference,
    /// 已按任务治理证据关闭，不代表业务差异已解决。
    Closed,
}

impl ResultingStatus {
    /// 返回面向用户的稳定标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "待处理",
            Self::EvidencePending => "待核验证据",
            Self::ConfirmedNoError => "确认无误",
            Self::ConfirmedValidDifference => "确认有效差异",
            Self::Closed => "已关闭",
        }
    }

    /// 返回持久化稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::EvidencePending => "evidence_pending",
            Self::ConfirmedNoError => "confirmed_no_error",
            Self::ConfirmedValidDifference => "confirmed_valid_difference",
            Self::Closed => "closed",
        }
    }

    /// 判断状态是否已经形成正式业务结论。
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::ConfirmedNoError | Self::ConfirmedValidDifference | Self::Closed
        )
    }
}

/// 差异决定记录的创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReconciliationDifferenceResolutionData {
    /// 所属对账差异 ID。
    pub reconciliation_difference_id: ReconciliationDifferenceId,
    /// 递增决定序号（从 1 开始）。
    pub resolution_no: u32,
    /// 强类型决定。
    pub resolution_action: ResolutionAction,
    /// 动作后的固定派生状态。
    pub resulting_status: ResultingStatus,
    /// 受控证据或正式结果引用。
    pub evidence_reference: Option<String>,
    /// 决定人。
    pub handled_by: String,
    /// 决定时间。
    pub handled_at: Instant,
}

/// 对账差异的不可变追加式决定记录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ReconciliationDifferenceResolution {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属对账差异 ID。
    pub reconciliation_difference_id: ReconciliationDifferenceId,
    /// 递增决定序号。
    pub resolution_no: u32,
    /// 强类型决定。
    pub resolution_action: ResolutionAction,
    /// 决定后的固定派生状态。
    pub resulting_status: ResultingStatus,
    /// 受控证据或正式结果引用。
    pub evidence_reference: Option<String>,
    /// 决定人。
    pub handled_by: String,
    /// 决定时间。
    pub handled_at: Instant,
}

impl ReconciliationDifferenceResolution {
    /// 创建由 W02 受控关闭命令形成的专用领域关闭证据。
    ///
    /// # 错误
    /// 动作不是关闭重复或关闭误派，或证据、序号、操作人不满足实体约束时返回错误。
    pub fn new_close_evidence(
        id: ReconciliationDifferenceResolutionId,
        reconciliation_difference_id: ReconciliationDifferenceId,
        resolution_no: u32,
        action: ResolutionAction,
        evidence_reference: String,
        handled_by: String,
        handled_at: Instant,
    ) -> Result<Self> {
        if !matches!(
            action,
            ResolutionAction::CloseDuplicate | ResolutionAction::CloseMisrouted
        ) {
            return Err(Error::from("领域关闭证据只接受重复或误派关闭动作"));
        }
        let evidence_reference = evidence_reference.trim();
        let fields = evidence_reference.split(';').collect::<Vec<_>>();
        let valid = match action {
            ResolutionAction::CloseDuplicate => {
                fields.len() == 3
                    && reference_field_is_present(fields[0], "work_item:")
                    && reference_field_is_present(fields[1], "replacement_work_item:")
                    && reference_field_is_present(fields[2], "audit_log:")
            }
            ResolutionAction::CloseMisrouted => {
                fields.len() == 2
                    && reference_field_is_present(fields[0], "work_item:")
                    && reference_field_is_present(fields[1], "audit_log:")
            }
            _ => false,
        };
        if !valid {
            return Err(Error::from("领域关闭证据引用格式非法"));
        }
        Self::new(
            id,
            ReconciliationDifferenceResolutionData {
                reconciliation_difference_id,
                resolution_no,
                resolution_action: action,
                resulting_status: ResultingStatus::Closed,
                evidence_reference: Some(evidence_reference.to_string()),
                handled_by,
                handled_at,
            },
        )
    }

    /// 创建不可变决定记录并校验固定派生关系。
    ///
    /// # 错误
    /// 序号、派生状态、决定人或证据约束不成立时返回错误。
    pub fn new(
        id: ReconciliationDifferenceResolutionId,
        data: ReconciliationDifferenceResolutionData,
    ) -> Result<Self> {
        if data.resolution_no == 0 {
            return Err(Error::from("决定序号必须从 1 开始"));
        }
        if data.resulting_status != data.resolution_action.derived_status() {
            return Err(Error::from("决定后的派生状态与决定不一致"));
        }
        let handled_by = normalize_required_text(
            data.handled_by,
            "决定人不能为空",
            HANDLED_BY_MAX_LEN,
            "决定人标识过长",
        )?;
        let evidence_reference =
            normalize_optional_text(data.evidence_reference, "证据引用", EVIDENCE_REFERENCE_MAX_LEN)?;
        if data.resolution_action.requires_evidence() && evidence_reference.is_none() {
            return Err(Error::from("该决定必须引用可追溯证据或正式结果"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            reconciliation_difference_id: data.reconciliation_difference_id,
            resolution_no: data.resolution_no,
            resolution_action: data.resolution_action,
            resulting_status: data.resulting_status,
            evidence_reference,
            handled_by,
            handled_at: data.handled_at,
        })
    }
}

fn reference_field_is_present(field: &str, prefix: &str) -> bool {
    field
        .strip_prefix(prefix)
        .is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{
        ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionData, ResolutionAction,
        ResultingStatus,
    };
    use crate::common::time::Instant;
    use crate::ids::{ReconciliationDifferenceId, ReconciliationDifferenceResolutionId};

    fn data(action: ResolutionAction) -> ReconciliationDifferenceResolutionData {
        ReconciliationDifferenceResolutionData {
            reconciliation_difference_id: ReconciliationDifferenceId::new("diff-1"),
            resolution_no: 1,
            resolution_action: action,
            resulting_status: action.derived_status(),
            evidence_reference: None,
            handled_by: " ops-1 ".to_string(),
            handled_at: Instant::from_unix_secs(1_700_000_000),
        }
    }

    #[test]
    fn non_terminal_query_stays_open() {
        let record = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-1"),
            data(ResolutionAction::QueryOriginalResult),
        )
        .unwrap();

        assert_eq!(record.handled_by, "ops-1");
        assert_eq!(record.resulting_status, ResultingStatus::Open);
        assert!(!record.resulting_status.is_terminal());
    }

    #[test]
    fn evidence_and_terminal_decisions_require_reference() {
        for action in [
            ResolutionAction::AddEvidence,
            ResolutionAction::Reattribute,
            ResolutionAction::LinkCompensation,
            ResolutionAction::ConfirmNoError,
            ResolutionAction::ConfirmValidDifference,
        ] {
            assert!(ReconciliationDifferenceResolution::new(
                ReconciliationDifferenceResolutionId::new(format!("res-{}", action.as_str())),
                data(action),
            )
            .is_err());
        }
    }

    #[test]
    fn terminal_statuses_are_decision_specific() {
        assert_eq!(
            ResolutionAction::ConfirmNoError.derived_status(),
            ResultingStatus::ConfirmedNoError
        );
        assert_eq!(
            ResolutionAction::ConfirmValidDifference.derived_status(),
            ResultingStatus::ConfirmedValidDifference
        );
        assert!(ResultingStatus::ConfirmedNoError.is_terminal());
        assert!(ResultingStatus::ConfirmedValidDifference.is_terminal());
        assert!(ResultingStatus::Closed.is_terminal());
    }

    #[test]
    fn close_actions_form_terminal_non_business_evidence() {
        for action in [ResolutionAction::CloseDuplicate, ResolutionAction::CloseMisrouted] {
            let mut input = data(action);
            input.evidence_reference = Some("work_item://wi-1/replacement/wi-2".to_string());
            let record = ReconciliationDifferenceResolution::new(
                ReconciliationDifferenceResolutionId::new(format!("close-{}", action.as_str())),
                input,
            )
            .unwrap();
            assert_eq!(record.resulting_status, ResultingStatus::Closed);
        }
    }

    #[test]
    fn close_evidence_constructor_rejects_business_decisions() {
        let result = ReconciliationDifferenceResolution::new_close_evidence(
            ReconciliationDifferenceResolutionId::new("close-invalid"),
            ReconciliationDifferenceId::new("diff-1"),
            2,
            ResolutionAction::ConfirmNoError,
            "audit_log:audit-1".to_string(),
            "ops-1".to_string(),
            Instant::from_unix_secs(1_700_000_001),
        );

        assert!(result.is_err());
    }

    #[test]
    fn close_evidence_constructor_requires_fixed_reference_shape() {
        let result = ReconciliationDifferenceResolution::new_close_evidence(
            ReconciliationDifferenceResolutionId::new("close-duplicate"),
            ReconciliationDifferenceId::new("diff-1"),
            2,
            ResolutionAction::CloseDuplicate,
            "work_item:wi-1;audit_log:audit-1".to_string(),
            "ops-1".to_string(),
            Instant::from_unix_secs(1_700_000_001),
        );

        assert!(result.is_err());
    }

    #[test]
    fn entity_roundtrips_through_bson() {
        let mut input = data(ResolutionAction::AddEvidence);
        input.evidence_reference = Some(" audit://evidence-1 ".to_string());
        let record = ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new("res-2"),
            input,
        )
        .unwrap();
        let decoded: ReconciliationDifferenceResolution =
            bson::deserialize_from_document(bson::serialize_to_document(&record).unwrap()).unwrap();

        assert_eq!(decoded, record);
        assert_eq!(record.evidence_reference.as_deref(), Some("audit://evidence-1"));
    }
}
