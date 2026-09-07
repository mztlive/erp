//! Composition adapters that bind consumer ports to providing domains.

mod catalog;
mod contract;
mod customer;
mod import;
mod inventory;
mod party;
mod supplier;
mod warehouse;

pub use catalog::{catalog_service, CatalogPendingAttachments, MongoCatalogAudit, MongoCatalogFileAssets};
pub use contract::contract_service;
pub use customer::{MongoCustomerAccountFacts, MongoCustomerAudit, MongoCustomerPartyFacts};
pub use import::{import_apply_service, legacy_import_service, MongoImportBulkJobs};
pub use inventory::{authorize_inventory, inventory_adjustment_service, inventory_service};
pub use party::{MongoPartyAudit, MongoSupplierRole};
pub use supplier::{MongoSupplierFileAssets, MongoSupplierPartyFacts, MongoSupplierSensitiveTokens};
pub use warehouse::warehouse_service;

use std::sync::Arc;

use erp_customer::{CustomerAssignmentService, CustomerService};
use erp_party::{
    PartyAddressService, PartyBankAccountService, PartyContactService, PartyService, PartyTaxProfileService,
    SensitiveDataCodec,
};
use erp_supplier::SupplierService;
use mongodb::Database;

/// Construct a party service with audit and supplier-role adapters.
pub fn party_service(db: Database) -> PartyService {
    PartyService::new(
        db.clone(),
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
    )
}

/// Construct a party contact service with composition adapters.
pub fn party_contact_service(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> PartyContactService {
    PartyContactService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
    )
}

/// Construct a party address service with composition adapters.
pub fn party_address_service(db: Database, sensitive_data: Arc<SensitiveDataCodec>) -> PartyAddressService {
    PartyAddressService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
    )
}

/// Construct a party bank-account service with composition adapters.
pub fn party_bank_account_service(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
) -> PartyBankAccountService {
    PartyBankAccountService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
    )
}

/// Construct a party tax-profile service with composition adapters.
pub fn party_tax_profile_service(db: Database) -> PartyTaxProfileService {
    PartyTaxProfileService::new(
        db.clone(),
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
    )
}

/// Construct a customer service with audit, party and account adapters.
pub fn customer_service(db: Database) -> CustomerService {
    CustomerService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerPartyFacts::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db),
    )
}

/// Construct a customer assignment service with audit and account adapters.
pub fn customer_assignment_service(db: Database) -> CustomerAssignmentService {
    CustomerAssignmentService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db),
    )
}

/// Construct a supplier list/detail service with party facts.
pub fn supplier_service(db: Database) -> SupplierService {
    SupplierService::new(db.clone(), MongoSupplierPartyFacts::shared(db))
}

/// Construct a supplier list/detail service that can issue reveal tokens.
pub fn supplier_service_with_sensitive(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
) -> SupplierService {
    SupplierService::with_sensitive_data(
        db.clone(),
        MongoSupplierPartyFacts::shared(db),
        MongoSupplierSensitiveTokens::shared(sensitive_data),
    )
}
