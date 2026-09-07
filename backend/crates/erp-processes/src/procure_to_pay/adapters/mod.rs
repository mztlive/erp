//! 采购消费方窄事实接口的组合层适配。
pub(crate) mod payment_term;
mod sales_allocation;
pub(crate) use sales_allocation::SalesAllocationAdapter;
pub(crate) mod audit;
