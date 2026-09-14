//! Composition adapters that bind consumer ports to providing domains.

mod catalog;
mod contract;
mod customer;
mod customer_data_scope;
mod import;
mod inventory;
mod organization;
mod party;
mod supplier;
mod warehouse;
pub use organization::organization_service;

pub use catalog::{catalog_service, CatalogPendingAttachments, MongoCatalogAudit, MongoCatalogFileAssets};
pub use contract::contract_service;
pub use customer::{MongoCustomerAccountFacts, MongoCustomerAudit, MongoCustomerPartyFacts};
pub use customer_data_scope::{customer_access, MongoCustomerDataScope};
pub use import::{import_apply_service, legacy_import_service, MongoImportBulkJobs};
pub use inventory::{authorize_inventory, inventory_adjustment_service, inventory_service};
pub use party::{MongoPartyAudit, MongoSupplierRole};
pub use supplier::{MongoSupplierFileAssets, MongoSupplierPartyFacts, MongoSupplierSensitiveTokens};
pub use warehouse::warehouse_service;

use std::sync::Arc;

use erp_customer::{CustomerAssignmentService, CustomerService, FailClosedCustomerDataScopePort};
use erp_identity::SharedRbacService;
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

/// 构造只装载客户事实的服务；范围 Port 失败关闭。
///
/// # 参数
/// * `db` - 客户集合所在数据库
///
/// # 返回
/// 返回未接线范围解析的客户服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 不得用于列表、详情或写命令；解析范围必须调用 [`scoped_customer_service`]。
pub fn customer_service(db: Database) -> CustomerService {
    CustomerService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerPartyFacts::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db),
        FailClosedCustomerDataScopePort::shared(),
    )
}

/// 构造已接入身份域公共解析器的客户服务。
///
/// # 参数
/// * `db` - 客户与身份集合所在数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回可解析客户范围的服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// adapter 必须调用 DataScopeService；客户域不得直接依赖身份域。
pub fn scoped_customer_service(db: Database, rbac: SharedRbacService) -> CustomerService {
    CustomerService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerPartyFacts::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db.clone()),
        MongoCustomerDataScope::shared(db, rbac),
    )
}

/// 构造只装载归属事实的服务；范围 Port 失败关闭。
///
/// # 参数
/// * `db` - 客户集合所在数据库
///
/// # 返回
/// 返回未接线范围解析的归属服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 归属变更必须调用 [`scoped_customer_assignment_service`]。
pub fn customer_assignment_service(db: Database) -> CustomerAssignmentService {
    CustomerAssignmentService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db),
        FailClosedCustomerDataScopePort::shared(),
    )
}

/// 构造已接入身份域公共解析器的客户归属服务。
///
/// # 参数
/// * `db` - 客户与身份集合所在数据库
/// * `rbac` - 当前 RBAC 快照
///
/// # 返回
/// 返回可在事务内重验客户范围的归属服务。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 归属变更必须证明 customer:update，不得只依赖入口事前检查。
pub fn scoped_customer_assignment_service(
    db: Database,
    rbac: SharedRbacService,
) -> CustomerAssignmentService {
    CustomerAssignmentService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerAccountFacts::shared(db.clone()),
        MongoCustomerDataScope::shared(db, rbac),
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

pub mod catalog_supply_query;
pub mod supplier_api;
pub mod supplier_failure;
pub mod supplier_fulfillment_gateway;
pub mod workflow;

pub mod identity;
pub mod identity_audit;
pub mod support_audit;
pub mod support_documents;
