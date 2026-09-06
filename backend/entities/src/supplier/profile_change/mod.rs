//! 供应商资料根修订的领域变更计划（`PROC-E06`）。
//!
//! 集中资料修订中的状态迁移、旧默认事实停用、能力启停及资质字段变更等纯规则；
//! 不触及 MongoDB、HTTP、全局时钟或全局 ID，Service 显式注入已加载事实、
//! 新 ID、修订号、业务日与操作人。

mod party;
mod plan;
mod qualification;
mod types;

pub use party::{
    disable_addresses, disable_bank_accounts, disable_contacts, disable_tax_profiles,
    plan_commercial_profile_revision, plan_party_revision, PlanCommercialProfileRevisionParams,
    PlanPartyRevisionParams,
};
pub use plan::{CapabilityToggle, PlannedQualificationInput, SupplierProfileChangePlan};
pub use qualification::{
    apply_qualification_input, new_capability, new_qualification, NewQualificationParams,
};

#[cfg(test)]
mod tests;
