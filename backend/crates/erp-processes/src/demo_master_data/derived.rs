//! 按外键图删除引用演示主数据的单据、审批待办和演示商品库存。
//!
//! 账号、部门、已发布审批流程和主数据聚合不在这里删除。

use std::collections::{HashMap, HashSet};

use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{NoTransaction, mongo_ops};

use super::derived_graph::{self, Edge, IdPull, SeedKind};
use super::plan::DemoKind;
use super::record::DemoMasterRecord;
use crate::{Error, Result};

const ROUNDS: usize = 8;
const ID_CHUNK: usize = 200;

/// 会被业务单据引用的演示身份。商品和仓库主键不进入种子。
pub(super) struct MasterIds {
    /// 客户角色 ID。
    customer: Vec<String>,
    /// 主体 ID。
    party: Vec<String>,
    /// 供应商角色 ID。
    supplier: Vec<String>,
    /// SKU ID。
    sku: Vec<String>,
}

impl MasterIds {
    fn is_empty(&self) -> bool {
        self.customer.is_empty() && self.party.is_empty() && self.supplier.is_empty() && self.sku.is_empty()
    }

    fn of(&self, kind: SeedKind) -> &[String] {
        match kind {
            SeedKind::Customer => &self.customer,
            SeedKind::Party => &self.party,
            SeedKind::Supplier => &self.supplier,
            SeedKind::Sku => &self.sku,
        }
    }
}

/// 从清单收集客户、供应商、主体和 SKU。
///
/// # 参数
/// * `records` - 演示清单，包含已删除仍可恢复的记录
///
/// # 返回
/// 返回四类身份。品牌、分类、单位、商品和仓库主键不返回。
pub(super) fn master_ids(records: &[DemoMasterRecord]) -> MasterIds {
    let mut ids =
        MasterIds { customer: Vec::new(), party: Vec::new(), supplier: Vec::new(), sku: Vec::new() };
    for record in records {
        match record.kind() {
            Some(DemoKind::Customer) => {
                push_id(&mut ids.customer, &record.entity_id);
                extend_ids(&mut ids.party, &record.related_ids);
            },
            Some(DemoKind::Supplier) => {
                push_id(&mut ids.supplier, &record.entity_id);
                extend_ids(&mut ids.party, &record.related_ids);
            },
            Some(DemoKind::Product) => extend_ids(&mut ids.sku, &record.related_ids),
            _ => {},
        }
    }
    ids
}

/// 删除引用演示身份的衍生记录。
///
/// # 参数
/// * `db` - 目标数据库
/// * `seed` - 演示客户、供应商、主体和 SKU
///
/// # 返回
/// 返回删除的文档数。
///
/// # 错误
/// 查询或删除失败，或图指向受保护集合时返回错误。
pub(super) async fn purge_derived(db: &Database, seed: &MasterIds) -> Result<u64> {
    if seed.is_empty() {
        return Ok(0);
    }
    ensure_graph_allowed()?;
    let mut doomed: HashMap<String, HashSet<String>> = HashMap::new();
    let mut parents = HashSet::new();
    for edge in derived_graph::edges() {
        let Some(kind) = edge.seed else {
            continue;
        };
        absorb(db, edge, seed.of(kind), &mut doomed, &mut parents).await?;
    }
    for _ in 0..ROUNDS {
        if !expand_once(db, &mut doomed, &mut parents).await? {
            break;
        }
    }
    delete_doomed(db, &doomed).await
}

fn ensure_graph_allowed() -> Result<()> {
    for edge in derived_graph::edges() {
        if !derived_graph::deletion_allowed(edge.collection) {
            return Err(Error::Internal(format!("衍生清理不能删除 {}", edge.collection)));
        }
    }
    for pull in derived_graph::id_pulls() {
        if !derived_graph::deletion_allowed(pull.collection) {
            return Err(Error::Internal(format!("衍生清理不能删除 {}", pull.collection)));
        }
    }
    Ok(())
}

async fn expand_once(
    db: &Database,
    doomed: &mut HashMap<String, HashSet<String>>,
    parents: &mut HashSet<String>,
) -> Result<bool> {
    let mut grew = false;
    let parent_ids = parents.iter().cloned().collect::<Vec<_>>();
    for edge in derived_graph::edges().iter().filter(|edge| edge.seed.is_none()) {
        if absorb(db, edge, &parent_ids, doomed, parents).await? {
            grew = true;
        }
    }
    for pull in derived_graph::id_pulls() {
        if absorb_pull(db, pull, &parent_ids, doomed, parents).await? {
            grew = true;
        }
    }
    Ok(grew)
}

async fn absorb(
    db: &Database,
    edge: &Edge,
    values: &[String],
    doomed: &mut HashMap<String, HashSet<String>>,
    parents: &mut HashSet<String>,
) -> Result<bool> {
    let hits = find_hits(db, edge.collection, edge.field, values, edge.lift).await?;
    Ok(note_hits(edge.collection, &hits, doomed, parents))
}

