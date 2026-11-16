//! 供应商资料、供给、预存款与门户材料的完整实现绑定。
use erp_identity::Result;
use erp_permissions::{
    internal, supplier, supplier_api, supplier_deposit, supplier_fulfillment, supplier_offering,
    supplier_settlement,
};
use erp_supplier::authorization::bindings as supplier_bindings;
use erp_supplier_catalog::supplier_catalog_adoption::preview_facts_port;
use erp_supplier_catalog::supplier_portal::material_facts_port;
use erp_supplier_orders::supplier_credentials::authorization::connection_facts_port;
use erp_supplier_orders::supplier_deposits::deposit_bindings;
use erp_supply::authorization::{facts as supply_facts, mapping as supply_mapping};
use erp_supply::repository::supplier_api::connection_mapping;
use mongodb::Database;

use super::{FactsBindings, MappingBindings};

/// 创建供应商相关资源的唯一事实读取器与固定查询映射。
/// # 参数
/// db 为共享业务数据库；读取器与映射仅在启动时构造。
/// # 返回
/// 按资源登记的事实与按权限登记的映射；同资源查询权限复用同一实例。
/// # 错误
/// 拥有领域拒绝资源描述时返回接入错误，不构造占位实现。
pub(super) fn bindings(db: Database) -> Result<(FactsBindings, MappingBindings)> {
    let (supplier_facts, supplier_mapping) = supplier_bindings(db.clone())?;
    let connection_mapping = connection_mapping(supplier_mapping.clone())?;
    let (deposit_facts, deposit_mapping) = deposit_bindings(db.clone());
    let mut facts = vec![
        ("supplier".into(), supplier_facts),
        ("supplier_deposit".into(), deposit_facts),
        ("supplier_api_connection".into(), connection_facts_port(db.clone())),
        ("supplier_catalog_adoption_preview".into(), preview_facts_port(db.clone())),
        ("supplier_portal_material".into(), material_facts_port(db.clone())),
    ];
    let mut mappings = vec![
        (supplier::LIST, supplier_mapping.clone()),
        (supplier::DETAIL, supplier_mapping),
        (supplier_deposit::LIST, deposit_mapping.clone()),
        (supplier_deposit::DETAIL, deposit_mapping),
    ];
    mappings.extend(
        [supplier_api::CONNECTION_DETAIL, supplier_api::JD_VOP_AUTHORIZE, internal::CONNECTION_CATALOG_READ]
            .into_iter()
            .map(|permission| (permission, connection_mapping.clone())),
    );
    for (resource, permissions) in [
        ("supplier_offering", vec![supplier_offering::LIST, supplier_offering::DETAIL]),
        (
            "supplier_fulfillment_order",
            vec![
                supplier_fulfillment::ORDER_LIST,
                supplier_fulfillment::ORDER_DETAIL,
                supplier_fulfillment::ORDER_SUBMIT,
                supplier_fulfillment::ORDER_CANCEL,
                supplier_fulfillment::ORDER_REFUND,
            ],
        ),
        (
            "supplier_settlement_statement",
            vec![supplier_settlement::STATEMENT_LIST, supplier_settlement::STATEMENT_DETAIL],
        ),
    ] {
        facts.push((resource.into(), supply_facts(db.clone(), resource)?));
        let mapping = supply_mapping(resource)?;
        mappings.extend(permissions.into_iter().map(|permission| (permission, mapping.clone())));
    }
    Ok((facts, mappings))
}
