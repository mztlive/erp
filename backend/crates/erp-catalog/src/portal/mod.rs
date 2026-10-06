//! Supplier new-product proposals; formal writes join the caller's transaction.

mod category_mapping;
mod category_mapping_service;
mod dictionaries;
mod materialize;
mod model;
mod query;
mod repository;
mod service;

pub use category_mapping::{
    CategoryHierarchyNode, CategoryMappingConfirmation, CategoryMappingSuggestion,
    CategoryMappingSuggestionStatus, SupplierCategoryMapping,
};
pub use materialize::{CatalogMaterializeCommand, ExistingProductRef};
pub use model::*;
pub use query::{DictionaryCandidate, DictionaryKind, DuplicateCandidate, DuplicateSkuCandidate};
pub use repository::{CatalogDraftFilter, CatalogPortalExt};
pub use service::{CatalogDraftSubmitCommand, CatalogPortalService, PortalSupplyTermsPort};
