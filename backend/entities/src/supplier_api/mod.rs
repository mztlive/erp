//! 域 D25 `supplier_api`：supplier_api_connection、supplier_api_capability（页面：W20）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与约束见数据模型 §6.14；公共字段归属按 §4.3 判定：
//! - `supplier_api_connection` 是稳定连接配置（字典含启停/连接状态）→ 组合
//!   [`crate::common::stable::StableBase`]，连接键与健康检查字段按字典精确建模；
//! - `supplier_api_capability` 是连接下的能力声明注册行（字典含启停状态，无版本
//!   对象与审计字段）→ 只用 `BaseModel` 持久化元数据，`status` 按 §6.14 建模，
//!   不硬套 StableBase 的 `current_revision_id`/`created_by` 语义
//!   （`source_registry.external_identity_map` 同款注册表判定）。
//!
//! 敏感字段：连接密钥只保存密钥管理系统引用（`credential_reference`，
//! 数据模型 §6.14「不保存明文密钥」、phase-2 §14.1「连接密钥只保存密钥管理系统
//! 引用，不得在业务表和操作日志保存明文」），业务表和日志不落明文密钥，因此
//! 本域没有 §4.5.5 密文列/HMAC 查询指纹的字典字段落点。
//!
//! # 跨聚合不变式（P3，§8 无对应条目，契约来源：§6.14、phase-2 §6.2）
//! - `connection_code` 唯一（§6.14 必需约束）；
//! - `(connection_id, capability_code)` 唯一（§6.14 必需约束）；
//! - `supplier_id + status` 查询索引（§6.14）；
//! - 连接启用前地址配置/密钥引用就绪，业务模块只调用统一 Supplier Connector，
//!   不直接依赖供应商专用协议（phase-2 §6.2）。

pub mod capability;
pub mod capability_change_set;
pub mod connection;
pub mod governance;
pub mod prepared_commands;

pub use capability::{
    SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiCapabilityData, SupplierApiCapabilityStatus,
    SupplierApiCapabilityUpdate,
};
pub use capability_change_set::{
    CapabilityChangeInput, CapabilityChangeSet, CapabilityChangeSetRejection, ClassifiedCapabilityChangeSet,
    PendingCapabilityChange, ValidatedCapabilityChange,
};
pub use connection::{
    ConnectionEnvironment, HealthCheckResult, RateLimitPolicy, SupplierApiConnection,
    SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierApiConnectionUpdate,
};
pub use governance::{
    ensure_unique_capability_codes, BusinessCapabilityConfirmation, BusinessCapabilityConfirmationData,
    BusinessCapabilityRequirement, CapabilityVersionSnapshot, SupplierCommandOutcome,
    SupplierConnectionAction, SupplierConnectionBusinessImpact, SupplierConnectionCommandReceipt,
    SupplierConnectionCommandReceiptData, SupplierConnectionGovernance, SupplierGovernanceBlocker,
    SupplierHealthCheckRun, SupplierHealthCheckRunData, SupplierHealthCheckStatus, SupplierHealthCheckType,
};
pub use prepared_commands::{
    CommandOptionalField, PreparedSupplierConnectionCommand, PreparedSupplierConnectionCreate,
    SupplierCommandShapeRejection,
};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
