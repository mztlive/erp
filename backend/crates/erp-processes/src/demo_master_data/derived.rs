//! 在同一事务中收集关联闭包、检查混用、按 ID 清理并验证残留。

use std::collections::{BTreeMap, BTreeSet};

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use mongodb::Database;
use persistence_core::Executor;
use persistence_core::repository::LinkedId;

use super::derived_graph::{self, SeedKind};
use super::plan::DemoKind;
use super::record::DemoMasterRecord;
use super::repository;
use crate::audit::run_audited;
use crate::{Error, Result};

type IdSet = BTreeSet<String>;
type DeletionSet = BTreeMap<String, IdSet>;
const MAX_ROUNDS: usize = 128;

/// 由清单实际 ID 派生的清理根。
#[derive(Clone, Default)]
pub(super) struct MasterIds {
    customer: Vec<String>,
    party: Vec<String>,
    supplier: Vec<String>,
    sku: Vec<String>,
    warehouse: Vec<String>,
}

impl MasterIds {
    /// 取对应外键种类的根 ID。
    fn of(&self, kind: SeedKind) -> &[String] {
        match kind {
            SeedKind::Customer => &self.customer,
            SeedKind::Party => &self.party,
            SeedKind::Supplier => &self.supplier,
            SeedKind::Sku => &self.sku,
            SeedKind::Warehouse => &self.warehouse,
        }
    }
}

/// 从包括已软删除记录在内的清单提取真实 ID。
///
/// # 参数
/// `records` - 登记清单，包含已软删除记录。
///
/// # 返回
/// 返回清理所需的真实身份集合。
///
/// # 错误
/// 无。
pub(super) fn master_ids(records: &[DemoMasterRecord]) -> MasterIds {
    let mut ids = MasterIds::default();
    for record in records {
        match record.kind() {
            Some(DemoKind::Customer) => {
                ids.customer.push(record.entity_id.clone());
                ids.party.extend(record.related_ids.iter().cloned());
            },
            Some(DemoKind::Supplier) => {
                ids.supplier.push(record.entity_id.clone());
                ids.party.extend(record.related_ids.iter().cloned());
            },
            Some(DemoKind::Warehouse) => ids.warehouse.push(record.entity_id.clone()),
            Some(DemoKind::Product) => ids.sku.extend(record.related_ids.iter().cloned()),
            _ => {},
        }
    }
    ids
}

/// 清理衍生数据；所有发现、校验、删除与成功审计共享一个事务。
///
/// # 参数
/// `db` - 数据库；`seed` - 清理根；`actor` - 审计操作人。
///
/// # 返回
/// 返回事务内物理删除的记录数量。
///
/// # 错误
/// 混用、关联异常、残留或事务失败时返回错误。
pub(super) async fn purge_derived(db: &Database, seed: MasterIds, actor: &AuditActor) -> Result<u64> {
    let audit =
        actor.clone().resource_log("demo_master_data.purge", "demo_master_data", "master-data".into())?;
    run_audited(db, audit, move |db, executor| {
        Box::pin(async move { purge(&mut MongoStore { db, executor }, &seed).await })
    })
    .await
}

/// 清理编排只依赖精确关联读取和 ID 删除。
#[async_trait]
trait Store: Send {
    async fn linked(
        &mut self,
        collection: &str,
        field: &str,
        values: &[String],
        parent: Option<&str>,
    ) -> Result<Vec<LinkedId>>;
    async fn delete(&mut self, collection: &str, ids: &[String]) -> Result<u64>;
}

struct MongoStore<'a> {
    db: &'a Database,
    executor: &'a mut dyn Executor,
}

