//! 把消费方端口接到提供方领域的组合 adapter。

pub mod authorization;
pub mod errors;

#[cfg(test)]
mod dangaoshushu_composition_tests;

mod audit_conversion;
mod catalog;
mod catalog_data_scope;
mod contract;
mod contract_template;
mod customer;
mod customer_data_scope;
mod fulfillment_order_data_scope;
mod funds_data_scope;
mod identity_error;
mod import;
mod integration_data_scope;
mod inventory;
mod offering_data_scope;
mod organization;
mod party;
mod scope_support;
mod selection_data_scope;
mod settlement_data_scope;
mod supplier;
mod supplier_data_scope;
mod warehouse;
use std::sync::Arc;

pub(crate) use authorization::contract_access_with_db as contract_access;
pub use catalog::{MongoCatalogAudit, MongoCatalogFileAssets};
pub use contract::scoped_contract_service;
pub use contract_template::contract_template_service;
pub use customer::{MongoCustomerAudit, MongoCustomerPartyFacts};
pub use customer_data_scope::customer_access;
use erp_customer::{CustomerAssignmentService, CustomerService};
use erp_identity::AuthorizationService;
use erp_party::{
    PartyAddressService, PartyBankAccountService, PartyContactService, PartyService, PartyTaxProfileService,
    SensitiveDataCodec,
};
pub use funds_data_scope::{MongoFundsDataScope, funds_access};
pub use import::{MongoImportBulkJobs, legacy_import_service};
pub use integration_data_scope::scoped_integration_ops_service;
pub use inventory::{authorize_inventory, inventory_adjustment_process, inventory_service};
use mongodb::Database;
pub use organization::organization_service;
pub use party::{MongoPartyAudit, MongoSupplierRole};
pub use purchase::purchase_access;
pub use selection_data_scope::selection_access;
pub use supplier::{MongoSupplierFileAssets, MongoSupplierPartyFacts, MongoSupplierSensitiveTokens};
pub use supplier_data_scope::{
    scoped_supplier_service, scoped_supplier_service_with_sensitive, supplier_access,
};
pub use warehouse::{MongoWarehouseIdentity, warehouse_service};

/// 装配带审计与供应商角色 adapter 的主体服务。
/// 返回已注入审计与供应商角色 Port 的主体服务。
pub fn party_service(db: Database, authorization: Arc<AuthorizationService>) -> PartyService {
    PartyService::new(
        db.clone(),
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
        authorization,
    )
}

/// 装配带审计、供应商角色与敏感数据编解码的主体联系人服务。
/// `sensitive_data` 为联系人敏感字段编解码器。
/// 返回主体联系人服务。
pub fn party_contact_service(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    authorization: Arc<AuthorizationService>,
) -> PartyContactService {
    PartyContactService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
        authorization,
    )
}

/// 装配带审计、供应商角色与敏感数据编解码的主体地址服务。
/// `sensitive_data` 为地址敏感字段编解码器。
/// 返回主体地址服务。
pub fn party_address_service(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    authorization: Arc<AuthorizationService>,
) -> PartyAddressService {
    PartyAddressService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
        authorization,
    )
}

/// 装配带审计、供应商角色与敏感数据编解码的主体银行账户服务。
/// `sensitive_data` 为银行账户敏感字段编解码器。
/// 返回主体银行账户服务。
pub fn party_bank_account_service(
    db: Database,
    sensitive_data: Arc<SensitiveDataCodec>,
    authorization: Arc<AuthorizationService>,
) -> PartyBankAccountService {
    PartyBankAccountService::new(
        db.clone(),
        sensitive_data,
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
        authorization,
    )
}

/// 装配带审计与供应商角色 adapter 的主体税务档案服务。
/// 返回主体税务档案服务。
pub fn party_tax_profile_service(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> PartyTaxProfileService {
    PartyTaxProfileService::new(
        db.clone(),
        MongoPartyAudit::shared(db.clone()),
        MongoSupplierRole::shared(db),
        authorization,
    )
}

/// 构造已接入身份域公共解析器的客户服务。
/// 返回可解析客户范围的服务。
///
/// 本域访问器直接调用 identity 统一授权服务，不解释范围条款。
pub fn scoped_customer_service(db: Database, authorization: Arc<AuthorizationService>) -> CustomerService {
    CustomerService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        MongoCustomerPartyFacts::shared(db.clone()),
        customer_access(db, authorization),
    )
}

/// 构造已接入身份域公共解析器的客户归属服务。
/// 返回可在事务内重验客户范围的归属服务。
///
/// 归属变更必须证明 customer:update，不得只依赖入口事前检查。
pub fn scoped_customer_assignment_service(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> CustomerAssignmentService {
    CustomerAssignmentService::new(
        db.clone(),
        MongoCustomerAudit::shared(db.clone()),
        customer_access(db, authorization),
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

pub use catalog_data_scope::{catalog_access, scoped_catalog_service};
pub use fulfillment_order_data_scope::{
    MongoFulfillmentExceptionHandlers, fulfillment_order_access, scoped_fulfillment_service,
    supplier_fulfillment_identity,
};
pub use offering_data_scope::{offering_access, scoped_offering_process, scoped_offering_service};

#[cfg(test)]
mod supplier_composition_tests;

mod scope_target_validation;
pub use scope_target_validation::scope_configuration;
pub use settlement_data_scope::{scoped_settlement_process, scoped_settlement_service, settlement_access};

mod warehouse_directory;
pub use warehouse_directory::warehouse_directory;

mod sales_timeline;
pub use sales_timeline::sales_timeline_service;

mod supplier_catalog;
pub use supplier_catalog::catalog_services;

mod approval_runtime;
pub use approval_runtime::approval_runtime;

pub mod sales;

mod purchase;
pub use purchase::{purchase_formalization_process, purchase_ports};

mod finance_posting;
pub use finance_posting::{finance_posting_ports, receivable_posting_ports};

mod reverse_flow;
pub use reverse_flow::{local_reverse_flow_ports, reverse_flow_ports};

mod finance_responsibility;
pub use finance_responsibility::{finance_responsibility, finance_tasks};

pub mod application;
pub(crate) use application::JdVopCredentialMaintenance;
pub use application::{Application, ExternalConnectorPorts, ExternalConnectorReadiness};
pub mod approval_dispatch;
pub(crate) mod data_transfer;

pub mod notification;

pub(crate) mod notification_sources;
