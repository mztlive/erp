//! `legacy_import_confirmation`：旧数据导入业务确认事实（数据模型 §6.12）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{LegacyImportBatchId, WorkItemId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 确认范围（销售、采购、运营、仓储、财务等）最大长度。
const SCOPE_MAX_LEN: usize = 64;
/// 责任角色最大长度。
const ROLE_MAX_LEN: usize = 128;
/// 导入规则版本最大长度。
const RULE_VERSION_MAX_LEN: usize = 64;
/// 退回原因代码最大长度。
const REASON_CODE_MAX_LEN: usize = 64;
/// 意见或说明最大长度。
const COMMENT_MAX_LEN: usize = 1024;
/// 确认人标识最大长度。
const DECIDER_MAX_LEN: usize = 128;

/// 确认状态（数据模型 §6.12：`PENDING`、`CONFIRMED`、`REJECTED`、`INVALIDATED`）。
///
/// 固定状态机：待确认单向推进到确认、退回或失效；已完成的
/// `CONFIRMED`/`REJECTED` 与已失效的 `INVALIDATED` 永久保留（§6.12）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationStatus {
    /// 待确认。
    Pending,
    /// 已确认。
    Confirmed,
    /// 已退回。
    Rejected,
    /// 已失效（被新试算、规则或 manifest 取代）。
    Invalidated,
}

impl ConfirmationStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待确认",
            Self::Confirmed => "已确认",
            Self::Rejected => "已退回",
            Self::Invalidated => "已失效",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Confirmed => "CONFIRMED",
            Self::Rejected => "REJECTED",
            Self::Invalidated => "INVALIDATED",
        }
    }
}

impl DocumentState for ConfirmationStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Confirmed, Self::Rejected, Self::Invalidated],
            Self::Confirmed | Self::Rejected | Self::Invalidated => &[],
        }
    }
}

/// 确认决策（数据模型 §6.12：`CONFIRM_SCOPE` 或 `RETURN_FOR_FIX`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmationDecision {
    /// 确认本责任范围。
    ConfirmScope,
    /// 退回修复。
    ReturnForFix,
}

impl ConfirmationDecision {
    /// 返回决策的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ConfirmScope => "确认本范围",
            Self::ReturnForFix => "退回修复",
        }
    }

    /// 返回决策的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ConfirmScope => "CONFIRM_SCOPE",
            Self::ReturnForFix => "RETURN_FOR_FIX",
        }
    }
}

/// 导入确认创建数据（数据模型 §6.12）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportConfirmationData {
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围（销售、采购、运营、仓储、财务等）。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    pub import_rule_version: String,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: WorkItemId,
}

/// 旧数据导入业务确认事实（数据模型 §6.12）。
///
/// 正式确认事实：创建即 `PENDING`，只允许按固定状态机完成确认、退回或失效；
/// 已完成的确认永久保留，不设业务软删除（§4.5）。批次上的确认状态仅是该表
/// 事实的派生摘要，不保存单个确认人作为多范围事实源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct LegacyImportConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 责任范围。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 本次确认针对的导入规则版本。
    pub import_rule_version: String,
    /// 确认状态。
    pub status: ConfirmationStatus,
    /// 确认决策；待确认/失效时为空。
    pub decision: Option<ConfirmationDecision>,
    /// 退回原因代码（退回时必填）。
    pub reason_code: Option<String>,
    /// 意见说明（确认意见可选）。
    pub comment: Option<String>,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务（唯一）。
    pub work_item_id: WorkItemId,
    /// 实际确认或退回人。
    pub decided_by: Option<String>,
    /// 实际确认或退回时间。
    pub decided_at: Option<Instant>,
    /// 失效时间。
    pub invalidated_at: Option<Instant>,
    /// 替代确认事实。
    pub replacement_confirmation_id: Option<crate::ids::LegacyImportConfirmationId>,
}

