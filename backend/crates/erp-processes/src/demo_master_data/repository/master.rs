//! 主数据硬删除仅使用拥有领域公开的仓储。

use erp_catalog::CatalogExt;
use erp_customer::CustomerExt;
use erp_party::PartyExt;
use erp_supplier::SupplierExt;
use erp_warehouse::WarehouseExt;
use mongodb::Database;
use persistence_core::repository::IdRepository;

/// 绑定允许硬删除的主数据集合。
///
/// # 参数
/// db - 数据库；collection - 内部集合标识。
/// # 返回
/// 已绑定领域集合的 ID 仓储，未登记集合返回空值。
/// # 错误
/// 无。
pub(super) fn ids(db: &Database, collection: &str) -> Option<IdRepository> {
    Some(match collection {
        "products" => db.products().ids(),
        "product_revisions" => db.product_revisions().ids(),
        "product_revision_medias" => db.product_revision_medias().ids(),
        "skus" => db.skus().ids(),
        "sku_revisions" => db.sku_revisions().ids(),
        "sku_revision_attribute_values" => db.sku_revision_attribute_values().ids(),
        "voucher_category_profile_revisions" => db.voucher_category_profile_revisions().ids(),
        "product_brands" => db.product_brands().ids(),
        "product_categories" => db.product_categories().ids(),
        "product_category_attributes" => db.product_category_attributes().ids(),
        "unit_of_measures" => db.unit_of_measures().ids(),
        "parties" => db.parties().ids(),
        "party_revisions" => db.party_revisions().ids(),
        "party_contacts" => db.party_contacts().ids(),
        "party_addresses" => db.party_addresses().ids(),
        "party_bank_accounts" => db.party_bank_accounts().ids(),
        "party_tax_profiles" => db.party_tax_profiles().ids(),
        "customer_accounts" => db.customer_accounts().ids(),
        "customer_assignments" => db.customer_assignments().ids(),
        "customer_profile_commands" => db.customer_profile_commands().ids(),
        "supplier_accounts" => db.supplier_accounts().ids(),
        "supplier_capabilities" => db.supplier_capabilities().ids(),
        "supplier_capability_revisions" => db.supplier_capability_revisions().ids(),
        "supplier_commercial_profile_revisions" => db.supplier_commercial_profile_revisions().ids(),
        "supplier_qualifications" => db.supplier_qualifications().ids(),
        "supplier_qualification_revisions" => db.supplier_qualification_revisions().ids(),
        "supplier_qualification_capabilities" => db.supplier_qualification_capabilities().ids(),
        "supplier_rating_revisions" => db.supplier_rating_revisions().ids(),
        "supplier_profile_commands" => db.supplier_profile_commands().ids(),
        "warehouses" => db.warehouses().ids(),
        "warehouse_revisions" => db.warehouse_revisions().ids(),
        "warehouse_sku_policies" => db.warehouse_sku_policies().ids(),
        _ => return None,
    })
}
