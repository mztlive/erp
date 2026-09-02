//! 客户归属写入命令：按动作拆成不可共存的 `Assign` / `End`。
//!
//! HTTP DTO 仍可携带与 `action` 不一致的可选字段；本模块只接受已经按动作
//! 拆开的必填组合，内部命令因此不存在非法状态。客户/账号存在性、重叠查询、
//! 事务和审计不在本层。

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

use super::customer_assignment::{
    AssignmentRole, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentId,
};
use super::CustomerAccountId;

/// 销售人员标识最大长度（与归属实体一致）。
const USER_ID_MAX_LEN: usize = 128;
/// 目标归属 ID 最大长度。
const ASSIGNMENT_ID_MAX_LEN: usize = 128;
/// 调整原因最大长度（与归属实体一致）。
const CHANGE_REASON_MAX_LEN: usize = 500;

/// 强类型客户归属写入命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomerAssignmentCommand {
    /// 建立新归属并结束重叠旧归属。
    Assign(AssignCustomerAssignment),
    /// 提前结束既有协作归属。
    End(EndCustomerAssignment),
}

/// 建立新归属所需的已规范化输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignCustomerAssignment {
    user_id: String,
    assignment_role: AssignmentRole,
    valid_from: BusinessDate,
    valid_to: Option<BusinessDate>,
    change_reason: String,
}

/// 提前结束既有归属所需的已规范化输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndCustomerAssignment {
    assignment_id: String,
    valid_to: BusinessDate,
    version: u64,
    change_reason: String,
}

impl AssignCustomerAssignment {
    /// 构造建立归属命令并完成输入规范化。
    ///
    /// # 参数
    /// * `user_id` - 销售人员账号 ID
    /// * `assignment_role` - 归属角色
    /// * `valid_from` - 生效开始日期
    /// * `valid_to` - 可选生效结束日期
    /// * `change_reason` - 调整原因
    ///
    /// # 返回
    /// 返回已去除首尾空白、窗口合法的建立命令。
    ///
    /// # 错误
    /// 销售人员或原因为空/超长，或结束日期不晚于开始日期时返回 [`Error::LogicError`]。
    ///
    /// # 关键业务约束
    /// 本命令不携带目标归属 ID 或乐观锁版本；那些字段属于结束动作。
    pub fn new(
        user_id: String,
        assignment_role: AssignmentRole,
        valid_from: BusinessDate,
        valid_to: Option<BusinessDate>,
        change_reason: String,
    ) -> Result<Self> {
        let user_id =
            normalize_required_text(user_id, "销售人员不能为空", USER_ID_MAX_LEN, "销售人员标识过长")?;
        let change_reason = normalize_required_text(
            change_reason,
            "调整原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "调整原因过长",
        )?;
        ensure_window_valid(valid_from, valid_to)?;
        Ok(Self {
            user_id,
            assignment_role,
            valid_from,
            valid_to,
            change_reason,
        })
    }

    /// 返回已规范化的销售人员 ID。
    ///
    /// # 返回
    /// 返回去空白后的账号 ID。
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// 返回归属角色。
    ///
    /// # 返回
    /// 返回 `OWNER` 或 `COLLABORATOR`。
    pub fn assignment_role(&self) -> AssignmentRole {
        self.assignment_role
    }

    /// 返回生效开始日期。
    ///
    /// # 返回
    /// 返回业务日期。
    pub fn valid_from(&self) -> BusinessDate {
        self.valid_from
    }

    /// 返回可选生效结束日期。
    ///
    /// # 返回
    /// 未指定结束日时返回 `None`。
    pub fn valid_to(&self) -> Option<BusinessDate> {
        self.valid_to
    }

    /// 返回已规范化的调整原因。
    ///
    /// # 返回
    /// 返回去空白后的原因文本。
    pub fn change_reason(&self) -> &str {
        &self.change_reason
    }

    /// 把命令转换为可持久化的归属实体。
    ///
    /// ID 与客户身份由调用方注入；本方法不生成 ID、不访问仓储。
    ///
    /// # 参数
    /// * `id` - 新归属主键
    /// * `customer_id` - 目标客户角色 ID
    ///
    /// # 返回
    /// 返回新建的归属实体。
    ///
    /// # 错误
    /// 窗口或文本不变量被破坏时返回 [`Error::LogicError`]。
    pub fn into_assignment(
        self,
        id: CustomerAssignmentId,
        customer_id: CustomerAccountId,
    ) -> Result<CustomerAssignment> {
        CustomerAssignment::new(
            id,
            CustomerAssignmentData {
                customer_id,
                user_id: self.user_id,
                assignment_role: self.assignment_role,
                valid_from: self.valid_from,
                valid_to: self.valid_to,
                change_reason: self.change_reason,
            },
        )
    }
}

