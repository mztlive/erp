//! CLI 自有装配；运维入口不依赖 HTTP 应用。
mod audit_conversion;
mod identity;
mod identity_audit;
mod scope_target_validation;
pub use identity::shared_rbac_service;
pub use scope_target_validation::scope_configuration;
