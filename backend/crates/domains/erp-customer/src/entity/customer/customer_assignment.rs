//! `customer_assignment`：客户归属（数据模型 §6.2，页面：W03）。
//!
//! 同一客户允许多个 `OWNER`，同一客户/用户/角色的有效期不得
//! 重叠；区间与角色冲突由实体判定，P3 只负责事务内加载并持久化冲突行
//! （§6.2）。负责人变化后只影响新增单据权限，不删除历史参与权（W03 / §11.1）。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::field_update::FieldUpdate;
pub use erp_core::ids::{CustomerAccountId, CustomerAssignmentId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 用户标识最大长度（归属实体与归属命令共用）。
pub(crate) const USER_ID_MAX_LEN: usize = 128;
/// 调整原因最大长度（归属实体与归属命令共用）。
pub(crate) const CHANGE_REASON_MAX_LEN: usize = 500;
/// 目标归属 ID 最大长度（结束归属命令使用）。
pub(crate) const ASSIGNMENT_ID_MAX_LEN: usize = 128;

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
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Owner => "负责销售",
            Self::Collaborator => "协作销售",
        }
    }

    /// 返回角色的稳定代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    ///
    /// # 错误
    /// 不返回错误。
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
    /// * `id` - 实体主键（`erp_core::ids::CustomerAssignmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的归属实体。
    ///
    /// # 错误
    /// 当 user_id / change_reason 为空或超长，或生效区间倒挂时返回错误。
    pub fn new(id: CustomerAssignmentId, data: CustomerAssignmentData) -> Result<Self> {
        let user_id = normalize_user_id(data.user_id)?;
        let change_reason = normalize_change_reason(data.change_reason)?;
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

    /// 结束指定人员的负责或协作关系。
    ///
    /// 本方法只维护单行有效期；最后一名负责销售的限制由事务用例校验。
    ///
    /// # 参数
    /// * `valid_to` - 生效结束日期
    ///
    /// # 返回
    /// 结束成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 结束日期不晚于开始日期，或晚于既有结束日期时返回错误。
    pub fn end_directly(&mut self, valid_to: BusinessDate) -> Result<()> {
        if self.valid_to.is_some_and(|current| valid_to > current) {
            return Err(Error::from("结束负责关系只能缩短有效期，不能延后原结束日期"));
        }
        self.set_valid_to(valid_to)
    }

    /// 兼容归属生命周期更新入口。
    ///
    /// 该入口与 [`Self::end_directly`] 使用相同领域规则，只允许直接结束
    /// 负责或协作归属；其他人员的关系不受影响。
    ///
    /// # 参数
    /// * `update` - 结束日期更新意图
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 结束日期不晚于开始日期时返回错误。
    pub fn update(&mut self, update: CustomerAssignmentUpdate) -> Result<()> {
        if let Some(valid_to) = update.valid_to.into_option() {
            self.end_directly(valid_to)?;
        }
        Ok(())
    }

    /// 在新归属冲突时结束当前归属。
    ///
    /// 同一客户、同一用户、同一角色的重叠关系才冲突。结束日为新区间开始日，保持开区间
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
            return Err(Error::from("新归属开始日期必须晚于旧归属开始日期，请调整生效日期"));
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
    ///
    /// # 错误
    /// 不返回错误。
    pub fn conflicts_with(&self, other: &Self) -> bool {
        if self.customer_id != other.customer_id || self.assignment_role != other.assignment_role {
            return false;
        }
        let role_conflicts = self.user_id == other.user_id;
        role_conflicts && windows_overlap(self.valid_from, self.valid_to, other.valid_from, other.valid_to)
    }

    /// 拒绝同客户、同人员、同角色的重叠负责区间。
    ///
    /// # 参数
    /// * `existing` - 原写入事务读取的人员关系
    /// # 返回
    /// 没有重叠区间时返回成功。
    /// # 错误
    /// 同人同角色已有重叠关系时返回错误。
    pub fn ensure_no_overlap(&self, existing: &[Self]) -> Result<()> {
        if existing.iter().any(|item| !item.base.is_deleted() && self.conflicts_with(item)) {
            return Err(Error::from("该销售在此期间已负责该客户，请调整有效期"));
        }
        Ok(())
    }

    /// 校验被缩短的整个区间仍由其他有效负责销售连续覆盖。
    ///
    /// # 参数
    /// * `remaining` - 同一事务读取的客户全部人员关系
    /// * `at` - 拟提前结束日期
    /// # 返回
    /// 从新结束日到原结束日连续有人负责时返回成功。
    /// # 错误
    /// 被移除的任意区间无人负责时返回错误。
    pub fn ensure_owner_retained(&self, remaining: &[Self], at: BusinessDate) -> Result<()> {
        if self.assignment_role != AssignmentRole::Owner || self.valid_to.is_some_and(|end| at >= end) {
            return Ok(());
        }
        let mut owners = remaining
            .iter()
            .filter(|item| {
                !item.base.is_deleted()
                    && item.base.id != self.base.id
                    && item.customer_id == self.customer_id
                    && item.assignment_role == AssignmentRole::Owner
            })
            .collect::<Vec<_>>();
        owners.sort_by_key(|item| item.valid_from);
        let mut covered_until = at;
        for owner in owners {
            if owner.valid_from > covered_until {
                break;
            }
            let Some(end) = owner.valid_to else { return Ok(()) };
            covered_until = covered_until.max(end);
            if self.valid_to.is_some_and(|end| covered_until >= end) {
                return Ok(());
            }
        }
        Err(Error::from("至少保留一名有效负责销售，请先添加接任人员"))
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
        super::ensure_entity_version(self.base.version, expected)
    }

    /// 判断归属当前是否有效（按业务日期判定有效期，结束日为开区间）。
    ///
    /// # 参数
    /// * `as_of` - 业务日期
    ///
    /// # 返回
    /// `as_of` 不早于 `valid_from`，且早于 `valid_to` 时返回 `true`；`valid_to` 为 `None` 时开始日及之后有效。
    ///
    /// # 错误
    /// 不返回错误。
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

impl CustomerAssignment {
    /// 返回去重并稳定排序的负责销售账号。
    ///
    /// # 参数
    /// * `assignments` - 已按有效期读取的客户人员关系
    /// # 返回
    /// 返回全部未删除负责销售账号。
    /// # 错误
    /// 无。
    pub fn owner_ids(assignments: &[Self]) -> Vec<String> {
        let mut ids = assignments
            .iter()
            .filter(|item| !item.base.is_deleted() && item.assignment_role == AssignmentRole::Owner)
            .map(|item| item.user_id.clone())
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        ids
    }

    /// 解析同一客户归属行的主负责人与协作计数。
    ///
    /// # 参数
    /// * `assignments` - 同一客户的归属行
    /// * `account_names` - 账号显示名
    ///
    /// # 返回
    /// 返回切片中首个 `Owner` 的账号 ID 与显示名，以及 `Collaborator` 行数。没有 `Owner` 时 ID 为 `None`；ID 不在 `account_names` 中时显示名为 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn owner_summary(
        assignments: &[Self],
        account_names: &HashMap<String, String>,
    ) -> (Option<String>, Option<String>, u32) {
        let owner_user_id = assignments
            .iter()
            .find(|assignment| assignment.assignment_role == AssignmentRole::Owner)
            .map(|assignment| assignment.user_id.clone());
        let owner_user_name = owner_user_id.as_ref().and_then(|id| account_names.get(id)).cloned();
        let collaborator_count = assignments
            .iter()
            .filter(|assignment| assignment.assignment_role == AssignmentRole::Collaborator)
            .count() as u32;
        (owner_user_id, owner_user_name, collaborator_count)
    }
}

