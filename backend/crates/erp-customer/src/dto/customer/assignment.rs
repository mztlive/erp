//! 客户归属命令请求、列表参数与响应视图。

use application_core::non_blank;
use erp_core::common::time::BusinessDate;
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::entity::customer::{
    AssignCustomerAssignment, AssignmentRole, CustomerAssignment, CustomerAssignmentCommand,
    EndCustomerAssignment,
};
use crate::error::{Error, Result};

/// 客户归属列表允许的排序字段白名单。
pub(crate) const CUSTOMER_ASSIGNMENT_SORT_FIELDS: &[&str] = &["created_at", "valid_from", "valid_to"];

/// 归属变更动作（W03：结束旧归属并建立新归属，不原地修改）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentAction {
    /// 建立新归属（同时结束同一客户同一角色的重叠旧归属；OWNER 恰好一人）。
    Assign,
    /// 提前结束既有归属（只允许结束有效期）。
    End,
}

/// 归属写入请求。
///
/// - `Assign`：`user_id`/`assignment_role`/`valid_from`/`valid_to` 必填；
///   同一客户同一角色、同一客户的 OWNER 均不允许重叠区间（§6.2）。
/// - `End`：`assignment_id` 必填，`valid_to` 必填（晚于该归属 `valid_from`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAssignmentRequest {
    /// 归属变更动作。
    pub action: AssignmentAction,
    /// 销售人员（`Assign` 必填）。
    pub user_id: Option<String>,
    /// 归属角色（`Assign` 必填）。
    pub assignment_role: Option<AssignmentRole>,
    /// 归属生效开始日期（`Assign` 必填）。
    pub valid_from: Option<BusinessDate>,
    /// 归属生效结束日期（`Assign`/`End` 均可选；`End` 必填）。
    pub valid_to: Option<BusinessDate>,
    /// 目标归属 ID（`End` 必填）。
    pub assignment_id: Option<String>,
    /// 调整原因。
    #[validate(custom(function = "non_blank", message = "调整原因不能为空"))]
    pub change_reason: String,
    /// 期望的乐观锁版本（`End` 必填）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: Option<u64>,
}

impl CustomerAssignmentRequest {
    /// 将外部请求一次转换为强类型归属命令。
    ///
    /// 完成动作字段组合校验与输入规范化；客户/账号存在性、重叠查询、
    /// 事务和审计仍由 Service 负责。
    ///
    /// # 返回
    /// 返回不含非法状态的 `Assign` 或 `End` 命令。
    ///
    /// # 错误
    /// 必填字段缺失、错误字段组合、空白文本、倒序窗口或版本小于 1 时返回
    /// `ValidationError` 或领域逻辑错误。
    ///
    /// # 关键业务约束
    /// 外部 JSON 形状保持兼容；内部命令按动作拆开，不得同时持有另一动作字段。
    pub fn into_command(self) -> Result<CustomerAssignmentCommand> {
        match self.action {
            AssignmentAction::Assign => self.into_assign_command(),
            AssignmentAction::End => self.into_end_command(),
        }
    }

    /// 把 `assign` 请求转换为建立归属命令。
    ///
    /// # 返回
    /// 返回已规范化的建立命令。
    ///
    /// # 错误
    /// 缺少销售人员/角色/开始日期，或携带结束动作字段时返回 `ValidationError`。
    ///
    /// # 关键业务约束
    /// 不得把目标归属 ID 或乐观锁版本带入建立命令。
    fn into_assign_command(self) -> Result<CustomerAssignmentCommand> {
        reject_present(self.assignment_id, "Assign 不得携带目标归属 ID")?;
        reject_present(self.version, "Assign 不得携带乐观锁版本")?;
        let user_id = require_field(self.user_id, "Assign 必须携带销售人员")?;
        let assignment_role = require_field(self.assignment_role, "Assign 必须携带归属角色")?;
        let valid_from = require_field(self.valid_from, "Assign 必须携带生效开始日期")?;
        Ok(CustomerAssignmentCommand::Assign(AssignCustomerAssignment::new(
            user_id,
            assignment_role,
            valid_from,
            self.valid_to,
            self.change_reason,
        )?))
    }

