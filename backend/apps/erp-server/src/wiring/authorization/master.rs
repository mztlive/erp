//! 客户、合同、商品及仓库真实实现的固定装配。
mod contract;
use std::sync::Arc;

use erp_catalog::authorization::bindings as product_bindings;
use erp_contract::authorization::ContractAccess;
use erp_contract::repository::authorization::creation_facts;
use erp_customer::authorization::{CustomerFacts, CustomerMapping};
use erp_identity::{AccessFactsPort, AccessMapping, AuthorizationService, Result};
use erp_permissions::{catalog, contract as contract_permissions, customer, internal, warehouse};
use erp_warehouse::authorization::{WarehouseFacts, WarehouseMapping};
use mongodb::Database;

use super::{FactsBindings, MappingBindings};

pub(super) struct MasterBindings {
    pub facts: FactsBindings,
    pub mappings: MappingBindings,
    pub customer_facts: Arc<dyn AccessFactsPort>,
    pub customer_mapping: Arc<dyn AccessMapping>,
    pub warehouse_facts: Arc<dyn AccessFactsPort>,
    pub warehouse_mapping: Arc<dyn AccessMapping>,
}

// 构造合同访问器，绑定共享统一授权服务及客户事实端口。
// 使用真实客户事实的合同访问器。
pub(crate) fn contract_access_with_db(
    db: Database,
    authorization: Arc<AuthorizationService>,
) -> ContractAccess {
    let customer_facts = CustomerFacts::shared(db.clone());
    contract_access(db, authorization, customer_facts)
}

// 构造拥有领域的真实实现并连接对应查询入口。
// 完整根绑定与供跨域来源复用的同一事实、映射实例。
// 固定模型不满足 identity 合同时返回错误。
pub(super) fn bindings(db: Database) -> Result<MasterBindings> {
    let customer_facts = CustomerFacts::shared(db.clone());
    let customer_mapping = CustomerMapping::shared(db.clone());
    let warehouse_facts = WarehouseFacts::shared(db.clone());
    let warehouse_mapping = WarehouseMapping::shared("warehouse")?;
    let inventory_warehouse_mapping = WarehouseMapping::shared("inventory_warehouse")?;
    let (product_facts, product_mapping) = product_bindings(db.clone())?;
    let (contract_facts, contract_mapping) = contract::bindings(db)?;
    let facts = vec![
        ("customer".into(), customer_facts.clone()),
        ("warehouse".into(), warehouse_facts.clone()),
        ("inventory_warehouse".into(), warehouse_facts.clone()),
        ("product".into(), product_facts),
        ("contract".into(), contract_facts),
    ];
    let mut mappings = vec![
        (customer::LIST, customer_mapping.clone()),
        (customer::DETAIL, customer_mapping.clone()),
        (catalog::PRODUCT_PRODUCT_LIST, product_mapping.clone()),
        (catalog::PRODUCT_PRODUCT_DETAIL, product_mapping),
        (contract_permissions::LIST, contract_mapping.clone()),
        (contract_permissions::DETAIL, contract_mapping),
        (warehouse::LIST, warehouse_mapping.clone()),
    ];
    mappings.extend(
        [
            internal::INVENTORY_BALANCE_WAREHOUSES,
            internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES,
            internal::INVENTORY_MOVEMENT_WAREHOUSES,
            internal::INVENTORY_RESERVATION_WAREHOUSES,
            internal::INVENTORY_ADJUSTMENT_WAREHOUSES,
            internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES,
            internal::INVENTORY_CREATE_WAREHOUSES,
            internal::INVENTORY_UPDATE_WAREHOUSES,
        ]
        .into_iter()
        .map(|permission| (permission, inventory_warehouse_mapping.clone())),
    );
    Ok(MasterBindings {
        facts,
        mappings,
        customer_facts,
        customer_mapping,
        warehouse_facts,
        warehouse_mapping,
    })
}

// 将共享服务及同一客户事实实例注入合同访问器。
// 不自行构造授权政策的合同访问器。
pub(crate) fn contract_access(
    db: Database,
    authorization: Arc<AuthorizationService>,
    customer_facts: Arc<dyn AccessFactsPort>,
) -> ContractAccess {
    ContractAccess::new(db, authorization, creation_facts(customer_facts))
}
