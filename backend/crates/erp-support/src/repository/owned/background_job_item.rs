//! Owned MongoDB repository for [`crate::entity::bulk_job::BackgroundJobItem`].

crate::repository::owned::owned_repo!(
    BackgroundJobItemRepository,
    crate::entity::bulk_job::BackgroundJobItem
);
