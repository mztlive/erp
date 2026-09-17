//! Owned MongoDB repository for [`crate::entity::customer::CustomerAssignment`].

use std::ops::Deref;

use super::owned_repository::owned_repository;

owned_repository!(CustomerAssignmentRepository, crate::entity::customer::CustomerAssignment);
