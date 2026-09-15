//! Remaining-call-site helper: supplier account ids to current legal names.

use std::collections::HashMap;

use erp_core::ids::{PartyId, SupplierAccountId};
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::{Executor, Result};

/// Load current legal names for supplier accounts by composing supplier refs and party names.
///
/// # Parameters
/// * `db` - MongoDB database
/// * `supplier_ids` - supplier account ids
/// * `executor` - caller-chosen executor
///
/// # Returns
/// Map of supplier account id to current legal name. Missing parties or revisions are omitted.
pub async fn current_legal_names_by_account_ids(
    db: &Database,
    supplier_ids: &[SupplierAccountId],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, String>> {
    let refs = db.supplier().supplier_party_ids_by_account_ids(supplier_ids, executor).await?;
    let party_ids: Vec<PartyId> = refs.values().cloned().collect();
    let party_names = db.party().current_legal_names_by_party_ids(&party_ids, executor).await?;
    Ok(refs
        .into_iter()
        .filter_map(|(supplier_id, party_id)| {
            party_names.get(&party_id.to_string()).cloned().map(|legal_name| (supplier_id, legal_name))
        })
        .collect())
}
