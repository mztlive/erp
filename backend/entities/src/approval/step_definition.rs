//! `approval_step_definition`：审批定义版本内的严格串行步骤。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{ApprovalDefinitionId, ApprovalStepDefinitionId};
use crate::validation::normalize_required_text;
use crate::work_item::WorkItemType;

use super::{ApprovalAssignmentMode, ApprovalDecision};

const STEP_KEY_MAX_LEN: usize = 128;
const HANDLER_KEY_MAX_LEN: usize = 128;
const RESOLVER_KEY_MAX_LEN: usize = 128;

/// 审批步骤定义创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalStepDefinitionData {
    /// 所属审批定义版本记录。
    pub approval_definition_id: ApprovalDefinitionId,
    /// 定义版本内稳定且唯一的步骤编码。
    pub step_key: String,
    /// 从 1 开始的严格串行顺序号。
    pub sequence_no: u32,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 已注册的页面处理器编码。
    pub handler_key: String,
    /// 人工责任分派模式。
    pub assignment_mode: ApprovalAssignmentMode,
    /// 已注册的服务端处理人解析器编码。
    pub assignee_resolver_key: String,
    /// 本步骤允许形成的决定集合。
    pub allowed_decisions: Vec<ApprovalDecision>,
}

/// 审批步骤定义实体。
///
/// 步骤内容由发布 Service 在父定义为 `DRAFT` 时写入；父定义发布后必须永久冻结。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalStepDefinition {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属审批定义版本记录。
    pub approval_definition_id: ApprovalDefinitionId,
    /// 定义版本内稳定且唯一的步骤编码。
    pub step_key: String,
    /// 从 1 开始的严格串行顺序号。
    pub sequence_no: u32,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 已注册的页面处理器编码。
    pub handler_key: String,
    /// 人工责任分派模式。
    pub assignment_mode: ApprovalAssignmentMode,
    /// 已注册的服务端处理人解析器编码。
    pub assignee_resolver_key: String,
    /// 本步骤允许形成的决定集合。
    pub allowed_decisions: Vec<ApprovalDecision>,
}

impl ApprovalStepDefinition {
    /// 创建审批步骤定义。
    ///
    /// 本构造器校验单行字段；父定义仍为草稿、序号整体连续以及全部注册表引用
    /// 有效，由发布 Service 在同一受控操作中校验。
    ///
    /// # 参数
    /// * `id` - 步骤定义主键
    /// * `data` - 步骤定义创建数据
    ///
    /// # 返回
    /// 返回规范化后的步骤定义。
    ///
    /// # 错误
    /// 顺序号为零、文本字段非法、决定集合为空或包含重复决定时返回错误。
    pub fn new(id: ApprovalStepDefinitionId, data: ApprovalStepDefinitionData) -> Result<Self> {
        if data.sequence_no == 0 {
            return Err(Error::from("审批步骤顺序号必须从 1 开始"));
        }
        if data.allowed_decisions.is_empty() {
            return Err(Error::from("审批步骤至少允许一个固定决定"));
        }
        let unique_decisions: HashSet<_> = data.allowed_decisions.iter().copied().collect();
        if unique_decisions.len() != data.allowed_decisions.len() {
            return Err(Error::from("审批步骤允许决定不得重复"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            approval_definition_id: data.approval_definition_id,
            step_key: normalize_required_text(
                data.step_key,
                "审批步骤编码不能为空",
                STEP_KEY_MAX_LEN,
                "审批步骤编码过长",
            )?,
            sequence_no: data.sequence_no,
            work_item_type: data.work_item_type,
            handler_key: normalize_required_text(
                data.handler_key,
                "审批步骤处理器不能为空",
                HANDLER_KEY_MAX_LEN,
                "审批步骤处理器过长",
            )?,
            assignment_mode: data.assignment_mode,
            assignee_resolver_key: normalize_required_text(
                data.assignee_resolver_key,
                "审批步骤处理人解析器不能为空",
                RESOLVER_KEY_MAX_LEN,
                "审批步骤处理人解析器过长",
            )?,
            allowed_decisions: data.allowed_decisions,
        })
    }

    /// 判断本步骤是否允许指定审批决定。
    ///
    /// # 参数
    /// * `decision` - 待校验的固定决定
    ///
    /// # 返回
    /// 决定存在于冻结允许集合中时返回 `true`。
    pub fn allows(&self, decision: ApprovalDecision) -> bool {
        self.allowed_decisions.contains(&decision)
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalStepDefinition, ApprovalStepDefinitionData};
    use crate::approval::{ApprovalAssignmentMode, ApprovalDecision};
    use crate::ids::{ApprovalDefinitionId, ApprovalStepDefinitionId};
    use crate::work_item::WorkItemType;

    fn data() -> ApprovalStepDefinitionData {
        ApprovalStepDefinitionData {
            approval_definition_id: ApprovalDefinitionId::new("definition-1"),
            step_key: " SALES_MANAGER ".to_string(),
            sequence_no: 1,
            work_item_type: WorkItemType::CardSalesManagerApproval,
            handler_key: " card_sales_approval ".to_string(),
            assignment_mode: ApprovalAssignmentMode::Direct,
            assignee_resolver_key: " sales_manager_of_owner ".to_string(),
            allowed_decisions: vec![
                ApprovalDecision::Approve,
                ApprovalDecision::RejectToApplicant,
                ApprovalDecision::TerminateApproval,
            ],
        }
    }

    #[test]
    fn new_step_definition_normalizes_and_keeps_allowed_decisions() {
        let step =
            ApprovalStepDefinition::new(ApprovalStepDefinitionId::new("step-definition-1"), data()).unwrap();
        assert_eq!(step.step_key, "SALES_MANAGER");
        assert_eq!(step.handler_key, "card_sales_approval");
        assert_eq!(step.assignee_resolver_key, "sales_manager_of_owner");
        assert!(step.allows(ApprovalDecision::Approve));
        assert!(step.allows(ApprovalDecision::TerminateApproval));
    }

    #[test]
    fn step_sequence_must_start_at_one() {
        let payload = ApprovalStepDefinitionData {
            sequence_no: 0,
            ..data()
        };
        assert!(
            ApprovalStepDefinition::new(ApprovalStepDefinitionId::new("step-definition-1"), payload,)
                .is_err()
        );
    }

    #[test]
    fn allowed_decisions_must_be_non_empty_and_unique() {
        let empty = ApprovalStepDefinitionData {
            allowed_decisions: vec![],
            ..data()
        };
        assert!(
            ApprovalStepDefinition::new(ApprovalStepDefinitionId::new("step-definition-1"), empty,).is_err()
        );

        let duplicate = ApprovalStepDefinitionData {
            allowed_decisions: vec![ApprovalDecision::Approve, ApprovalDecision::Approve],
            ..data()
        };
        assert!(
            ApprovalStepDefinition::new(ApprovalStepDefinitionId::new("step-definition-1"), duplicate,)
                .is_err()
        );
    }

    #[test]
    fn entity_roundtrips_through_bson() {
        let step =
            ApprovalStepDefinition::new(ApprovalStepDefinitionId::new("step-definition-1"), data()).unwrap();
        let roundtrip: ApprovalStepDefinition =
            bson::from_document(bson::to_document(&step).unwrap()).unwrap();
        assert_eq!(roundtrip, step);
    }
}