impl EndCustomerAssignment {
    /// 构造结束归属命令并完成输入规范化。
    ///
    /// # 参数
    /// * `assignment_id` - 目标归属 ID
    /// * `valid_to` - 生效结束日期
    /// * `version` - 客户端期望的乐观锁版本
    /// * `change_reason` - 调整原因
    ///
    /// # 返回
    /// 返回已去除首尾空白且版本合法的结束命令。
    ///
    /// # 错误
    /// 归属 ID 或原因为空/超长，或版本小于 1 时返回 [`Error::LogicError`]。
    ///
    /// # 关键业务约束
    /// 本命令不携带销售人员、归属角色或生效开始日期；那些字段属于建立动作。
    /// 结束日相对既有 `valid_from` 的校验由实体 `end_directly` 执行。
    pub fn new(
        assignment_id: String,
        valid_to: BusinessDate,
        version: u64,
        change_reason: String,
    ) -> Result<Self> {
        if version < 1 {
            return Err(Error::from("乐观锁版本必须大于 0"));
        }
        let assignment_id = normalize_required_text(
            assignment_id,
            "目标归属 ID 不能为空",
            ASSIGNMENT_ID_MAX_LEN,
            "目标归属 ID 过长",
        )?;
        let change_reason = normalize_required_text(
            change_reason,
            "调整原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "调整原因过长",
        )?;
        Ok(Self {
            assignment_id,
            valid_to,
            version,
            change_reason,
        })
    }

    /// 返回已规范化的目标归属 ID。
    ///
    /// # 返回
    /// 返回去空白后的归属主键。
    pub fn assignment_id(&self) -> &str {
        &self.assignment_id
    }

    /// 返回结束日期。
    ///
    /// # 返回
    /// 返回业务日期。
    pub fn valid_to(&self) -> BusinessDate {
        self.valid_to
    }

    /// 返回乐观锁期望版本。
    ///
    /// # 返回
    /// 返回大于 0 的版本号。
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 返回已规范化的调整原因。
    ///
    /// # 返回
    /// 返回去空白后的原因文本。
    pub fn change_reason(&self) -> &str {
        &self.change_reason
    }
}

/// 校验生效区间：结束日必须晚于开始日。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 可选结束日期
///
/// # 返回
/// 窗口合法时返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回 [`Error::LogicError`]。
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
    use super::{AssignCustomerAssignment, AssignmentRole, EndCustomerAssignment};
    use crate::common::time::BusinessDate;
    use crate::errors::Error;
    use crate::ids::{CustomerAccountId, CustomerAssignmentId};

    fn date(year: i32, month: u32, day: u32) -> BusinessDate {
        BusinessDate::from_ymd(year, month, day).unwrap()
    }

    #[test]
    fn assign_normalizes_and_rejects_illegal_window() {
        let command = AssignCustomerAssignment::new(
            " sales-1 ".to_string(),
            AssignmentRole::Collaborator,
            date(2026, 8, 8),
            Some(date(2026, 12, 31)),
            " 联合跟进 ".to_string(),
        )
        .unwrap();
        assert_eq!(command.user_id(), "sales-1");
        assert_eq!(command.assignment_role(), AssignmentRole::Collaborator);
        assert_eq!(command.assignment_role().as_str(), "COLLABORATOR");
        assert_eq!(AssignmentRole::Owner.as_str(), "OWNER");
        assert_eq!(command.change_reason(), "联合跟进");

        let assignment = command
            .clone()
            .into_assignment(
                CustomerAssignmentId::new("asg-1"),
                CustomerAccountId::new("customer-1"),
            )
            .unwrap();
        assert_eq!(assignment.user_id, "sales-1");
        assert_eq!(assignment.customer_id, CustomerAccountId::new("customer-1"));

        let reversed = AssignCustomerAssignment::new(
            "sales-1".to_string(),
            AssignmentRole::Owner,
            date(2026, 8, 8),
            Some(date(2026, 8, 1)),
            "换任".to_string(),
        );
        assert!(matches!(reversed, Err(Error::LogicError(_))));
        assert!(AssignCustomerAssignment::new(
            "   ".to_string(),
            AssignmentRole::Owner,
            date(2026, 8, 8),
            None,
            "换任".to_string(),
        )
        .is_err());
    }

    #[test]
    fn end_requires_positive_version_and_assignment_id() {
        let command = EndCustomerAssignment::new(
            " asg-9 ".to_string(),
            date(2026, 9, 1),
            3,
            " 结束协作 ".to_string(),
        )
        .unwrap();
        assert_eq!(command.assignment_id(), "asg-9");
        assert_eq!(command.valid_to(), date(2026, 9, 1));
        assert_eq!(command.version(), 3);
        assert_eq!(command.change_reason(), "结束协作");

        assert!(matches!(
            EndCustomerAssignment::new("asg-9".to_string(), date(2026, 9, 1), 0, "结束".to_string()),
            Err(Error::LogicError(message)) if message.contains("乐观锁版本必须大于 0")
        ));
        assert!(
            EndCustomerAssignment::new("  ".to_string(), date(2026, 9, 1), 1, "结束".to_string(),).is_err()
        );
    }
}
