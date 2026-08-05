//! `workflow_action`：提交、审批、驳回、确认、完成等追加式动作（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{BusinessDocumentId, WorkflowActionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 状态代码最大长度。
const STATUS_CODE_MAX_LEN: usize = 64;
/// 操作者 ID 最大长度。
const ACTOR_ID_MAX_LEN: usize = 128;
/// 责任角色标识最大长度。
const ACTOR_ROLE_MAX_LEN: usize = 128;
/// 意见最大长度。
const COMMENT_MAX_LEN: usize = 512;

/// 动作类型（数据模型 §6.1：提交、通过、驳回、确认、作废、完成等；
/// 固定枚举，其余动作属二期扩展的地基修订候选）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowActionType {
    /// 提交。
    Submit,
    /// 通过。
    Approve,
    /// 驳回。
    Reject,
    /// 确认。
    Confirm,
    /// 作废。
    Void,
    /// 完成。
    Complete,
}

impl WorkflowActionType {
    /// 返回动作类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Submit => "提交",
            Self::Approve => "通过",
            Self::Reject => "驳回",
            Self::Confirm => "确认",
            Self::Void => "作废",
            Self::Complete => "完成",
        }
    }

    /// 返回动作类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Submit => "submit",
            Self::Approve => "approve",
            Self::Reject => "reject",
            Self::Confirm => "confirm",
            Self::Void => "void",
            Self::Complete => "complete",
        }
    }
}

/// 工作流动作创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkflowActionData {
    /// 业务单据（`business_document` 稳定注册）。
    pub document_id: BusinessDocumentId,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态（单据域的状态稳定代码）。
    pub from_status: String,
    /// 迁移后状态（单据域的状态稳定代码）。
    pub to_status: String,
    /// 实际操作者。
    pub actor_id: String,
    /// 动作发生时的责任角色。
    pub actor_role: String,
    /// 意见或驳回原因。
    pub comment: Option<String>,
}

/// 工作流动作实体（数据模型 §6.1）。
///
/// 追加式动作：每次状态迁移都追加一条记录（数据模型第 7 章），只追加不修改；
/// `document_id + created_at` 历史索引与 `actor_id + created_at` 审计索引由 P2
/// 建立。`from_status` / `to_status` 记录其它域单据的状态代码（跨域开放目录，
/// 按固定代码形态校验），与目标单据状态机的逐边一致性核对留给 P5（第 7 章）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WorkflowAction {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 业务单据。
    pub document_id: BusinessDocumentId,
    /// 动作类型。
    pub action_type: WorkflowActionType,
    /// 迁移前状态代码。
    pub from_status: String,
    /// 迁移后状态代码。
    pub to_status: String,
    /// 实际操作者。
    pub actor_id: String,
    /// 动作发生时的责任角色。
    pub actor_role: String,
    /// 意见或驳回原因。
    pub comment: Option<String>,
}

impl WorkflowAction {
    /// 创建工作流动作。
    ///
    /// 完成 from_status/to_status/actor 的校验与规范化：状态代码去首尾空白、
    /// 非空、上限长度、只含大写字母/数字/下划线；动作类字段非空。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::WorkflowActionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的动作记录。
    ///
    /// # 错误
    /// 当状态代码为空/超长/含非法字符，或操作者/角色为空/超长时返回错误。
    pub fn new(id: WorkflowActionId, data: WorkflowActionData) -> Result<Self> {
        let from_status = normalize_status_code(data.from_status, "迁移前状态")?;
        let to_status = normalize_status_code(data.to_status, "迁移后状态")?;
        let actor_id =
            normalize_required_text(data.actor_id, "操作者不能为空", ACTOR_ID_MAX_LEN, "操作者过长")?;
        let actor_role = normalize_required_text(
            data.actor_role,
            "责任角色不能为空",
            ACTOR_ROLE_MAX_LEN,
            "责任角色过长",
        )?;
        let comment = normalize_optional_text(data.comment, "意见", COMMENT_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            document_id: data.document_id,
            action_type: data.action_type,
            from_status,
            to_status,
            actor_id,
            actor_role,
            comment,
        })
    }
}

