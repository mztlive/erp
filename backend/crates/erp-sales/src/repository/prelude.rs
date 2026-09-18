//! Extension traits for collection repositories.

pub use super::fulfillment_facts::SalesOrderRevisionLineRepositoryFulfillmentExt;
pub use super::sales_order::{
    SalesOrderGoodsServiceLineRevisionRepositoryExt, SalesOrderLineRepositoryExt, SalesOrderRepositoryExt,
    SalesOrderRepositoryProfitLossExt, SalesOrderRepositoryQualityExt, SalesOrderRepositoryScopeExt,
    SalesOrderRevisionLineRepositoryExt, SalesOrderRevisionRepositoryExt,
    SalesOrderSubmissionLineRepositoryExt, SalesOrderSubmissionRepositoryExt,
    SalesOrderVoucherLineRevisionRepositoryExt, SalesOrderWorkingCopyLineRepositoryExt,
    SalesOrderWorkingCopyRepositoryExt,
};
pub use super::sales_review::{
    SalesChangeOrderRepositoryExt, SalesChangeSubmissionLineRepositoryExt, SalesChangeSubmissionRepositoryExt,
};
pub use super::sales_selection::{
    SalesSelectionBookletRepositoryExt, SalesSelectionDisplayItemRepositoryExt,
    SalesSelectionIdempotencyRepositoryExt, SalesSelectionPoolMemberRepositoryExt,
    SalesSelectionPrepareTaskRepositoryExt, SalesSelectionProposalDisplayLineRepositoryExt,
    SalesSelectionProposalRepositoryExt, SalesSelectionProposalSkuLineRepositoryExt,
    SalesSelectionSessionRepositoryExt,
};
