//! 销售 MongoDB 仓储、查询事实与集合访问器。

pub mod extensions;
pub(crate) mod filter;
mod fulfillment_facts;
pub mod owned;
pub mod prelude;
pub mod sales_order;
pub mod sales_review;
pub mod sales_selection;

pub use extensions::{SalesOrderExt, SalesReviewExt, SalesSelectionExt};
pub use prelude::{
    SalesChangeOrderRepositoryExt, SalesChangeSubmissionLineRepositoryExt,
    SalesChangeSubmissionRepositoryExt, SalesOrderGoodsServiceLineRevisionRepositoryExt,
    SalesOrderLineRepositoryExt, SalesOrderRepositoryExt, SalesOrderRepositoryProfitLossExt,
    SalesOrderRepositoryQualityExt, SalesOrderRepositoryScopeExt, SalesOrderRevisionLineRepositoryExt,
    SalesOrderRevisionLineRepositoryFulfillmentExt, SalesOrderRevisionRepositoryExt,
    SalesOrderSubmissionLineRepositoryExt, SalesOrderSubmissionRepositoryExt,
    SalesOrderVoucherLineRevisionRepositoryExt, SalesOrderWorkingCopyLineRepositoryExt,
    SalesOrderWorkingCopyRepositoryExt, SalesSelectionBookletRepositoryExt,
    SalesSelectionDisplayItemRepositoryExt, SalesSelectionIdempotencyRepositoryExt,
    SalesSelectionPoolMemberRepositoryExt, SalesSelectionPrepareTaskRepositoryExt,
    SalesSelectionProposalDisplayLineRepositoryExt, SalesSelectionProposalRepositoryExt,
    SalesSelectionProposalSkuLineRepositoryExt, SalesSelectionSessionRepositoryExt,
};

#[cfg(test)]
mod serialization_contract;
