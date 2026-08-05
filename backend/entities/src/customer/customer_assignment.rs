//! `customer_assignment`：客户归属（数据模型 §6.2，页面：W03）。
//!
//! 同一客户同一时点恰好一个 `OWNER`、同一客户/用户/角色的有效期不得
//! 重叠（跨行约束由 P3 事务校验，§6.2）；负责人变化后只影响新增单据
//! 权限，不删除历史参与权（W03 / §11.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;
use crate::validation::normalize_required_text;

pub use crate::ids::{CustomerAccountId, CustomerAssignmentId};

/// 用户标识最大长度。
const USER_ID_MAX_LEN: usize = 128;
/// 调整原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 500;

/// 归属角色（§6.2：`OWNER` 或 `COLLABORATOR`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentRole {
    /// 负责销售（主负责人）。
    Owner,
    /// 协作销售。
    Collaborator,
}

impl AssignmentRole {
    /// 返回角色的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Owner => "负责销售",
            Self::Collaborator => "协作销售",
        }
    }

    /// 返回角色的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "OWNER",
            Self::Collaborator => "COLLABORATOR",
        }
    }
}

/// 归属创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAssignmentData {
    /// 客户角色 ID。
    pub customer_id: CustomerAccountId,
    /// 销售人员（账号或系统身份；跨域引用，无专用 ID newtype）。
    pub user_id: String,
    /// 归属角色。
    pub assignment_role: AssignmentRole,
    /// 归属生效开始日期。
    pub valid_from: BusinessDate,
    /// 归属生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 调整原因。
    pub change_reason: String,
}

/// 归属更新数据。
///
/// 归属变化按「结束旧归属并建立新归属」维护（W03），原地更新只允许
/// 结束有效期（`Set` 时校验晚于 `valid_from`，用于提前结束）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAssignmentUpdate {
    /// 归属生效结束日期更新意图。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
}

/// 归属实体（§6.2：按有效期保存的主负责人/协作销售归属）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CustomerAssignment {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户角色 ID。
    pub customer_id: CustomerAccountId,
    /// 销售人员。
    pub user_id: String,
    /// 归属角色。
    pub assignment_role: AssignmentRole,
    /// 归属生效开始日期。
    pub valid_from: BusinessDate,
    /// 归属生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 调整原因。
    pub change_reason: String,
}

