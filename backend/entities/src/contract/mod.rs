//! 域 D12 `contract`：contract、contract_revision（页面：W04）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.4；公共字段归属按 §4.3 判定：
//! - `contract` 是「稳定基础资料」→ 组合 [`crate::common::stable::StableBase`]；
//! - `contract_revision` 是「不可变修订」→ 组合 [`crate::common::revision::RevisionBase`]。
//! - 正式版本内联客户名称、合同编号、结算主体、税务与付款条件等结构化快照
//!   （数据模型 §4.4 / P1 §2.2），禁止 JSON blob。
//!
//! 快照值对象与 D13/D14/D15 需要的是同一组类型；`common/**` 在 P0 冻结（P1 §3
//! 跨域约束），各域各自定义同形结构，待 `chore/erp-p0-amend-*` 地基修订统一下沉到
//! `entities/src/common/`。

pub mod contract_revision;
mod entity;
pub mod snapshot;

pub use contract_revision::{ArchiveSource, ContractRevision, ContractRevisionData};
pub use entity::{Contract, ContractData, ContractStatus, ContractUpdate};
pub use snapshot::{
    CustomerSnapshot, InvoiceRequirementSnapshot, PaymentTermSnapshot, SettlementPartySnapshot,
};

/// 域内 ID newtype 的统一出口（实体层无跨域依赖，只引用 entities::ids）。
pub use crate::ids::{ContractId, ContractRevisionId};
