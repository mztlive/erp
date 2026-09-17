//! Owned MongoDB repository for [`crate::entity::customer::CustomerAccount`].

use std::ops::Deref;

use super::owned_repository::owned_repository;

owned_repository!(CustomerAccountRepository, crate::entity::customer::CustomerAccount);