impl CustomerAssignment {
    /// 创建归属。
    ///
    /// 完成 user_id 与 change_reason 的必填校验与规范化（去首尾空白、
    /// 长度上限）；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerAssignmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的归属实体。
    ///
    /// # 错误
    /// 当 user_id / change_reason 为空或超长，或生效区间倒挂时返回错误。
    pub fn new(id: CustomerAssignmentId, data: CustomerAssignmentData) -> Result<Self> {
        let user_id = normalize_required_text(
            data.user_id,
            "销售人员不能为空",
            USER_ID_MAX_LEN,
            "销售人员标识过长",
        )?;
        let change_reason = normalize_required_text(
            data.change_reason,
            "调整原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "调整原因过长",
        )?;
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            customer_id: data.customer_id,
            user_id,
            assignment_role: data.assignment_role,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            change_reason,
        })
    }

    /// 更新归属（仅允许结束有效期）。
    ///
    /// 归属角色与人员变更必须结束旧归属并建立新归属（W03：结束旧
    /// OWNER 有效期并建立新 OWNER，写变更原因），不原地修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 `valid_to` 不晚于 `valid_from` 时返回错误。
    pub fn update(&mut self, update: CustomerAssignmentUpdate) -> Result<()> {
        if let Some(valid_to) = update.valid_to.into_option() {
            ensure_window_valid(self.valid_from, Some(valid_to))?;
            self.valid_to = Some(valid_to);
        }
        Ok(())
    }

    /// 判断归属当前是否有效（按业务日期判定有效期）。
    ///
    /// # 参数
    /// * `as_of` - 业务日期
    ///
    /// # 返回
    /// 业务日期落在生效区间内时返回 `true`。
    pub fn is_active_on(&self, as_of: BusinessDate) -> bool {
        as_of >= self.valid_from && self.valid_to.is_none_or(|valid_to| as_of < valid_to)
    }
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
    use super::{AssignmentRole, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentUpdate};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{CustomerAccountId, CustomerAssignmentId};

    fn assignment_data() -> CustomerAssignmentData {
        CustomerAssignmentData {
            customer_id: CustomerAccountId::new("customer-1"),
            user_id: " sales-zhangsan ".to_string(),
            assignment_role: AssignmentRole::Owner,
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            change_reason: " 首次指派 ".to_string(),
        }
    }

    /// happy path：用户与原因去空白，角色代码稳定。
    #[test]
    fn new_trims_and_normalizes() {
        let assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-1"), assignment_data()).unwrap();
        assert_eq!(assignment.user_id, "sales-zhangsan");
        assert_eq!(assignment.change_reason, "首次指派");
        assert_eq!(assignment.assignment_role, AssignmentRole::Owner);
        assert_eq!(assignment.assignment_role.as_str(), "OWNER");
        assert_eq!(AssignmentRole::Collaborator.as_str(), "COLLABORATOR");
    }

    /// 失败路径：用户为空/超长、原因为空/超长、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_user = CustomerAssignmentData {
            user_id: "   ".to_string(),
            ..assignment_data()
        };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), blank_user).is_err());

        let blank_reason = CustomerAssignmentData {
            change_reason: "   ".to_string(),
            ..assignment_data()
        };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), blank_reason).is_err());

        let overlong_user = CustomerAssignmentData {
            user_id: "u".repeat(129),
            ..assignment_data()
        };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), overlong_user).is_err());

        let reversed = CustomerAssignmentData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..assignment_data()
        };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), reversed).is_err());
    }

    /// 有效期判定：区间内有效、边界与区间外无效。
    #[test]
    fn validity_window_is_active_checked() {
        let assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-2"), assignment_data()).unwrap();
        assert!(assignment.is_active_on(BusinessDate::from_ymd(2026, 6, 1).unwrap()));
        assert!(assignment.is_active_on(BusinessDate::from_ymd(2026, 1, 1).unwrap()));
        assert!(
            !assignment.is_active_on(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            "结束日不包含"
        );
        assert!(!assignment.is_active_on(BusinessDate::from_ymd(2025, 12, 31).unwrap()));

        let open = CustomerAssignmentData {
            valid_to: None,
            ..assignment_data()
        };
        let open_assignment = CustomerAssignment::new(CustomerAssignmentId::new("assign-3"), open).unwrap();
        assert!(open_assignment.is_active_on(BusinessDate::from_ymd(2030, 1, 1).unwrap()));
    }

    /// 更新：提前结束有效期，倒挂被拒；人员/角色不原地修改。
    #[test]
    fn update_only_ends_validity() {
        let mut assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-4"), assignment_data()).unwrap();
        assignment
            .update(CustomerAssignmentUpdate {
                valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 3, 31).unwrap()),
            })
            .unwrap();
        assert_eq!(
            assignment.valid_to,
            Some(BusinessDate::from_ymd(2026, 3, 31).unwrap())
        );

        let reversed = CustomerAssignmentUpdate {
            valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2025, 1, 1).unwrap()),
        };
        assert!(assignment.update(reversed).is_err());
        assert_eq!(assignment.user_id, "sales-zhangsan");
        assert_eq!(assignment.assignment_role, AssignmentRole::Owner);
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-5"), assignment_data()).unwrap();
        let roundtrip: CustomerAssignment =
            bson::from_document(bson::to_document(&assignment).unwrap()).unwrap();
        assert_eq!(roundtrip, assignment);
    }
}