/// 规范化销售人员 ID（归属实体与归属命令共用同一文案与长度）。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回去空白后的账号 ID。
///
/// # 错误
/// 为空或超长时返回领域逻辑错误。
pub(crate) fn normalize_user_id(value: String) -> Result<String> {
    normalize_required_text(value, "销售人员不能为空", USER_ID_MAX_LEN, "销售人员标识过长")
}

/// 规范化归属调整原因（归属实体与归属命令共用同一文案与长度）。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回去空白后的原因文本。
///
/// # 错误
/// 为空或超长时返回领域逻辑错误。
pub(crate) fn normalize_change_reason(value: String) -> Result<String> {
    normalize_required_text(value, "调整原因不能为空", CHANGE_REASON_MAX_LEN, "调整原因过长")
}

/// 规范化结束归属的目标归属 ID。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回去空白后的归属主键。
///
/// # 错误
/// 为空或超长时返回领域逻辑错误。
pub(crate) fn normalize_assignment_id(value: String) -> Result<String> {
    normalize_required_text(value, "目标归属 ID 不能为空", ASSIGNMENT_ID_MAX_LEN, "目标归属 ID 过长")
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

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`（erp-customer-002）。
///
/// 窗口不变式的权威实现：实体构造、实体结束日期设置与命令层构造均复用本函数；
/// DTO 只做字段组合分流，不再各自实现同语义校验。
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
pub(crate) fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to
        && valid_to <= valid_from
    {
        return Err(Error::from("生效结束日期必须晚于生效开始日期"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use erp_core::common::time::BusinessDate;
    use erp_core::field_update::FieldUpdate;
    use erp_core::ids::{CustomerAccountId, CustomerAssignmentId};

    use super::{AssignmentRole, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentUpdate};

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
        let blank_user = CustomerAssignmentData { user_id: "   ".to_string(), ..assignment_data() };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), blank_user).is_err());

        let blank_reason = CustomerAssignmentData { change_reason: "   ".to_string(), ..assignment_data() };
        assert!(CustomerAssignment::new(CustomerAssignmentId::new("a"), blank_reason).is_err());

        let overlong_user = CustomerAssignmentData { user_id: "u".repeat(129), ..assignment_data() };
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
        assert!(!assignment.is_active_on(BusinessDate::from_ymd(2026, 12, 31).unwrap()), "结束日不包含");
        assert!(!assignment.is_active_on(BusinessDate::from_ymd(2025, 12, 31).unwrap()));

        let open = CustomerAssignmentData { valid_to: None, ..assignment_data() };
        let open_assignment = CustomerAssignment::new(CustomerAssignmentId::new("assign-3"), open).unwrap();
        assert!(open_assignment.is_active_on(BusinessDate::from_ymd(2030, 1, 1).unwrap()));
    }

    /// 正常路径：协作归属可以直接结束，人员与角色保持不变。
    #[test]
    fn collaborator_can_end_directly() {
        let data =
            CustomerAssignmentData { assignment_role: AssignmentRole::Collaborator, ..assignment_data() };
        let mut assignment = CustomerAssignment::new(CustomerAssignmentId::new("assign-4"), data).unwrap();
        assignment
            .update(CustomerAssignmentUpdate {
                valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 3, 31).unwrap()),
            })
            .unwrap();
        assert_eq!(assignment.valid_to, Some(BusinessDate::from_ymd(2026, 3, 31).unwrap()));
        assert_eq!(assignment.user_id, "sales-zhangsan");
        assert_eq!(assignment.assignment_role, AssignmentRole::Collaborator);
    }

    /// 负责销售可以结束单行关系，倒挂结束日期被拒绝。
    #[test]
    fn direct_end_allows_owner_and_rejects_reversed_window() {
        let mut owner =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-owner"), assignment_data()).unwrap();
        assert!(owner.end_directly(BusinessDate::from_ymd(2026, 3, 31).unwrap()).is_ok());

        let data =
            CustomerAssignmentData { assignment_role: AssignmentRole::Collaborator, ..assignment_data() };
        let mut collaborator =
            CustomerAssignment::new(CustomerAssignmentId::new("assign-collaborator"), data).unwrap();
        assert!(collaborator.end_directly(BusinessDate::from_ymd(2025, 1, 1).unwrap()).is_err());
    }

    /// 不同负责销售可以同时有效；同人同角色仍拒绝重叠。
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
        assert!(!owner.conflicts_with(&new_owner));
        assert!(!owner.end_for_replacement(&new_owner).unwrap());
        assert_eq!(owner.valid_to, assignment_data().valid_to);
        assert!(new_owner.ensure_no_overlap(&[owner.clone()]).is_ok());
        assert!(owner.ensure_no_overlap(&[owner.clone()]).is_err());
        let at = BusinessDate::from_ymd(2026, 6, 1).unwrap();
        assert!(owner.ensure_owner_retained(&[new_owner], at).is_ok());
        assert!(owner.ensure_owner_retained(&[], at).is_err());

        let collaborator_data =
            CustomerAssignmentData { assignment_role: AssignmentRole::Collaborator, ..assignment_data() };
        let collaborator =
            CustomerAssignment::new(CustomerAssignmentId::new("collaborator-old"), collaborator_data.clone())
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
            CustomerAssignmentData { valid_to: None, ..assignment_data() },
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
        assert!(assignment.ensure_customer(&CustomerAccountId::new("customer-1")).is_ok());
        assert!(assignment.ensure_customer(&CustomerAccountId::new("customer-2")).is_err());
    }

    /// 规范化 helper：去空白成功，空值与超长失败。
    #[test]
    fn normalize_helpers_trim_and_reject() {
        assert_eq!(super::normalize_user_id(" sales-1 ".to_string()).unwrap(), "sales-1");
        assert_eq!(super::normalize_change_reason(" 换任 ".to_string()).unwrap(), "换任");
        assert_eq!(super::normalize_assignment_id(" asg-1 ".to_string()).unwrap(), "asg-1");
        assert!(super::normalize_user_id("   ".to_string()).is_err());
        assert!(super::normalize_change_reason("   ".to_string()).is_err());
        assert!(super::normalize_assignment_id("   ".to_string()).is_err());
        assert!(super::normalize_user_id("u".repeat(129)).is_err());
    }

    /// 负责人汇总：解析主负责人显示名并统计协作人数。
    #[test]
    fn owner_summary_resolves_names_and_counts() {
        let owner = CustomerAssignment::new(CustomerAssignmentId::new("o"), assignment_data()).unwrap();
        let collaborator = CustomerAssignment::new(
            CustomerAssignmentId::new("c"),
            CustomerAssignmentData {
                assignment_role: AssignmentRole::Collaborator,
                user_id: "sales-lisi".to_string(),
                ..assignment_data()
            },
        )
        .unwrap();
        let names = [("sales-zhangsan".to_string(), "张三".to_string())].into_iter().collect();
        let (id, name, count) = CustomerAssignment::owner_summary(&[owner, collaborator], &names);
        assert_eq!(id.as_deref(), Some("sales-zhangsan"));
        assert_eq!(name.as_deref(), Some("张三"));
        assert_eq!(count, 1);
        let (empty_id, _, empty_count) = CustomerAssignment::owner_summary(&[], &names);
        assert_eq!(empty_id, None);
        assert_eq!(empty_count, 0);
    }
    #[test]
    fn multiple_owners_are_listed_and_last_owner_cannot_end() {
        let first = CustomerAssignment::new(CustomerAssignmentId::new("a"), assignment_data()).unwrap();
        let second = CustomerAssignment::new(
            CustomerAssignmentId::new("b"),
            CustomerAssignmentData { user_id: "sales-other".into(), valid_to: None, ..assignment_data() },
        )
        .unwrap();
        let at = BusinessDate::from_ymd(2026, 6, 1).unwrap();
        assert_eq!(CustomerAssignment::owner_ids(&[first.clone(), second.clone()]).len(), 2);
        assert!(second.ensure_no_overlap(slice::from_ref(&first)).is_ok());
        assert!(first.ensure_owner_retained(&[second], at).is_ok());
        assert!(first.ensure_owner_retained(&[], at).is_err());
    }

    fn date(day: u32) -> BusinessDate {
        BusinessDate::from_ymd(2026, 10, day).unwrap()
    }

    fn window(id: &str, from: u32, to: Option<u32>) -> CustomerAssignment {
        CustomerAssignment::new(
            CustomerAssignmentId::new(id),
            CustomerAssignmentData {
                user_id: id.into(),
                valid_from: date(from),
                valid_to: to.map(date),
                ..assignment_data()
            },
        )
        .unwrap()
    }

    #[test]
    fn staggered_ends_cannot_leave_a_future_owner_gap() {
        let a = window("a", 1, None);
        let mut b = window("b", 1, None);
        assert!(b.ensure_owner_retained(slice::from_ref(&a), date(25)).is_ok());
        b.end_directly(date(25)).unwrap();
        assert!(a.ensure_owner_retained(&[b], date(24)).is_err());
    }

    #[test]
    fn remaining_owners_must_cover_the_entire_removed_interval() {
        let a = window("a", 1, None);
        let b = window("b", 1, Some(25));
        let c = window("c", 25, None);
        assert!(a.ensure_owner_retained(&[c.clone(), b.clone()], date(24)).is_ok());
        let late = window("late", 26, None);
        assert!(a.ensure_owner_retained(&[b.clone(), late], date(24)).is_err());
        let finite = window("finite", 1, Some(25));
        assert!(finite.ensure_owner_retained(&[b], date(24)).is_ok());
        assert!(finite.ensure_owner_retained(&[c], date(24)).is_err());
    }

    #[test]
    fn retained_owner_excludes_self_deleted_other_customer_and_collaborator() {
        let a = window("a", 1, None);
        let mut deleted = window("deleted", 1, None);
        deleted.base.deleted_at = 1;
        let mut foreign = window("foreign", 1, None);
        foreign.customer_id = CustomerAccountId::new("other");
        let mut collaborator = window("collaborator", 1, None);
        collaborator.assignment_role = AssignmentRole::Collaborator;
        assert!(
            a.ensure_owner_retained(&[a.clone(), deleted, foreign, collaborator.clone()], date(24)).is_err()
        );
        assert!(collaborator.ensure_owner_retained(&[], date(24)).is_ok());
    }

    #[test]
    fn ending_never_extends_a_previously_closed_interval() {
        for role in [AssignmentRole::Owner, AssignmentRole::Collaborator] {
            let mut assignment = window("a", 1, Some(25));
            assignment.assignment_role = role;
            assert!(assignment.end_directly(date(26)).is_err());
            assert_eq!(assignment.valid_to, Some(date(25)));
            assert!(assignment.end_directly(date(25)).is_ok());
            assert!(assignment.end_directly(date(24)).is_ok());
            assert!(assignment.end_directly(date(1)).is_err());
            assert_eq!(assignment.valid_to, Some(date(24)));
        }
    }
}
