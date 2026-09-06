//! Cross-domain read models: workbench, customer center and fulfillment queue.

mod errors {
    pub type Error = services::Error;
    pub type Result<T> = services::Result<T>;
}

pub mod customer_center;
pub mod fulfillment_queue;
pub mod workbench;

pub use customer_center::CustomerCenterReadService;
pub use workbench::WorkbenchReadService;
