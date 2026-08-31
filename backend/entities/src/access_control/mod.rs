//! 域 D06 `access_control`：role、permission、user_role、data_scope、audit_event（页面：W19）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype、`entities::rbac`
//! 的既有 RBAC 基元（`RoleId` / `Permission`）与 `common` 基元。
//! - `role` 沿用既有 `entities::role::Role` / `entities::rbac::RoleId`（P0 前
//!   已存在且带解析校验），本域不重定义；
//! - `permission` 是「配置化权限」的定义目录（§5.1），模块与动作权限的
//!   授权绑定仍由既有 Casbin 规则承载，本实体不做重建；
//! - `audit_event` 与既有 `audit_log` 字段对齐（domains.md 注：`audit_log →
//!   audit_event` 字段对齐），字段口径见 W19 §5.2 审计事件与数据模型 §4.5.4
//!   （敏感字段只记录「已变更」和摘要，不记录完整旧值或新值）；
//! - `data_scope` 按 W19 §5.1 数据范围（公司、组织、团队、本人负责、协作等
//!   固定策略）建模，是本域的新增能力。

pub mod audit_event;
pub mod data_scope;
pub mod permission;
pub mod responsibility_scope;
pub mod user_role;

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids；
// `role` 沿用 rbac::RoleId，见 ids.rs 映射表）。
pub use crate::ids::{AuditEventId, DataScopeId, PermissionId, UserRoleId};
pub use crate::rbac::RoleId;
pub use audit_event::{AuditEvent, AuditEventData, AuditEventResult};
pub use data_scope::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
pub use permission::{Permission, PermissionData, PermissionUpdate};
pub use responsibility_scope::{OrganizationCoverage, ResponsibilityScopeSet};
pub use user_role::{UserRole, UserRoleData, UserRoleRevokeData};