    /// 把 `end` 请求转换为结束归属命令。
    ///
    /// # 返回
    /// 返回已规范化的结束命令。
    ///
    /// # 错误
    /// 缺少归属 ID/结束日期/版本，或携带建立动作字段时返回 `ValidationError`。
    ///
    /// # 关键业务约束
    /// 不得把销售人员、归属角色或开始日期带入结束命令；版本必须大于 0。
    fn into_end_command(self) -> Result<CustomerAssignmentCommand> {
        reject_present(self.user_id, "End 不得携带销售人员")?;
        reject_present(self.assignment_role, "End 不得携带归属角色")?;
        reject_present(self.valid_from, "End 不得携带生效开始日期")?;
        let assignment_id = require_field(self.assignment_id, "End 必须携带目标归属 ID")?;
        let valid_to = require_field(self.valid_to, "End 必须携带生效结束日期")?;
        let version = require_field(self.version, "End 必须携带乐观锁版本")?;
        Ok(CustomerAssignmentCommand::End(EndCustomerAssignment::new(
            assignment_id,
            valid_to,
            version,
            self.change_reason,
        )?))
    }
}

/// 提取动作必填字段。
///
/// # 参数
/// * `value` - 请求可选值
/// * `message` - 缺失时的稳定校验文案
///
/// # 返回
/// 字段存在时返回内部值。
///
/// # 错误
/// 字段缺失时返回 `ValidationError`。
fn require_field<T>(value: Option<T>, message: &str) -> Result<T> {
    value.ok_or_else(|| Error::ValidationError(message.to_string()))
}

/// 拒绝当前动作不应出现的字段。
///
/// # 参数
/// * `value` - 请求可选值
/// * `message` - 错误字段组合文案
///
/// # 返回
/// 字段未携带时返回 `Ok(())`。
///
/// # 错误
/// 字段存在时返回 `ValidationError`。
fn reject_present<T>(value: Option<T>, message: &str) -> Result<()> {
    if value.is_some() {
        return Err(Error::ValidationError(message.to_string()));
    }
    Ok(())
}

/// 归属响应视图（契约形状对齐 `customer_assignment` 投影行）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CustomerAssignmentView {
    /// 实体主键。
    pub id: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 销售人员。
    pub user_id: String,
    /// 销售人员展示名。
    pub user_name: String,
    /// 归属角色。
    pub assignment_role: AssignmentRole,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 调整原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<CustomerAssignment> for CustomerAssignmentView {
    /// 从实体构造响应视图。
    fn from(assignment: CustomerAssignment) -> Self {
        let user_name = assignment.user_id.clone();
        Self {
            id: assignment.base.id,
            customer_id: assignment.customer_id.to_string(),
            user_id: assignment.user_id,
            user_name,
            assignment_role: assignment.assignment_role,
            valid_from: assignment.valid_from.to_string(),
            valid_to: assignment.valid_to.map(|date| date.to_string()),
            change_reason: assignment.change_reason,
            version: assignment.base.version,
            created_at: assignment.base.created_at,
        }
    }
}

