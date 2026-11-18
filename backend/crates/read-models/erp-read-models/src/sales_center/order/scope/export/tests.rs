use std::sync::Mutex;

use application_core::PageView;
use erp_core::AccountKind;
use persistence_core::ReadExecutor;
use persistence_core::testing::SnapshotExecutor;

use super::super::list_tests::presented_row;
use super::*;

#[derive(Default)]
struct Reader {
    calls: Mutex<Vec<(&'static str, usize)>>,
    fail_final: bool,
    change: Option<&'static str>,
    hidden_contract: bool,
}

impl Reader {
    fn record(&self, step: &'static str, executor: &mut dyn Executor) {
        self.calls.lock().unwrap().push((step, executor as *mut dyn Executor as *mut () as usize));
    }

    fn stages(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().iter().map(|(step, _)| *step).collect()
    }
}

#[async_trait]
impl ExportRead for Reader {
    async fn page(
        &self,
        _: &SalesListParams,
        _: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListView> {
        let final_read = !self.calls.lock().unwrap().is_empty();
        self.record(if final_read { "final-sales-query-and-delivery" } else { "initial-sales" }, executor);
        if final_read && self.fail_final {
            return Err(Error::ConflictError("销售根交付检查失败".into()));
        }
        executor.next_read();
        let mut row = presented_row();
        row.contract_id = Some("contract".into());
        let mut view = SalesListView {
            data: PageView { items: vec![row], total: 31, page: 2, page_size: 20 },
            ownership_basis: "document_sales_owner",
            scope_version: "scope-version".into(),
            as_of: "2026-11-16T00:00:00Z".into(),
            empty_reason: None,
            scope_summary: "销售范围",
        };
        if final_read {
            match self.change {
                Some("root") => view.data.items[0].id = "another-sale".into(),
                Some("version") => view.data.items[0].version += 1,
                Some("contract") => view.data.items[0].contract_id = Some("another-contract".into()),
                Some("scope") => view.scope_version = "another-scope".into(),
                Some("total") => view.data.total += 1,
                _ => {},
            }
        }
        Ok(view)
    }

    async fn contracts(
        &self,
        ids: &BTreeSet<String>,
        _: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, String>> {
        assert_eq!(*ids, BTreeSet::from(["contract".into()]));
        assert!(executor.boundary().unwrap().is_snapshot());
        self.record("contract-query-and-delivery", executor);
        executor.next_read();
        Ok(if self.hidden_contract { HashMap::new() } else { [("contract".into(), "HT-1".into())].into() })
    }
}

fn params() -> SalesListParams {
    serde_json::from_value(serde_json::json!({})).unwrap()
}

fn actor() -> AuditActor {
    AuditActor::new("sales".into(), "销售".into(), AccountKind::Admin)
}

#[tokio::test]
async fn complete_export_uses_one_executor_and_finishes_with_the_sales_root_delivery() {
    for hidden_contract in [false, true] {
        let reader = Reader { hidden_contract, ..Default::default() };
        let mut executor = SnapshotExecutor::default();
        let pointer = &mut executor as *mut SnapshotExecutor as usize;
        let result = export_page(&reader, &params(), &actor(), &mut executor).await.unwrap();
        assert_eq!(
            reader.stages(),
            ["initial-sales", "contract-query-and-delivery", "final-sales-query-and-delivery"]
        );
        assert!(reader.calls.lock().unwrap().iter().all(|(_, id)| *id == pointer));
        assert_eq!(
            result["items"][0]["contract_no"],
            if hidden_contract { "合同资料不可读取" } else { "HT-1" }
        );
        assert_eq!(result["items"][0]["id"], "sale");
        assert_eq!(result["items"][0]["owner_user_name"], "销售姓名");
        assert_eq!(result["total"], 31);
        assert_eq!(result["scope_version"], "scope-version");
    }
}

#[tokio::test]
async fn final_authorization_or_changed_root_binding_discards_contract_numbers_and_the_whole_page() {
    let reader = Reader { fail_final: true, ..Default::default() };
    let result = export_page(&reader, &params(), &actor(), &mut SnapshotExecutor::default()).await;
    assert!(matches!(result, Err(Error::ConflictError(message)) if message == "销售根交付检查失败"));

    for change in ["root", "version", "contract", "scope", "total"] {
        let reader = Reader { change: Some(change), ..Default::default() };
        let result = export_page(&reader, &params(), &actor(), &mut SnapshotExecutor::default()).await;
        assert!(
            matches!(result, Err(Error::ConflictError(message)) if message == "销售导出根或合同引用已变化，请重新查询")
        );
    }
}

#[tokio::test]
async fn export_refuses_non_snapshot_before_any_reads() {
    let reader = Reader::default();
    let result = export_page(&reader, &params(), &actor(), &mut ReadExecutor::default()).await;
    assert!(matches!(result, Err(Error::ValidationError(message)) if message == "销售导出必须使用快照事务"));
    assert!(reader.stages().is_empty());
}
