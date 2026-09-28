//! 在同一事务中收集关联闭包、检查混用、按 ID 清理并验证残留。

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use mongodb::Database;
use persistence_core::Executor;
use persistence_core::repository::LinkedId;

use super::derived_graph::{self, SeedKind};
use super::plan::DemoKind;
use super::record::DemoMasterRecord;
use super::repository;
use crate::{Error, Result};

type IdSet = BTreeSet<String>;
pub(super) type DeletionSet = BTreeMap<String, IdSet>;
const MAX_ROUNDS: usize = 128;

/// 由清单实际 ID 派生的清理根。
#[derive(Clone, Default)]
pub(super) struct MasterIds {
    customer: Vec<String>,
    party: Vec<String>,
    supplier: Vec<String>,
    sku: Vec<String>,
    warehouse: Vec<String>,
    references: Vec<String>,
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
        ids.references.push(record.entity_id.clone());
        ids.references.extend(record.related_ids.iter().cloned());
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

/// 清理编排只依赖精确关联读取和 ID 删除。
#[async_trait]
pub(super) trait Store: Send {
    /// 精确读取关联 ID，包含软删除记录。
    ///
    /// # 参数
    /// collection - 已绑定集合；field - 外键；values - 精确值；parent - 父键投影。
    /// # 返回
    /// 返回匹配记录的实际 ID 和可选父键。
    /// # 错误
    /// 读取失败或关联身份无效时返回错误。
    async fn linked(
        &mut self,
        collection: &str,
        field: &str,
        values: &[String],
        parent: Option<&str>,
    ) -> Result<Vec<LinkedId>>;
    /// 按实际 ID 硬删除已收集记录。
    ///
    /// # 参数
    /// collection - 已绑定集合；ids - 精确主键。
    /// # 返回
    /// 实际删除条数。
    /// # 错误
    /// 删除失败时返回错误。
    async fn delete(&mut self, collection: &str, ids: &[String]) -> Result<u64>;
}

pub(super) struct MongoStore<'a> {
    pub(super) db: &'a Database,
    pub(super) executor: &'a mut dyn Executor,
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

/// 按登记外键扩展到不再增长；达到保护上限时失败，禁止部分清理。
///
/// # 参数
/// store - 当前事务数据访问；seed - 登记清单实际 ID。
/// # 返回
/// 关联记录的精确删除集合。
/// # 错误
/// 混用、超出遍历上限或读取失败时返回错误。
pub(super) async fn collect(store: &mut impl Store, seed: &MasterIds) -> Result<DeletionSet> {
    let mut doomed = DeletionSet::new();
    let mut parents = seed.references.iter().cloned().collect::<IdSet>();
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
    use super::super::removal::{RemovalStore, execute};
    use super::*;

    #[derive(Default)]
    struct MemoryStore {
        rows: BTreeMap<String, Vec<BTreeMap<String, String>>>,
        deleted: Vec<(String, Vec<String>)>,
        retain_rows: bool,
        fail_delete: bool,
        records: Vec<DemoMasterRecord>,
        forgotten: Vec<String>,
        fail_forget: bool,
        companies: Vec<String>,
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

    #[async_trait]
    impl RemovalStore for MemoryStore {
        async fn records(&mut self) -> Result<Vec<DemoMasterRecord>> {
            Ok(self.records.clone())
        }
        async fn guard_companies(&mut self, ids: &[String]) -> Result<()> {
            if ids.iter().any(|id| self.companies.contains(id)) {
                return Err(Error::BusinessLogicError("company protected".into()));
            }
            Ok(())
        }
        async fn forget(&mut self, records: &[DemoMasterRecord]) -> Result<()> {
            if self.fail_forget {
                return Err(Error::Internal("injected manifest failure".into()));
            }
            for row in records {
                self.forgotten.push(row.key.clone());
                self.records.retain(|item| item.key != row.key);
            }
            Ok(())
        }
    }

    fn record(kind: &str, id: &str, related: &[&str]) -> DemoMasterRecord {
        DemoMasterRecord {
            kind: kind.into(),
            key: format!("seed-{id}"),
            entity_id: id.into(),
            related_ids: related.iter().map(|id| id.to_string()).collect(),
            label: id.into(),
            removed: false,
        }
    }

    #[tokio::test]
    async fn hard_delete_clears_roots_revisions_commands_and_manifest_then_repeats_empty() {
        let mut db = MemoryStore {
            records: vec![
                record("customer", "customer", &["party"]),
                record("supplier", "supplier", &["supplier-party"]),
                record("product", "product", &[]),
                record("warehouse", "warehouse", &[]),
                record("unit", "unit", &[]),
                record("brand", "brand", &[]),
                record("category", "category", &[]),
            ],
            ..Default::default()
        };
        db.records[0].removed = true;
        for (collection, id) in [
            ("customer_accounts", "customer"),
            ("supplier_accounts", "supplier"),
            ("parties", "party"),
            ("parties", "supplier-party"),
            ("products", "product"),
            ("warehouses", "warehouse"),
            ("product_brands", "brand"),
            ("product_categories", "category"),
            ("unit_of_measures", "unit"),
        ] {
            db.add(collection, id, &[]);
        }
        for (collection, id, field, parent) in [
            ("customer_profile_commands", "customer-command", "customer_id", "customer"),
            ("customer_assignments", "assignment", "customer_id", "customer"),
            ("supplier_profile_commands", "supplier-command", "supplier_id", "supplier"),
            ("supplier_commercial_profile_revisions", "commercial", "supplier_id", "supplier"),
            ("supplier_capabilities", "capability", "supplier_id", "supplier"),
            ("supplier_capability_revisions", "capability-revision", "supplier_id", "supplier"),
            ("supplier_qualifications", "qualification", "supplier_id", "supplier"),
            ("supplier_qualification_revisions", "qualification-revision", "supplier_id", "supplier"),
            ("supplier_rating_revisions", "rating", "supplier_id", "supplier"),
            (
                "supplier_qualification_capabilities",
                "qualification-link",
                "qualification_id",
                "qualification",
            ),
            ("party_revisions", "party-revision", "party_id", "party"),
            ("party_contacts", "contact", "party_id", "party"),
            ("party_addresses", "address", "party_id", "party"),
            ("party_bank_accounts", "bank", "party_id", "party"),
            ("party_tax_profiles", "tax", "party_id", "party"),
            ("product_revisions", "product-revision", "product_id", "product"),
            ("product_revision_medias", "media", "product_revision_id", "product-revision"),
            ("skus", "late-sku", "product_id", "product"),
            ("sku_revisions", "sku-revision", "sku_id", "late-sku"),
            ("sku_revision_attribute_values", "attribute", "sku_revision_id", "sku-revision"),
            ("voucher_category_profile_revisions", "voucher", "sku_id", "late-sku"),
            ("warehouse_revisions", "warehouse-revision", "warehouse_id", "warehouse"),
            ("warehouse_sku_policies", "policy", "sku_id", "late-sku"),
            ("product_category_attributes", "category-attribute", "category_id", "category"),
            ("document_attachments", "attachment", "document_id", "product-revision"),
            ("sales_orders", "order", "customer_id", "customer"),
            ("work_items", "customer-task", "business_object_id", "customer"),
        ] {
            db.add(collection, id, &[(field, parent)]);
        }
        db.add("products", "real-product", &[]);
        db.add("parties", "company", &[]);
        db.add("customer_profile_commands", "real-command", &[("customer_id", "real-customer")]);
        db.add("accounts", "admin", &[]);
        db.add("file_assets", "asset", &[]);
        db.add("audit_logs", "audit", &[]);
        let outcome = execute(&mut db, &[]).await.unwrap();
        assert!(outcome.done);
        assert_eq!(outcome.removed, 7);
        assert!(db.records.is_empty());
        assert_eq!(db.forgotten.len(), 7);
        let survivors = db.rows.values().flatten().map(|row| row["id"].as_str()).collect::<BTreeSet<_>>();
        assert_eq!(
            survivors,
            BTreeSet::from(["real-product", "real-command", "company", "admin", "asset", "audit"])
        );
        let writes = db.deleted.len();
        let again = execute(&mut db, &[]).await.unwrap();
        assert!(again.done);
        assert_eq!((again.removed, again.related), (0, 0));
        assert_eq!(db.deleted.len(), writes);
    }

    #[tokio::test]
    async fn hard_delete_rejects_shared_master_references_before_any_write() {
        for (kind, collection, field, parent) in [
            ("brand", "product_revisions", "brand_id", Some(("product_id", "real-product"))),
            ("category", "product_categories", "parent_category_id", None),
            ("unit", "skus", "base_unit_id", None),
            ("customer", "supplier_accounts", "party_id", None),
            ("warehouse", "warehouse_sku_policies", "warehouse_id", Some(("sku_id", "real-sku"))),
        ] {
            let mut db = MemoryStore::default();
            let related = if kind == "customer" { vec!["root"] } else { vec![] };
            db.records.push(record(kind, "root", &related));
            let mut fields = vec![(field, "root")];
            fields.extend(parent);
            db.add(collection, "real-reference", &fields);
            assert!(execute(&mut db, &[]).await.is_err(), "{kind}");
            assert!(db.deleted.is_empty());
            assert!(db.forgotten.is_empty());
        }
    }

    #[tokio::test]
    async fn hard_delete_failures_keep_manifest_and_never_report_success() {
        for failure in 0..4 {
            let mut db = MemoryStore::default();
            db.records.push(record("customer", "customer", &["party"]));
            db.add("customer_accounts", "customer", &[("party_id", "party")]);
            db.add("parties", "party", &[]);
            match failure {
                0 => db.fail_delete = true,
                1 => db.retain_rows = true,
                2 => db.fail_forget = true,
                _ => db.companies.push("party".into()),
            }
            assert!(execute(&mut db, &[]).await.is_err());
            assert_eq!(db.records.len(), 1);
            assert!(db.forgotten.is_empty());
        }
    }

    #[tokio::test]
    async fn hard_delete_chunks_include_old_soft_deleted_records_and_finish() {
        let mut db = MemoryStore::default();
        for n in 0..10 {
            let id = format!("unit-{n}");
            let mut row = record("unit", &id, &[]);
            row.removed = true;
            db.records.push(row);
            db.add("unit_of_measures", &id, &[]);
        }
        let first = execute(&mut db, &[]).await.unwrap();
        assert!(!first.done);
        assert_eq!(first.removed, 8);
        assert_eq!(db.records.len(), 2);
        let last = execute(&mut db, &[]).await.unwrap();
        assert!(last.done);
        assert_eq!(last.removed, 2);
        assert!(db.rows["unit_of_measures"].is_empty());
    }

    #[tokio::test]
    async fn shared_registered_party_is_kept_until_last_role_is_deleted() {
        let mut db = MemoryStore::default();
        db.records.push(record("customer", "customer", &["party"]));
        db.add("customer_accounts", "customer", &[("party_id", "party")]);
        db.add("parties", "party", &[]);
        for n in 0..8 {
            let id = format!("supplier-{n}");
            db.records.push(record("supplier", &id, &["party"]));
            db.add("supplier_accounts", &id, &[("party_id", "party")]);
        }
        let first = execute(&mut db, &[]).await.unwrap();
        assert!(!first.done);
        assert_eq!(first.removed, 8);
        assert_eq!(db.rows["parties"].len(), 1);
        let last = execute(&mut db, &[]).await.unwrap();
        assert!(last.done);
        assert_eq!(last.related, 1);
        assert!(db.rows["parties"].is_empty());
    }

    #[tokio::test]
    async fn missing_primary_still_cleans_children_and_counts_actual_related_rows() {
        let mut db = MemoryStore::default();
        db.records.push(record("customer", "gone-customer", &["gone-party"]));
        db.add("customer_profile_commands", "command", &[("customer_id", "gone-customer")]);
        let result = execute(&mut db, &[]).await.unwrap();
        assert_eq!((result.removed, result.related), (1, 1));
        assert!(db.records.is_empty());
        assert!(db.rows["customer_profile_commands"].is_empty());
    }

    #[tokio::test]
    async fn mixed_document_blocks_master_and_manifest_deletion() {
        let mut db = MemoryStore::default();
        db.records.push(record("customer", "demo-customer", &["party"]));
        db.add("customer_accounts", "demo-customer", &[("party_id", "party")]);
        db.add("sales_orders", "order", &[("customer_id", "demo-customer")]);
        db.add("sales_order_working_copies", "draft", &[("sales_order_id", "order")]);
        db.add(
            "sales_order_working_copy_lines",
            "line",
            &[("working_copy_id", "draft"), ("sku_id", "real-sku")],
        );
        assert!(execute(&mut db, &[]).await.is_err());
        assert!(db.deleted.is_empty());
        assert!(db.forgotten.is_empty());
    }

    /// 构造登记夹具后执行实际生产硬删除流程，不复制清理编排。
    async fn purge(db: &mut MemoryStore, seed: &MasterIds) -> Result<u64> {
        db.records.clear();
        for id in &seed.customer {
            db.records.push(record("customer", id, &[]));
        }
        for id in &seed.supplier {
            db.records.push(record("supplier", id, &[]));
        }
        for id in &seed.warehouse {
            db.records.push(record("warehouse", id, &[]));
        }
        if !seed.sku.is_empty() {
            db.records.push(record(
                "product",
                "demo-product",
                &seed.sku.iter().map(String::as_str).collect::<Vec<_>>(),
            ));
        }
        Ok(u64::from(execute(db, &[]).await?.related))
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
