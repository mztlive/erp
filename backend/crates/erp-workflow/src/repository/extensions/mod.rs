//! MongoDB `Database` accessors for workflow collections.

mod approval_integration;
mod bpm;
mod document_registry;
mod work_item;

pub use approval_integration::ApprovalIntegrationExt;
pub use bpm::BpmExt;
pub use document_registry::DocumentRegistryExt;
pub use work_item::WorkItemExt;