#[async_trait]
impl Store for MongoStore<'_> {
    /// 通过领域仓储读取身份和混用校验字段。
    async fn linked(
        &mut self,
        collection: &str,
        field: &str,
        values: &[String],
        parent: Option<&str>,
    ) -> Result<Vec<LinkedId>> {
        Ok(repository::ids(self.db, collection)?
            .linked(field, values, parent, Some("sku_id"), self.executor)
            .await?)
    }

    /// 通过领域仓储按业务 ID 删除。
    async fn delete(&mut self, collection: &str, ids: &[String]) -> Result<u64> {
        Ok(repository::ids(self.db, collection)?.purge(ids, self.executor).await?)
    }
}

/// 先完整收集和校验，任何失败均阻止后续写入。
async fn purge(store: &mut impl Store, seed: &MasterIds) -> Result<u64> {
    let doomed = collect(store, seed).await?;
    let mut deleted = 0;
    for (collection, ids) in &doomed {
        deleted += store.delete(collection, &ids.iter().cloned().collect::<Vec<_>>()).await?;
    }
    for (collection, ids) in &doomed {
        if !store.linked(collection, "id", &ids.iter().cloned().collect::<Vec<_>>(), None).await?.is_empty() {
            return Err(Error::BusinessLogicError("演示关联数据仍有残留，删除未完成".into()));
        }
    }
    if !collect(store, seed).await?.is_empty() {
        return Err(Error::BusinessLogicError("演示关联数据仍有残留，删除未完成".into()));
    }
    Ok(deleted)
}

/// 按登记外键扩展到不再增长；达到保护上限时失败，禁止部分清理。
async fn collect(store: &mut impl Store, seed: &MasterIds) -> Result<DeletionSet> {
    let mut doomed = DeletionSet::new();
    let mut parents = IdSet::new();
    for edge in derived_graph::edges() {
        if let Some(kind) = edge.seed {
            let hits = store.linked(edge.collection, edge.field, seed.of(kind), edge.lift).await?;
            absorb(edge.collection, hits, seed, &mut doomed, &mut parents)?;
        }
    }
    for _ in 0..MAX_ROUNDS {
        if !expand(store, seed, &mut doomed, &mut parents).await? {
            return Ok(doomed);
        }
    }
    Err(Error::BusinessLogicError("演示关联数据超过清理层级上限，未执行删除".into()))
}

/// 每轮同时追踪父单据及其全部子记录。
async fn expand(
    store: &mut impl Store,
    seed: &MasterIds,
    doomed: &mut DeletionSet,
    parents: &mut IdSet,
) -> Result<bool> {
    let values = parents.iter().cloned().collect::<Vec<_>>();
    let mut grew = false;
    for edge in derived_graph::edges().iter().filter(|edge| edge.seed.is_none()) {
        let hits = store.linked(edge.collection, edge.field, &values, edge.lift).await?;
        grew |= absorb(edge.collection, hits, seed, doomed, parents)?;
    }
    for pull in derived_graph::id_pulls() {
        let hits = store.linked(pull.collection, "id", &values, pull.lift).await?;
        grew |= absorb(pull.collection, hits, seed, doomed, parents)?;
    }
    Ok(grew)
}

/// 拒绝受保护集合以及包含非演示 SKU 的关联单据。
fn absorb(
    collection: &str,
    hits: Vec<LinkedId>,
    seed: &MasterIds,
    doomed: &mut DeletionSet,
    parents: &mut IdSet,
) -> Result<bool> {
    if !derived_graph::deletion_allowed(collection) {
        return Err(Error::Internal(format!("衍生清理不能删除 {collection}")));
    }
    let mut grew = false;
    for hit in hits {
        if (requires_sku(collection) && hit.tag.is_none())
            || collection == "sales_order_voucher_line_revisions"
            || hit.tag.as_ref().is_some_and(|sku| !seed.sku.contains(sku))
        {
            return Err(Error::BusinessLogicError("关联单据包含非演示商品，请先处理混用单据再删除".into()));
        }
        grew |= doomed.entry(collection.to_string()).or_default().insert(hit.id.clone());
        grew |= parents.insert(hit.id);
        if let Some(parent) = hit.parent {
            grew |= parents.insert(parent);
        }
    }
    Ok(grew)
}