/// 规范化状态代码：trim + 非空 + 上限 + 大写代码形态校验。
///
/// 状态代码是各单据域 `as_str()` 的稳定输出（如 `EFFECTIVE`、`PENDING_REVIEW`），
/// 这里只校验形态不校验取值，取值与目标单据状态机的核对在 P5。
///
/// # 参数
/// * `value` - 状态代码
/// * `label` - 字段说明（错误信息用）
///
/// # 返回
/// 返回规范化后的状态代码。
///
/// # 错误
/// 当代码为空、超长或含大写字母/数字/下划线以外字符时返回错误。
fn normalize_status_code(value: String, label: &str) -> Result<String> {
    let value = normalize_required_text(
        value,
        &format!("{label}不能为空"),
        STATUS_CODE_MAX_LEN,
        &format!("{label}过长"),
    )?;
    if !value
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
    {
        return Err(Error::from(format!(
            "{label}必须是稳定状态代码（大写字母/数字/下划线）"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{normalize_status_code, WorkflowAction, WorkflowActionData, WorkflowActionType};
    use crate::ids::{BusinessDocumentId, WorkflowActionId};

    fn data() -> WorkflowActionData {
        WorkflowActionData {
            document_id: BusinessDocumentId::new("order-1"),
            action_type: WorkflowActionType::Approve,
            from_status: " PENDING_REVIEW ".to_string(),
            to_status: "EFFECTIVE".to_string(),
            actor_id: " user-1 ".to_string(),
            actor_role: "sales-manager".to_string(),
            comment: Some(" 同意 ".to_string()),
        }
    }

    /// happy path：状态代码转大写形态并去空白，意见 trim。
    #[test]
    fn new_normalizes_status_codes_and_comment() {
        let action = WorkflowAction::new(WorkflowActionId::new("wa-1"), data()).unwrap();
        assert_eq!(action.from_status, "PENDING_REVIEW");
        assert_eq!(action.to_status, "EFFECTIVE");
        assert_eq!(action.actor_id, "user-1");
        assert_eq!(action.actor_role, "sales-manager");
        assert_eq!(action.comment.as_deref(), Some("同意"));
    }

    /// 失败路径：状态代码为空被拒。
    #[test]
    fn new_rejects_empty_status_code() {
        let payload = WorkflowActionData {
            from_status: "  ".to_string(),
            ..data()
        };
        assert!(WorkflowAction::new(WorkflowActionId::new("wa-1"), payload).is_err());
    }

    /// 失败路径：状态代码含非法字符（非大写代码形态）被拒。
    #[test]
    fn new_rejects_invalid_status_code_shape() {
        let payload = WorkflowActionData {
            from_status: "effective".to_string(),
            ..data()
        };
        assert!(WorkflowAction::new(WorkflowActionId::new("wa-1"), payload).is_err());
        assert!(normalize_status_code("PENDING_REVIEW".to_string(), "状态").is_ok());
    }

    /// 失败路径：超长状态代码被拒。
    #[test]
    fn new_rejects_overlong_status_code() {
        let payload = WorkflowActionData {
            to_status: "S".repeat(65),
            ..data()
        };
        assert!(WorkflowAction::new(WorkflowActionId::new("wa-1"), payload).is_err());
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn action_type_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&WorkflowActionType::Void).unwrap(),
            "\"void\""
        );
        assert_eq!(WorkflowActionType::Submit.as_str(), "submit");
        assert_eq!(WorkflowActionType::Reject.label(), "驳回");
        assert_eq!(WorkflowActionType::Confirm.label(), "确认");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let action = WorkflowAction::new(WorkflowActionId::new("wa-1"), data()).unwrap();
        let roundtrip: WorkflowAction = bson::from_document(bson::to_document(&action).unwrap()).unwrap();
        assert_eq!(roundtrip, action);
    }
}
