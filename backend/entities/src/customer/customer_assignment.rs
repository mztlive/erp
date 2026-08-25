//! `customer_assignment`：客户归属（数据模型 §6.2，页面：W03）。
//!
//! 同一客户同一时点恰好一个 `OWNER`、同一客户/用户/角色的有效期不得
//! 重叠；区间与角色冲突由实体判定，P3 只负责事务内加载并持久化冲突行
//! （§6.2）。负责人变化后只影响新增单据权限，不删除历史参与权（W03 / §11.1）。

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
/// 直接结束协作归属（`Set` 时校验晚于 `valid_from`）；负责人必须换任。
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

    /// 直接结束协作归属。
    ///
    /// 负责人归属必须通过换任建立新的负责人后结束，不能被独立结束；
    /// 协作归属允许直接设置结束日期。
    ///
    /// # 参数
    /// * `valid_to` - 生效结束日期
    ///
    /// # 返回
    /// 结束成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 负责人归属被直接结束，或结束日期不晚于开始日期时返回错误。
    pub fn end_directly(&mut self, valid_to: BusinessDate) -> Result<()> {
        if self.assignment_role == AssignmentRole::Owner {
            return Err(Error::from("负责人不能直接结束，请通过换任建立新的负责人归属"));
        }
        self.set_valid_to(valid_to)
    }

    /// 兼容归属生命周期更新入口。
    ///
    /// 该入口与 [`Self::end_directly`] 使用相同领域规则，只允许直接结束
    /// 协作归属；负责人换任必须使用 [`Self::end_for_replacement`]。
    ///
    /// # 参数
    /// * `update` - 结束日期更新意图
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 负责人归属被直接结束，或结束日期不晚于开始日期时返回错误。
    pub fn update(&mut self, update: CustomerAssignmentUpdate) -> Result<()> {
        if let Some(valid_to) = update.valid_to.into_option() {
            self.end_directly(valid_to)?;
        }
        Ok(())
    }

    /// 在新归属冲突时结束当前归属。
    ///
    /// OWNER 与同一客户任一 OWNER 冲突；COLLABORATOR 只与同一客户、
    /// 同一用户的 COLLABORATOR 冲突。结束日为新区间开始日，保持开区间
    /// 无重叠且无空档。
    ///
    /// # 参数
    /// * `replacement` - 待建立的新归属
    ///
    /// # 返回
    /// 当前归属被结束时返回 `true`；无冲突时返回 `false`。
    ///
    /// # 错误
    /// 新归属开始日期不晚于冲突旧归属开始日期时返回错误。
    pub fn end_for_replacement(&mut self, replacement: &Self) -> Result<bool> {
        if !self.conflicts_with(replacement) {
            return Ok(false);
        }
        if replacement.valid_from <= self.valid_from {
            return Err(Error::from(
                "新归属开始日期必须晚于旧归属开始日期，请调整生效日期",
            ));
        }
        self.set_valid_to(replacement.valid_from)?;
        Ok(true)
    }

    /// 判断当前归属是否与待建立归属冲突。
    ///
    /// # 参数
    /// * `other` - 待比较的新归属
    ///
    /// # 返回
    /// 同一客户下角色/用户组合需要唯一且有效期重叠时返回 `true`。
    pub fn conflicts_with(&self, other: &Self) -> bool {
        if self.customer_id != other.customer_id || self.assignment_role != other.assignment_role {
            return false;
        }
        let role_conflicts = other.assignment_role == AssignmentRole::Owner || self.user_id == other.user_id;
        role_conflicts && windows_overlap(self.valid_from, self.valid_to, other.valid_from, other.valid_to)
    }

    /// 校验归属属于指定客户。
    ///
    /// # 参数
    /// * `customer_id` - 期望的客户角色 ID
    ///
    /// # 返回
    /// 归属客户匹配时返回 `Ok(())`。
    ///
    /// # 错误
    /// 归属不属于指定客户时返回错误。
    pub fn ensure_customer(&self, customer_id: &CustomerAccountId) -> Result<()> {
        if &self.customer_id == customer_id {
            return Ok(());
        }
        Err(Error::from("归属不属于该客户"))
    }

    /// 校验乐观锁版本。
    ///
    /// # 参数
    /// * `expected` - 客户端期望版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前版本与期望版本不一致时返回错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version == expected {
            return Ok(());
        }
        Err(Error::from("数据已被其他请求修改，请刷新后重试"))
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

    /// 设置结束日期并复用有效期不变式。
    ///
    /// # 参数
    /// * `valid_to` - 生效结束日期
    ///
    /// # 返回
    /// 设置成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 结束日期不晚于开始日期时返回错误。
    fn set_valid_to(&mut self, valid_to: BusinessDate) -> Result<()> {
        ensure_window_valid(self.valid_from, Some(valid_to))?;
        self.valid_to = Some(valid_to);
        Ok(())
    }
}