impl LegacyImportConfirmation {
    /// 创建待确认确认事实。
    ///
    /// 完成确认范围、责任角色、规则版本的校验与规范化（去首尾空白、
    /// 非空、长度上限）；创建即 `PENDING`，决策与审计字段为空。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::LegacyImportConfirmationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的确认事实实体。
    ///
    /// # 错误
    /// 当必填文本为空或超长时返回错误。
    pub fn new(
        id: crate::ids::LegacyImportConfirmationId,
        data: LegacyImportConfirmationData,
    ) -> Result<Self> {
        let confirmation_scope = normalize_required_text(
            data.confirmation_scope,
            "确认范围不能为空",
            SCOPE_MAX_LEN,
            "确认范围过长",
        )?;
        let owner_role =
            normalize_required_text(data.owner_role, "责任角色不能为空", ROLE_MAX_LEN, "责任角色过长")?;
        let import_rule_version = normalize_required_text(
            data.import_rule_version,
            "导入规则版本不能为空",
            RULE_VERSION_MAX_LEN,
            "导入规则版本过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            batch_id: data.batch_id,
            confirmation_scope,
            owner_role,
            batch_version: data.batch_version,
            trial_version: data.trial_version,
            import_rule_version,
            status: ConfirmationStatus::Pending,
            decision: None,
            reason_code: None,
            comment: None,
            work_item_id: data.work_item_id,
            decided_by: None,
            decided_at: None,
            invalidated_at: None,
            replacement_confirmation_id: None,
        })
    }

    /// 完成一次业务确认或退回。
    ///
    /// 仅待确认事实可操作；退回（`RETURN_FOR_FIX`）原因代码必填，
    /// 确认（`CONFIRM_SCOPE`）意见可选（§6.12）。同一事务内由
    /// `COMPLETE_IMPORT_BUSINESS_CONFIRMATION` 动作命令联动写入
    /// `workflow_action` 与任务 `COMPLETED`（P3 职责）。
    ///
    /// # 参数
    /// * `decision` - 确认本范围或退回修复
    /// * `decided_by` - 实际确认或退回人
    /// * `decided_at` - 实际确认或退回时间
    /// * `reason_code` - 退回原因代码（退回时必填）
    /// * `comment` - 意见说明（可为空）
    ///
    /// # 返回
    /// 完成操作返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待确认状态、退回缺少原因代码、确认人或说明文本超长时返回错误。
    pub fn decide(
        &mut self,
        decision: ConfirmationDecision,
        decided_by: impl Into<String>,
        decided_at: Instant,
        reason_code: Option<String>,
        comment: Option<String>,
    ) -> Result<()> {
        if self.status != ConfirmationStatus::Pending {
            return Err(Error::from(format!(
                "仅待确认事实可决策，当前状态：{}",
                self.status.label()
            )));
        }
        let status = match decision {
            ConfirmationDecision::ConfirmScope => ConfirmationStatus::Confirmed,
            ConfirmationDecision::ReturnForFix => {
                let reason_code = normalize_required_text(
                    reason_code.unwrap_or_default(),
                    "退回原因代码不能为空",
                    REASON_CODE_MAX_LEN,
                    "退回原因代码过长",
                )?;
                self.reason_code = Some(reason_code);
                ConfirmationStatus::Rejected
            }
        };
        self.decision = Some(decision);
        self.comment = normalize_optional_text(comment, "意见", COMMENT_MAX_LEN)?;
        self.decided_by = Some(normalize_required_text(
            decided_by.into(),
            "确认人不能为空",
            DECIDER_MAX_LEN,
            "确认人过长",
        )?);
        self.decided_at = Some(decided_at);
        self.status = status;
        Ok(())
    }

    /// 使待确认事实失效并登记替代关系。
    ///
    /// 新试算、规则或 manifest 变化后，尚未完成的待确认任务由系统关闭
    /// （§6.12）：`RETURN_FOR_FIX` 后不转交、不创建后继任务，修复并产生新
    /// `trial_version` 后才创建新的确认事实和任务，本事实随即失效。
    ///
    /// # 参数
    /// * `replacement_confirmation_id` - 替代确认事实
    /// * `invalidated_at` - 失效时间
    ///
    /// # 返回
    /// 失效操作返回 `Ok(())`。
    ///
    /// # 错误
    /// 非待确认状态时返回错误（已完成确认永久保留，不可失效）。
    pub fn invalidate(
        &mut self,
        replacement_confirmation_id: crate::ids::LegacyImportConfirmationId,
        invalidated_at: Instant,
    ) -> Result<()> {
        ensure_transition(self.status, ConfirmationStatus::Invalidated)?;
        self.status = ConfirmationStatus::Invalidated;
        self.invalidated_at = Some(invalidated_at);
        self.replacement_confirmation_id = Some(replacement_confirmation_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::{LegacyImportConfirmationId, WorkItemId};

    fn confirmation_data() -> LegacyImportConfirmationData {
        LegacyImportConfirmationData {
            batch_id: LegacyImportBatchId::new("batch-1"),
            confirmation_scope: " 销售 ".to_string(),
            owner_role: " 销售领导 ".to_string(),
            batch_version: 1,
            trial_version: 2,
            import_rule_version: " v1 ".to_string(),
            work_item_id: WorkItemId::new("wi-1"),
        }
    }

    #[test]
    fn new_trims_and_starts_pending() {
        let confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-1"), confirmation_data())
                .unwrap();

        assert_eq!(confirmation.confirmation_scope, "销售");
        assert_eq!(confirmation.owner_role, "销售领导");
        assert_eq!(confirmation.import_rule_version, "v1");
        assert_eq!(confirmation.status, ConfirmationStatus::Pending);
        assert!(confirmation.decision.is_none());
        assert_eq!(confirmation.work_item_id, WorkItemId::new("wi-1"));
    }

    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_scope = LegacyImportConfirmationData {
            confirmation_scope: "   ".to_string(),
            ..confirmation_data()
        };
        assert!(LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-2"), empty_scope).is_err());

        let overlong_role = LegacyImportConfirmationData {
            owner_role: "r".repeat(ROLE_MAX_LEN + 1),
            ..confirmation_data()
        };
        assert!(
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-3"), overlong_role).is_err()
        );
    }

    #[test]
    fn confirm_scope_records_decision_and_audit() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-4"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        confirmation
            .decide(
                ConfirmationDecision::ConfirmScope,
                " 运营-张三 ".to_string(),
                at,
                None,
                Some(" 范围无误 ".to_string()),
            )
            .unwrap();

        assert_eq!(confirmation.status, ConfirmationStatus::Confirmed);
        assert_eq!(confirmation.decision, Some(ConfirmationDecision::ConfirmScope));
        assert_eq!(confirmation.decided_by.as_deref(), Some("运营-张三"));
        assert_eq!(confirmation.decided_at, Some(at));
        assert_eq!(confirmation.comment.as_deref(), Some("范围无误"));
        assert!(confirmation.reason_code.is_none());
    }

    #[test]
    fn reject_requires_reason_code() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-5"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        assert!(
            confirmation
                .decide(
                    ConfirmationDecision::ReturnForFix,
                    "财务".to_string(),
                    at,
                    None,
                    None
                )
                .is_err(),
            "退回原因必填"
        );

        confirmation
            .decide(
                ConfirmationDecision::ReturnForFix,
                "财务".to_string(),
                at,
                Some(" 客户资料缺失 ".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(confirmation.status, ConfirmationStatus::Rejected);
        assert_eq!(confirmation.decision, Some(ConfirmationDecision::ReturnForFix));
        assert_eq!(confirmation.reason_code.as_deref(), Some("客户资料缺失"));
    }

    #[test]
    fn completed_confirmation_cannot_be_decided_again() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-6"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);
        confirmation
            .decide(
                ConfirmationDecision::ConfirmScope,
                "运营".to_string(),
                at,
                None,
                None,
            )
            .unwrap();

        assert!(
            confirmation
                .decide(
                    ConfirmationDecision::ConfirmScope,
                    "运营".to_string(),
                    at,
                    None,
                    None
                )
                .is_err(),
            "已完成确认不可重复决策"
        );
        assert!(
            confirmation
                .invalidate(LegacyImportConfirmationId::new("c-7"), at)
                .is_err(),
            "已完成确认不可失效"
        );
    }

    #[test]
    fn pending_confirmation_invalidates_with_replacement() {
        let mut confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-8"), confirmation_data())
                .unwrap();
        let at = Instant::from_unix_secs(1_700_000_000);

        confirmation
            .invalidate(LegacyImportConfirmationId::new("c-9"), at)
            .unwrap();
        assert_eq!(confirmation.status, ConfirmationStatus::Invalidated);
        assert_eq!(confirmation.invalidated_at, Some(at));
        assert_eq!(
            confirmation.replacement_confirmation_id,
            Some(LegacyImportConfirmationId::new("c-9"))
        );
    }

    #[test]
    fn status_machine_is_directed() {
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Confirmed).is_ok());
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Rejected).is_ok());
        assert!(ensure_transition(ConfirmationStatus::Pending, ConfirmationStatus::Invalidated).is_ok());
        assert!(
            ensure_transition(ConfirmationStatus::Rejected, ConfirmationStatus::Pending).is_err(),
            "退回为终态，修复后创建新试算的确认事实"
        );
        assert!(ensure_transition(ConfirmationStatus::Invalidated, ConfirmationStatus::Confirmed).is_err());
    }

    #[test]
    fn status_and_decision_serde_use_stable_codes() {
        assert_eq!(
            serde_json::to_string(&ConfirmationStatus::Pending).unwrap(),
            "\"PENDING\""
        );
        assert_eq!(
            serde_json::to_string(&ConfirmationStatus::Invalidated).unwrap(),
            "\"INVALIDATED\""
        );
        assert_eq!(
            serde_json::to_string(&ConfirmationDecision::ReturnForFix).unwrap(),
            "\"RETURN_FOR_FIX\""
        );
        assert_eq!(ConfirmationStatus::Rejected.label(), "已退回");
        assert_eq!(ConfirmationDecision::ConfirmScope.label(), "确认本范围");
    }

    #[test]
    fn bson_roundtrip_preserves_entity() {
        let confirmation =
            LegacyImportConfirmation::new(LegacyImportConfirmationId::new("c-10"), confirmation_data())
                .unwrap();
        let roundtrip: LegacyImportConfirmation =
            bson::from_document(bson::to_document(&confirmation).unwrap()).unwrap();
        assert_eq!(roundtrip, confirmation);
    }
}
