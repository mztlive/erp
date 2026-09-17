//! Owned MongoDB repository for [`crate::entity::customer::CustomerProfileCommand`].

use std::ops::Deref;

use super::owned_repository::owned_repository;

owned_repository!(CustomerProfileCommandRepository, crate::entity::customer::CustomerProfileCommand);
