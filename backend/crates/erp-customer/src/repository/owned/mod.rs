//! Owned customer repositories composed from persistence-core.

mod customer_account;
mod customer_assignment;
mod customer_profile_command;

pub use customer_account::CustomerAccountRepository;
pub use customer_assignment::CustomerAssignmentRepository;
pub use customer_profile_command::CustomerProfileCommandRepository;
