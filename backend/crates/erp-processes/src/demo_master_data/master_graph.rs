//! 主数据聚合的专属子记录和阻止误删的外部引用。

use super::derived::{DeletionSet, Store};
use super::plan::DemoKind;
use super::record::DemoMasterRecord;
use crate::{Error, Result};

/// 按拓扑顺序列出专属子记录；共享字典和文件资产不作为子记录。
const CHILDREN: &[(&str, &str, &str)] = &[
    ("products", "skus", "product_id"),
    ("products", "product_revisions", "product_id"),
    ("product_revisions", "product_revision_medias", "product_revision_id"),
    ("skus", "sku_revisions", "sku_id"),
    ("skus", "voucher_category_profile_revisions", "sku_id"),
    ("sku_revisions", "sku_revision_attribute_values", "sku_revision_id"),
    ("skus", "warehouse_sku_policies", "sku_id"),
    ("warehouses", "warehouse_revisions", "warehouse_id"),
    ("warehouses", "warehouse_sku_policies", "warehouse_id"),
    ("product_categories", "product_category_attributes", "category_id"),
    ("customer_accounts", "customer_assignments", "customer_id"),
    ("customer_accounts", "customer_profile_commands", "customer_id"),
    ("supplier_accounts", "supplier_profile_commands", "supplier_id"),
    ("supplier_accounts", "supplier_commercial_profile_revisions", "supplier_id"),
    ("supplier_accounts", "supplier_capabilities", "supplier_id"),
    ("supplier_accounts", "supplier_capability_revisions", "supplier_id"),
    ("supplier_accounts", "supplier_qualifications", "supplier_id"),
    ("supplier_accounts", "supplier_qualification_revisions", "supplier_id"),
    ("supplier_accounts", "supplier_rating_revisions", "supplier_id"),
    ("supplier_qualifications", "supplier_qualification_capabilities", "qualification_id"),
    ("supplier_capabilities", "supplier_qualification_capabilities", "capability_id"),
    ("parties", "party_revisions", "party_id"),
    ("parties", "party_contacts", "party_id"),
    ("parties", "party_addresses", "party_id"),
    ("parties", "party_bank_accounts", "party_id"),
    ("parties", "party_tax_profiles", "party_id"),
];

/// 只接受已登记的主键，不按名称或编号推断归属。
///
/// # 参数
/// records - 已登记主数据。
/// # 返回
/// 按领域集合分组的实际 ID。
/// # 错误
/// 身份或种类无效时返回错误。
pub(super) fn roots(records: &[DemoMasterRecord]) -> Result<DeletionSet> {
    let mut roots = DeletionSet::new();
    for row in records {
        if row.entity_id.trim().is_empty() || row.related_ids.iter().any(|id| id.trim().is_empty()) {
            return Err(Error::Internal("演示登记包含无效 ID".into()));
        }
        let (collection, related) = match row.kind() {
            Some(DemoKind::Unit) => ("unit_of_measures", None),
            Some(DemoKind::Brand) => ("product_brands", None),
            Some(DemoKind::Category) => ("product_categories", None),
            Some(DemoKind::Warehouse) => ("warehouses", None),
            Some(DemoKind::Product) => ("products", Some("skus")),
            Some(DemoKind::Customer) => ("customer_accounts", Some("parties")),
            Some(DemoKind::Supplier) => ("supplier_accounts", Some("parties")),
            None => return Err(Error::Internal("演示登记包含未知种类".into())),
        };
        roots.entry(collection.into()).or_default().insert(row.entity_id.clone());
        if let Some(related) = related {
            roots.entry(related.into()).or_default().extend(row.related_ids.iter().cloned());
        }
    }
    Ok(roots)
}

/// 从指定聚合根收集全部历史子记录，所有命中最终按其实际 ID 删除。
///
/// # 参数
/// store - 事务数据访问；roots - 本批主数据根。
/// # 返回
/// 根及专属子记录的实际 ID。
/// # 错误
/// 关联读取失败时返回错误。
pub(super) async fn collect(store: &mut impl Store, roots: &DeletionSet) -> Result<DeletionSet> {
    let mut doomed = roots.clone();
    for &(source, collection, field) in CHILDREN {
        let values = values(&doomed, source);
        for row in store.linked(collection, field, &values, None).await? {
            doomed.entry(collection.into()).or_default().insert(row.id);
        }
    }
    let ids = doomed.values().flatten().cloned().collect::<Vec<_>>();
    for row in store.linked("document_attachments", "document_id", &ids, None).await? {
        doomed.entry("document_attachments".into()).or_default().insert(row.id);
    }
    Ok(doomed)
}

/// 禁止硬删被未登记主数据使用的主体、分类、品牌和单位。
///
/// # 参数
/// store - 事务数据访问；owned - 全部已登记主数据。
/// # 返回
/// 引用均属于登记范围时返回空结果。
/// # 错误
/// 存在未登记引用或读取失败时返回错误。
pub(super) async fn guard(store: &mut impl Store, owned: &DeletionSet) -> Result<()> {
    let references = [
        ("parties", "customer_accounts", "party_id", "customer_accounts", None),
        ("parties", "supplier_accounts", "party_id", "supplier_accounts", None),
        (
            "parties",
            "supplier_commercial_profile_revisions",
            "signing_entity_party_id",
            "supplier_accounts",
            Some("supplier_id"),
        ),
        (
            "parties",
            "supplier_commercial_profile_revisions",
            "payment_entity_party_id",
            "supplier_accounts",
            Some("supplier_id"),
        ),
        ("product_brands", "product_revisions", "brand_id", "products", Some("product_id")),
        ("product_categories", "product_revisions", "category_id", "products", Some("product_id")),
        ("product_categories", "product_categories", "parent_category_id", "product_categories", None),
        ("unit_of_measures", "skus", "base_unit_id", "skus", None),
    ];
    for row in store
        .linked("warehouse_sku_policies", "warehouse_id", &values(owned, "warehouses"), Some("sku_id"))
        .await?
    {
        if !values(owned, "skus").contains(&row.parent.unwrap_or_default()) {
            return Err(Error::BusinessLogicError("演示仓库包含非演示商品策略，请先解除引用再删除".into()));
        }
    }
    for (root, collection, field, owners, parent) in references {
        let allowed = values(owned, owners);
        for row in store.linked(collection, field, &values(owned, root), parent).await? {
            if !allowed.contains(row.parent.as_ref().unwrap_or(&row.id)) {
                return Err(Error::BusinessLogicError(
                    "演示主数据被未登记资料引用，请先解除引用再删除".into(),
                ));
            }
        }
    }
    Ok(())
}

/// 返回指定集合的精确 ID，集合不存在时不发起宽范围查询。
///
/// # 参数
/// set - ID 集合；collection - 领域集合。
/// # 返回
/// 集合中的 ID，缺失时返回空列表。
/// # 错误
/// 无。
pub(super) fn values(set: &DeletionSet, collection: &str) -> Vec<String> {
    set.get(collection).into_iter().flatten().cloned().collect()
}
