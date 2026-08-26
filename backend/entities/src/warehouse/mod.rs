//! 域 D11 `warehouse`：warehouse、warehouse_revision、warehouse_sku_policy
//! （页面：W14、W10）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.3；公共字段归属按 §4.3 判定：
//! - `warehouse` 是「稳定基础资料」→ 组合 [`crate::common::StableBase`]；
//! - `warehouse_revision` 是不可变修订 → 用 [`crate::common::RevisionBase`]
//!   （revision_no），正式版本按 §4.4 内联结构化快照字段，地址与联系人按
//!   §4.5.5 以加密值 + 带密钥 HMAC 指纹保存（[`SensitiveText`]）；
//! - `warehouse_sku_policy` 是库存预警策略行，只用 `BaseModel` 持久化元数据。
//!
//! 敏感字段指纹算法（HMAC-SHA256，带密钥、禁止裸摘要）当前因 `hmac`/`sha2`
//! 仅存在于 `[dev-dependencies]`（P0 修订提交 3786fac 放置），指纹函数仅在测试内
//! 定义；生产使用需地基修订将两依赖提升为正式依赖并把指纹值对象下沉 `common/`
//! 供 D07/D11 复用（见域报告「地基修订候选」）。

pub mod status;
pub mod warehouse_entity;
pub mod warehouse_revision;
pub mod warehouse_sku_policy;

pub use status::EnableStatus;
pub use warehouse_entity::{Warehouse, WarehouseFulfillmentOperation};
pub use warehouse_revision::SensitiveText;
pub use warehouse_revision::WarehouseRevision;
pub use warehouse_sku_policy::{WarehouseSkuPolicy, WarehouseSkuPolicyPeriod};

// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{WarehouseId, WarehouseRevisionId, WarehouseSkuPolicyId};
