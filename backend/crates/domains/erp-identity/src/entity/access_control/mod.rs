//! 身份权限、人员数据范围及审计事件的领域模型。

pub mod audit_actions;
pub mod audit_event;
pub(crate) mod governance;
pub mod person_scope;
pub mod resolved_scope;
pub(crate) mod scope_spec;
pub mod scope_type;
pub use audit_event::{AuditEvent, AuditEventData, AuditEventResult};
pub use erp_core::ids::AuditEventId;
pub use resolved_scope::{ResolvedScope, ScopeClause, ScopedObject};
pub use scope_type::{DataScopeType, ScopeDimension, ScopeTargetMode};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids；
// `role` 沿用 rbac::RoleId，见 ids.rs 映射表）。
pub use crate::entity::rbac::RoleId;

pub mod paging;

pub(crate) mod scope_target;
