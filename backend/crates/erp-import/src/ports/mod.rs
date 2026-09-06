//! Consumer ports for bulk-job facts used by import queries.

mod bulk_job;

pub use bulk_job::{BulkJobFactsPort, FailClosedBulkJobFacts};
