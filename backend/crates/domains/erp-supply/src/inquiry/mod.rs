//! 询价内部专项报价及费用收款依据；不与销售结果共享响应结构。
mod balance;
mod fee;
pub(crate) mod indexes;
mod quote;
mod repository;
mod service;
mod terms;

pub use balance::{InquiryQuoteBalance, InquiryQuoteReservation, QuoteReservationHistory};
pub use fee::{InquiryFeeBasis, InquiryFeeBasisData};
pub use quote::{InquiryQuote, InquiryQuoteData, InquiryQuoteScope};
pub use repository::{
    InquiryFeeBasisRepositoryExt, InquiryQuoteBalanceRepositoryExt, InquiryQuoteRepositoryExt,
    InquiryQuoteReservationRepositoryExt, InquirySupplyExt,
};
pub use service::{InquirySupplyService, QuoteReservationInput};
pub use terms::{InquiryQuoteUse, QuoteAddressLimit, QuoteTerms};

#[cfg(test)]
mod tests;
