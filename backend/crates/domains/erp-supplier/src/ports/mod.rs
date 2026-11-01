//! 主体事实、敏感令牌与资质附件的消费方端口。

mod account;
mod data_scope;
mod file_asset;
mod party;
mod sensitive;

pub use account::{AccountFactPort, FailClosedAccountFactPort};
pub use data_scope::{
    FailClosedSupplierDataScopePort, SupplierDataScopePort, SupplierResolvedClause, SupplierResolvedScope,
    SupplierScopeObject,
};
pub use file_asset::{EmptyFileAssetFacts, FileAssetFact, FileAssetFactsPort};
pub use party::{
    AddressTypeFact, EffectiveRecordStatusFact, EmptyPartyFacts, PartyAddressFact, PartyBankAccountFact,
    PartyContactFact, PartyFactsPort, PartyListFact, PartyRevisionFact, PartyStatusFact, PartyTaxProfileFact,
    select_current_default,
};
pub use sensitive::{EmptySensitiveTokens, SensitiveFieldKindFact, SensitiveTokenPort};
