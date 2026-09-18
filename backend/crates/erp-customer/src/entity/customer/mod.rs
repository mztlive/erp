//! 域 D08 `customer`：customer_account、customer_assignment（页面：W03、W15）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.2；公共字段归属按 §4.3 判定：
//! - `customer_account` 是「稳定基础资料」→ 组合 [`erp_core::common::StableBase`]；
//! - `customer_assignment` 是按有效期保存的归属事实行（§6.2），按字段字典
//!   精确建模：`BaseModel` + 归属字段 + 生效区间，不硬套 StableBase。

pub mod assignment_command;
pub mod customer_account;
pub mod customer_assignment;
pub mod customer_profile_command;
pub mod profile_validation;

pub use assignment_command::{AssignCustomerAssignment, CustomerAssignmentCommand, EndCustomerAssignment};
pub use customer_account::{
    CustomerAccount, CustomerAccountData, CustomerAccountStatus, CustomerAccountUpdate,
};
pub use customer_assignment::{
    AssignmentRole, CustomerAssignment, CustomerAssignmentData, CustomerAssignmentUpdate,
};
pub use customer_profile_command::{
    CustomerProfileCommand, CustomerProfileCommandData, CustomerProfileCommandResultData,
    CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
};
pub use erp_core::ids::{CustomerAccountId, CustomerAssignmentId};
pub use profile_validation::{
    CustomerProfileFactInput, CustomerProfileFactKind, CustomerProfileFactSet, CustomerProfileOperation,
    CustomerProfileRequestShape,
};

/// 校验乐观锁版本一致（客户账户与归属共用同一文案与语义）。
///
/// # 参数
/// * `current` - 当前持久化版本
/// * `expected` - 客户端期望版本
///
/// # 返回
/// 版本一致时返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回领域逻辑错误。
pub(crate) fn ensure_entity_version(current: u64, expected: u64) -> erp_core::Result<()> {
    if current == expected {
        return Ok(());
    }
    Err(erp_core::Error::from("数据已被其他请求修改，请刷新后重试"))
}