async fn absorb_pull(
    db: &Database,
    pull: &IdPull,
    values: &[String],
    doomed: &mut HashMap<String, HashSet<String>>,
    parents: &mut HashSet<String>,
) -> Result<bool> {
    let hits = find_hits(db, pull.collection, "id", values, pull.lift).await?;
    Ok(note_hits(pull.collection, &hits, doomed, parents))
}

fn note_hits(
    collection: &str,
    hits: &[Hit],
    doomed: &mut HashMap<String, HashSet<String>>,
    parents: &mut HashSet<String>,
) -> bool {
    let mut grew = false;
    for hit in hits {
        if doomed.entry(collection.to_string()).or_default().insert(hit.id.clone()) {
            grew = true;
        }
        grew |= parents.insert(hit.id.clone());
        for lifted in &hit.lifted {
            grew |= parents.insert(lifted.clone());
        }
    }
    grew
}

struct Hit {
    id: String,
    lifted: Vec<String>,
}

async fn find_hits(
    db: &Database,
    collection: &str,
    field: &str,
    values: &[String],
    lift: Option<&str>,
) -> Result<Vec<Hit>> {
    let mut hits = Vec::new();
    for chunk in values.chunks(ID_CHUNK) {
        if chunk.is_empty() {
            continue;
        }
        let found = mongo_ops::find_many(
            &db.collection::<Document>(collection),
            doc! { field: { "$in": chunk } },
            projection(lift),
            &mut NoTransaction,
        )
        .await?;
        hits.extend(found.iter().filter_map(|document| hit_from(document, lift)));
    }
    Ok(hits)
}

fn projection(lift: Option<&str>) -> FindOptions {
    let mut fields = doc! { "id": 1, "_id": 0 };
    if let Some(field) = lift {
        fields.insert(field, 1);
    }
    FindOptions::builder().projection(fields).build()
}

fn hit_from(document: &Document, lift: Option<&str>) -> Option<Hit> {
    let id = document.get_str("id").ok().filter(|id| !id.is_empty())?.to_string();
    let lifted = lift.and_then(|field| document.get_str(field).ok()).filter(|id| !id.is_empty());
    Some(Hit { id, lifted: lifted.into_iter().map(str::to_string).collect() })
}

async fn delete_doomed(db: &Database, doomed: &HashMap<String, HashSet<String>>) -> Result<u64> {
    let mut deleted = 0;
    for (collection, ids) in doomed {
        if !derived_graph::deletion_allowed(collection) {
            return Err(Error::Internal(format!("衍生清理不能删除 {collection}")));
        }
        deleted += delete_ids(db, collection, ids).await?;
    }
    Ok(deleted)
}

async fn delete_ids(db: &Database, collection: &str, ids: &HashSet<String>) -> Result<u64> {
    let ids = ids.iter().cloned().collect::<Vec<_>>();
    let mut deleted = 0;
    for chunk in ids.chunks(ID_CHUNK) {
        let result = db
            .collection::<Document>(collection)
            .delete_many(doc! { "id": { "$in": chunk } })
            .await
            .map_err(persistence_core::Error::from)?;
        deleted += result.deleted_count;
    }
    Ok(deleted)
}

fn push_id(ids: &mut Vec<String>, value: &str) {
    if !value.is_empty() && !ids.iter().any(|id| id == value) {
        ids.push(value.to_string());
    }
}

fn extend_ids(ids: &mut Vec<String>, values: &[String]) {
    for value in values {
        push_id(ids, value);
    }
}

#[cfg(test)]
mod tests {
    use super::master_ids;
    use crate::demo_master_data::plan::DemoKind;
    use crate::demo_master_data::record::DemoMasterRecord;

    #[test]
    fn seed_keeps_traded_ids_and_skips_dictionary_and_product_roots() {
        let records = vec![
            record(DemoKind::Customer, "customer-1", vec!["party-1"]),
            record(DemoKind::Product, "product-1", vec!["sku-1"]),
            record(DemoKind::Brand, "brand-1", vec![]),
            record(DemoKind::Warehouse, "warehouse-1", vec![]),
        ];
        let seed = master_ids(&records);
        assert_eq!(seed.customer, vec!["customer-1".to_string()]);
        assert_eq!(seed.party, vec!["party-1".to_string()]);
        assert_eq!(seed.sku, vec!["sku-1".to_string()]);
        assert!(!seed.customer.iter().any(|id| id == "product-1" || id == "brand-1" || id == "warehouse-1"));
        assert!(seed.supplier.is_empty());
    }

    fn record(kind: DemoKind, entity_id: &str, related: Vec<&str>) -> DemoMasterRecord {
        DemoMasterRecord {
            key: format!("key-{entity_id}"),
            kind: kind.as_str().to_string(),
            entity_id: entity_id.to_string(),
            related_ids: related.into_iter().map(str::to_string).collect(),
            label: entity_id.to_string(),
            removed: false,
        }
    }
}