/// 判断两个生效区间是否重叠（结束日为开区间）。
///
/// # 参数
/// * `a_from` - 第一个区间开始日
/// * `a_to` - 第一个区间结束日；`None` 表示无穷远
/// * `b_from` - 第二个区间开始日
/// * `b_to` - 第二个区间结束日；`None` 表示无穷远
///
/// # 返回
/// 两个左闭右开区间存在交集时返回 `true`。
fn windows_overlap(
    a_from: BusinessDate,
    a_to: Option<BusinessDate>,
    b_from: BusinessDate,
    b_to: Option<BusinessDate>,
) -> bool {
    let a_covers = |day: BusinessDate| a_to.is_none_or(|end| day < end);
    let b_covers = |day: BusinessDate| b_to.is_none_or(|end| day < end);
    a_covers(b_from) && b_covers(a_from)
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

    /// 正常路径：协作归属可以直接结束，人员与角色保持不变。
    #[test]
    fn collaborator_can_end_directly() {
        let data = CustomerAssignmentData {
            assignment_role: AssignmentRole::Collaborator,
            ..assignment_data()
        };
        let mut assignment = CustomerAssignment::new(CustomerAssignmentId::new("assign-4"), data).unwrap();
        assignment
            .update(CustomerAssignmentUpdate {
                valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 3, 31).unwrap()),
            })
            .unwrap();
        assert_eq!(
            assignment.valid_to,
            Some(BusinessDate::from_ymd(2026, 3, 31).unwrap())
        );
        assert_eq!(assignment.user_id, "sales-zhangsan");
        assert_eq!(assignment.assignment_role, AssignmentRole::Collaborator);
    }

    /// 失败路径：负责人不能直接结束，倒挂结束日期也被拒绝。
    #[test]
    fn direct_end_rejects_owner_and_reversed_window() {
        let mut owner =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-owner"), assignment_data()).unwrap();
        assert!(owner
            .end_directly(BusinessDate::from_ymd(2026, 3, 31).unwrap())
            .is_err());

        let data = CustomerAssignmentData {
            assignment_role: AssignmentRole::Collaborator,
            ..assignment_data()
        };
        let mut collaborator =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-collaborator"), data).unwrap();
        assert!(collaborator
            .end_directly(BusinessDate::from_ymd(2025, 1, 1).unwrap())
            .is_err());
    }

    /// 冲突规则：负责人跨用户冲突，协作者只与同用户冲突。
    #[test]
    fn replacement_conflicts_follow_role_and_user_rules() {
        let mut owner =
            CustomerAssignment::new(CustomerAssignmentId::new("owner-old"), assignment_data()).unwrap();
        let new_owner_data = CustomerAssignmentData {
            user_id: "sales-lisi".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 6, 1).unwrap(),
            valid_to: None,
            ..assignment_data()
        };
        let new_owner =
            CustomerAssignment::new(CustomerAssignmentId::new("owner-new"), new_owner_data).unwrap();
        assert!(owner.conflicts_with(&new_owner));
        assert!(owner.end_for_replacement(&new_owner).unwrap());
        assert_eq!(owner.valid_to, Some(new_owner.valid_from));

        let collaborator_data = CustomerAssignmentData {
            assignment_role: AssignmentRole::Collaborator,
            ..assignment_data()
        };
        let collaborator = CustomerAssignment::new(
            CustomerAssignmentId::new("collaborator-old"),
            collaborator_data.clone(),
        )
        .unwrap();
        let other_user_data = CustomerAssignmentData {
            user_id: "sales-lisi".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 6, 1).unwrap(),
            ..collaborator_data
        };
        let other_user =
            CustomerAssignment::new(CustomerAssignmentId::new("collaborator-new"), other_user_data).unwrap();
        assert!(!collaborator.conflicts_with(&other_user));
    }

    /// 边界路径：结束日为开区间，相接窗口不冲突；同日起点无法换任。
    #[test]
    fn replacement_respects_exclusive_end_and_start_boundary() {
        let mut old = CustomerAssignment::new(CustomerAssignmentId::new("old"), assignment_data()).unwrap();
        let adjacent_data = CustomerAssignmentData {
            valid_from: BusinessDate::from_ymd(2026, 12, 31).unwrap(),
            valid_to: None,
            ..assignment_data()
        };
        let adjacent = CustomerAssignment::new(CustomerAssignmentId::new("adjacent"), adjacent_data).unwrap();
        assert!(!old.conflicts_with(&adjacent));
        assert!(!old.end_for_replacement(&adjacent).unwrap());

        let same_start = CustomerAssignment::new(
            CustomerAssignmentId::new("same-start"),
            CustomerAssignmentData {
                valid_to: None,
                ..assignment_data()
            },
        )
        .unwrap();
        assert!(old.end_for_replacement(&same_start).is_err());
    }

    /// 版本与客户归属校验覆盖成功和失败路径。
    #[test]
    fn version_and_customer_identity_are_enforced() {
        let assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-version"), assignment_data()).unwrap();
        assert!(assignment.ensure_version(1).is_ok());
        assert!(assignment.ensure_version(2).is_err());
        assert!(assignment
            .ensure_customer(&CustomerAccountId::new("customer-1"))
            .is_ok());
        assert!(assignment
            .ensure_customer(&CustomerAccountId::new("customer-2"))
            .is_err());
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let assignment =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-5"), assignment_data()).unwrap();
        let roundtrip: CustomerAssignment =
            bson::deserialize_from_document(bson::serialize_to_document(&assignment).unwrap()).unwrap();
        assert_eq!(roundtrip, assignment);
    }
}