/// 当前 JSON 只生成实物 SKU；明细缺少 SKU 时不能证明属于演示数据。
fn requires_sku(collection: &str) -> bool {
    matches!(
        collection,
        "sales_order_goods_service_line_revisions"
            | "sales_order_working_copy_lines"
            | "sales_order_submission_lines"
            | "purchase_order_revision_lines"
            | "purchase_order_submission_lines"
            | "stock_adjustment_lines"
            | "stock_balances"
            | "stock_movements"
            | "stock_reservations"
            | "supplier_offerings"
            | "sales_selection_proposal_sku_lines"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        rows: BTreeMap<String, Vec<BTreeMap<String, String>>>,
        deleted: Vec<(String, Vec<String>)>,
        retain_rows: bool,
        fail_delete: bool,
    }

    impl MemoryStore {
        fn add(&mut self, collection: &str, id: &str, fields: &[(&str, &str)]) {
            let mut row = BTreeMap::from([("id".into(), id.into())]);
            row.extend(fields.iter().map(|(key, value)| (key.to_string(), value.to_string())));
            self.rows.entry(collection.into()).or_default().push(row);
        }
    }

    #[async_trait]
    impl Store for MemoryStore {
        async fn linked(
            &mut self,
            collection: &str,
            field: &str,
            values: &[String],
            parent: Option<&str>,
        ) -> Result<Vec<LinkedId>> {
            Ok(self
                .rows
                .get(collection)
                .into_iter()
                .flatten()
                .filter(|row| row.get(field).is_some_and(|value| values.contains(value)))
                .map(|row| LinkedId {
                    id: row["id"].clone(),
                    parent: parent.and_then(|field| row.get(field).cloned()),
                    tag: row.get("sku_id").cloned(),
                })
                .collect())
        }

        async fn delete(&mut self, collection: &str, ids: &[String]) -> Result<u64> {
            if self.fail_delete {
                return Err(Error::Internal("injected delete failure".into()));
            }
            self.deleted.push((collection.into(), ids.to_vec()));
            let rows = self.rows.entry(collection.into()).or_default();
            let before = rows.len();
            if !self.retain_rows {
                rows.retain(|row| !ids.contains(&row["id"]));
            }
            Ok(u64::try_from(before - rows.len()).unwrap())
        }
    }

    fn seed() -> MasterIds {
        MasterIds {
            sku: vec!["demo-sku".into()],
            customer: vec!["demo-customer".into()],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn deletes_adjustment_parent_children_workflow_and_keeps_unrelated_rows() {
        let mut db = MemoryStore::default();
        db.add(
            "stock_adjustment_lines",
            "line",
            &[("sku_id", "demo-sku"), ("stock_adjustment_id", "adjustment")],
        );
        db.add("stock_adjustments", "adjustment", &[]);
        db.add("work_items", "task", &[("business_object_id", "adjustment")]);
        db.add("approval_process_instances", "approval", &[("subject.subject_id", "adjustment")]);
        db.add("approval_node_executions", "node", &[("process_instance_id", "approval")]);
        db.add("stock_adjustments", "unrelated", &[]);
        db.add("accounts", "admin", &[]);
        assert_eq!(purge(&mut db, &seed()).await.unwrap(), 5);
        assert_eq!(db.rows["stock_adjustments"][0]["id"], "unrelated");
        assert_eq!(db.rows["accounts"][0]["id"], "admin");
        assert_eq!(purge(&mut db, &seed()).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn rejects_mixed_adjustment_before_any_delete() {
        let mut db = MemoryStore::default();
        db.add(
            "stock_adjustment_lines",
            "demo-line",
            &[("sku_id", "demo-sku"), ("stock_adjustment_id", "adjustment")],
        );
        db.add(
            "stock_adjustment_lines",
            "real-line",
            &[("sku_id", "real-sku"), ("stock_adjustment_id", "adjustment")],
        );
        db.add("stock_adjustments", "adjustment", &[]);
        assert!(purge(&mut db, &seed()).await.is_err());
        assert!(db.deleted.is_empty());
        assert_eq!(db.rows["stock_adjustment_lines"].len(), 2);
    }

    #[tokio::test]
    async fn follows_draft_sku_to_order_and_return_chain() {
        let mut db = MemoryStore::default();
        db.add(
            "sales_order_working_copy_lines",
            "line",
            &[("sku_id", "demo-sku"), ("working_copy_id", "draft")],
        );
        db.add("sales_order_working_copies", "draft", &[("sales_order_id", "order")]);
        db.add("sales_orders", "order", &[]);
        db.add("sales_return_cases", "return", &[("sales_order_id", "order")]);
        db.add("sales_return_lines", "return-line", &[("sales_return_case_id", "return")]);
        assert_eq!(purge(&mut db, &seed()).await.unwrap(), 5);
    }

    #[tokio::test]
    async fn expansion_beyond_eight_rounds_is_complete_and_limit_fails_closed() {
        for (depth, succeeds) in [(12, true), (MAX_ROUNDS + 2, false)] {
            let mut db = MemoryStore::default();
            db.add("sales_orders", "root", &[("customer_id", "demo-customer")]);
            let mut parent = "root".to_string();
            for index in 0..depth {
                let id = format!("node-{index}");
                db.add("approval_node_executions", &id, &[("process_instance_id", &parent)]);
                parent = id;
            }
            let result = purge(&mut db, &seed()).await;
            assert_eq!(result.is_ok(), succeeds);
            if succeeds {
                assert_eq!(result.unwrap(), u64::try_from(depth + 1).unwrap());
            } else {
                assert!(db.deleted.is_empty());
            }
        }
    }

    #[tokio::test]
    async fn residual_and_write_errors_never_report_success() {
        let mut db = MemoryStore { retain_rows: true, ..Default::default() };
        db.add("sales_orders", "order", &[("customer_id", "demo-customer")]);
        assert!(purge(&mut db, &seed()).await.is_err());
        db.retain_rows = false;
        db.fail_delete = true;
        assert!(purge(&mut db, &seed()).await.is_err());
        db.fail_delete = false;
        assert_eq!(purge(&mut db, &seed()).await.unwrap(), 1);
    }
    #[tokio::test]
    async fn stock_cleanup_does_not_pull_unrelated_source_order() {
        let mut db = MemoryStore::default();
        db.add(
            "stock_movements",
            "movement",
            &[("sku_id", "demo-sku"), ("source_document_id", "real-order")],
        );
        db.add("sales_orders", "real-order", &[("customer_id", "real-customer")]);
        assert_eq!(purge(&mut db, &seed()).await.unwrap(), 1);
        assert_eq!(db.rows["sales_orders"][0]["id"], "real-order");
    }

    #[tokio::test]
    async fn warehouse_with_non_demo_stock_fails_before_writes() {
        let mut db = MemoryStore::default();
        db.add("stock_balances", "real-stock", &[("sku_id", "real-sku"), ("warehouse_id", "demo-warehouse")]);
        let mut roots = seed();
        roots.warehouse.push("demo-warehouse".into());
        assert!(purge(&mut db, &roots).await.is_err());
        assert!(db.deleted.is_empty());
    }
    #[tokio::test]
    async fn draft_line_without_demo_sku_blocks_cleanup() {
        let mut db = MemoryStore::default();
        db.add("sales_orders", "order", &[("customer_id", "demo-customer")]);
        db.add("sales_order_working_copies", "draft", &[("sales_order_id", "order")]);
        db.add("sales_order_working_copy_lines", "voucher-or-empty", &[("working_copy_id", "draft")]);
        assert!(purge(&mut db, &seed()).await.is_err());
        assert!(db.deleted.is_empty());
    }
}