/// 客户归属列表查询参数（`customer_id` 走路径参数）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CustomerAssignmentListParams {
    /// 销售人员筛选。
    pub user_id: Option<String>,
    /// 归属角色筛选。
    pub assignment_role: Option<AssignmentRole>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`valid_from`/`valid_to`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{AssignmentAction, CustomerAssignmentRequest};
    use crate::entity::customer::{AssignmentRole, CustomerAssignmentCommand};

    #[test]
    fn assign_and_end_actions_accept_stable_wire_codes() {
        let request: CustomerAssignmentRequest = serde_json::from_value(json!({
            "action": "assign",
            "user_id": "admin-2",
            "assignment_role": "COLLABORATOR",
            "valid_from": "2026-08-08",
            "change_reason": "联合跟进"
        }))
        .unwrap();
        assert_eq!(request.action, AssignmentAction::Assign);
        assert!(request.version.is_none());
        match request.into_command().unwrap() {
            CustomerAssignmentCommand::Assign(command) => {
                assert_eq!(command.user_id(), "admin-2");
                assert_eq!(command.assignment_role(), AssignmentRole::Collaborator);
            },
            CustomerAssignmentCommand::End(_) => panic!("assign 不得变成 End"),
        }

        let end: CustomerAssignmentRequest = serde_json::from_value(json!({
            "action": "end",
            "assignment_id": "asg-1",
            "valid_to": "2026-09-01",
            "version": 3,
            "change_reason": "结束协作"
        }))
        .unwrap();
        assert_eq!(end.action, AssignmentAction::End);
        match end.into_command().unwrap() {
            CustomerAssignmentCommand::End(command) => {
                assert_eq!(command.assignment_id(), "asg-1");
                assert_eq!(command.version(), 3);
            },
            CustomerAssignmentCommand::Assign(_) => panic!("end 不得变成 Assign"),
        }
    }

    #[test]
    fn assignment_assign_rejects_missing_fields_and_end_only_combo() {
        let complete = json!({
            "action": "assign",
            "user_id": " admin-2 ",
            "assignment_role": "OWNER",
            "valid_from": "2026-08-08",
            "valid_to": "2026-12-31",
            "change_reason": " 换任 "
        });
        match parse_assignment(&complete).into_command().unwrap() {
            CustomerAssignmentCommand::Assign(command) => {
                assert_eq!(command.user_id(), "admin-2");
                assert_eq!(command.assignment_role().as_str(), "OWNER");
                assert_eq!(command.change_reason(), "换任");
            },
            CustomerAssignmentCommand::End(_) => panic!("complete assign 必须成功"),
        }

        for field in ["user_id", "assignment_role", "valid_from"] {
            let mut missing = complete.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse_assignment(&missing).into_command().is_err(), "{field} 缺失必须失败");
        }

        let mut with_id = complete.clone();
        with_id.as_object_mut().unwrap().insert("assignment_id".to_string(), json!("asg-1"));
        assert!(parse_assignment(&with_id).into_command().is_err());

        let mut with_version = complete;
        with_version.as_object_mut().unwrap().insert("version".to_string(), json!(1));
        assert!(parse_assignment(&with_version).into_command().is_err());
    }

    #[test]
    fn assignment_end_rejects_missing_fields_wrong_combo_and_lock_version() {
        let complete = json!({
            "action": "end",
            "assignment_id": " asg-9 ",
            "valid_to": "2026-09-01",
            "version": 2,
            "change_reason": " 结束协作 "
        });
        match parse_assignment(&complete).into_command().unwrap() {
            CustomerAssignmentCommand::End(command) => {
                assert_eq!(command.assignment_id(), "asg-9");
                assert_eq!(command.version(), 2);
            },
            CustomerAssignmentCommand::Assign(_) => panic!("complete end 必须成功"),
        }

        for field in ["assignment_id", "valid_to", "version"] {
            let mut missing = complete.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(parse_assignment(&missing).into_command().is_err(), "{field} 缺失必须失败");
        }

        let mut with_user = complete.clone();
        with_user.as_object_mut().unwrap().insert("user_id".to_string(), json!("admin-2"));
        assert!(parse_assignment(&with_user).into_command().is_err());

        let mut with_role = complete.clone();
        with_role.as_object_mut().unwrap().insert("assignment_role".to_string(), json!("COLLABORATOR"));
        assert!(parse_assignment(&with_role).into_command().is_err());

        let mut with_from = complete.clone();
        with_from.as_object_mut().unwrap().insert("valid_from".to_string(), json!("2026-08-08"));
        assert!(parse_assignment(&with_from).into_command().is_err());

        let mut zero_version = complete;
        zero_version.as_object_mut().unwrap().insert("version".to_string(), json!(0));
        assert!(parse_assignment(&zero_version).into_command().is_err());
    }

    fn parse_assignment(value: &serde_json::Value) -> CustomerAssignmentRequest {
        serde_json::from_value(value.clone()).unwrap()
    }
}
