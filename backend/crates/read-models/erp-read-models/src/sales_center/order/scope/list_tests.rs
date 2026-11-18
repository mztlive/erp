#![cfg(test)]
//! 无数据库的列表协调行为测试；替身只提供用例阶段，不复制仓储及授权实现。

use std::sync::Mutex;

use erp_core::AccountKind;
use erp_core::money::Amount;
use erp_identity::testing::unbound_authorization;
use erp_sales::entity::sales_order::{
    BusinessType, CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress,
    OriginSystem, ReviewStatus,
};
use persistence_core::testing::SnapshotExecutor;
use persistence_core::{ExecutionBoundary, ReadExecutor};

use super::*;
use crate::sales_center::order::dto::SalesOrderStageSummary;

#[derive(Default)]
struct ListReader {
    calls: Mutex<Vec<(&'static str, usize)>>,
    fail_load: bool,
    fail_present: bool,
    fail_delivery: bool,
    change_stage: bool,
}

impl ListReader {
    /// 记录实际传入的执行器身份，不自行构造执行器。
    fn record(&self, stage: &'static str, executor: &mut dyn Executor) {
        let id = executor as *mut dyn Executor as *mut () as usize;
        self.calls.lock().unwrap().push((stage, id));
    }

    /// 读取已完成阶段，供断言原执行器及失败停止顺序。
    fn stages(&self) -> Vec<&'static str> {
        self.calls.lock().unwrap().iter().map(|(stage, _)| *stage).collect()
    }
}

#[async_trait]
impl SalesListRead for ListReader {
    type Context = ExecutionBoundary;

    /// 返回分页元数据和设施签发的原阶段；不模拟数据库查询。
    async fn load(
        &self,
        params: &SalesListParams,
        _: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesListSnapshot<Self::Context>> {
        self.record("load", executor);
        if self.fail_load {
            return Err(Error::Forbidden("列表资格被拒绝".into()));
        }
        Ok(SalesListSnapshot {
            page: PageView { items: Vec::new(), total: 31, page: params.page.unwrap_or(1), page_size: 20 },
            context: executor.boundary().unwrap(),
            scope_version: version(),
            as_of: "2026-11-14T00:00:00+00:00".into(),
        })
    }

    /// 提供已补齐展示的页面，刻意等待一次以验证交付检查的先后顺序。
    async fn present(
        &self,
        page: PageView<SalesOrderRow>,
        executor: &mut dyn Executor,
    ) -> Result<PageView<SalesOrderView>> {
        self.record("present", executor);
        tokio::task::yield_now().await;
        if self.fail_present {
            return Err(Error::ConflictError("客户事实读取失败".into()));
        }
        if self.change_stage {
            executor.next_read();
        }
        self.record("present_complete", executor);
        Ok(PageView {
            items: vec![presented_row()],
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    /// 使用真实边界检查原阶段，模拟最终交付失败并结束成功阶段。
    async fn validate(&self, context: &Self::Context, executor: &mut dyn Executor) -> Result<()> {
        self.record("validate", executor);
        if self.fail_delivery || !context.is_current(executor) {
            return Err(Error::ConflictError("列表授权已失效".into()));
        }
        executor.next_read();
        Ok(())
    }
}

/// 构造当前页已完成金额、任务及客户补齐的替身数据。
pub(super) fn presented_row() -> SalesOrderView {
    SalesOrderView {
        id: "sale".into(),
        order_no: "SO-1".into(),
        business_type: BusinessType::GoodsService,
        origin_system: OriginSystem::Erp,
        customer_id: "customer".into(),
        customer_name: Some("客户名称".into()),
        contract_id: None,
        commercial_status: CommercialStatus::Draft,
        review_status: ReviewStatus::InApproval,
        fulfillment_progress: FulfillmentProgress::NotStarted,
        collection_progress: CollectionProgress::NotCollected,
        invoice_progress: InvoiceProgress::NotInvoiced,
        close_status: CloseStatus::NotSatisfied,
        effective_at: None,
        closed_at: None,
        version: 3,
        created_at: 1,
        updated_at: 1,
        owner_user_id: "sales".into(),
        owner_user_name: Some("销售姓名".into()),
        gross_amount: "113".parse().unwrap(),
        net_amount: "100".parse().unwrap(),
        tax_amount: "13".parse().unwrap(),
        stage: SalesOrderStageSummary {
            code: "awaiting_sales_lead",
            label: "待销售主管审核",
            tone: "warning",
            owner_role: Some("sales_lead".into()),
            owner_user_id: Some("reviewer".into()),
            owner_user_name: Some("审核姓名".into()),
            due_at: Some(10),
        },
    }
}

/// 跨页使用生产版本组合函数，固定授权及匹配销售版本。
fn version() -> String {
    list_scope_version("scope", &[SalesVersion { id: "sale".into(), version: 3 }])
}

/// 选择第二页，验证原页码及完整匹配总数不会被补齐步骤重算。
fn params(expected: Option<&str>) -> SalesListParams {
    serde_json::from_value(serde_json::json!({"page": 2, "scope_version": expected})).unwrap()
}

/// 构造真实类型的当前操作人。
fn actor() -> AuditActor {
    AuditActor::new("actor".into(), "操作人".into(), AccountKind::Admin)
}

#[tokio::test]
async fn production_load_checks_authorization_before_keyword_or_business_reads() {
    let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
        .await
        .unwrap()
        .database("sales_list_authorization");
    let service = SalesOrderReadService::new(db, unbound_authorization());
    let params = serde_json::from_value(serde_json::json!({"q": "客户关键词"})).unwrap();
    let result = service.load(&params, &actor(), &mut SnapshotExecutor::default()).await;
    assert!(matches!(
        result,
        Err(Error::ValidationError(reason)) if reason == "缺少查询 Mapping 绑定"
    ));
}

#[tokio::test]
async fn complete_page_is_delivered_only_after_enrichment_and_validation_on_the_original_executor() {
    let reader = ListReader::default();
    let mut executor = SnapshotExecutor::default();
    let original = &mut executor as *mut SnapshotExecutor as usize;
    let boundary = executor.boundary().unwrap();

    let view = read_list(&reader, &params(Some(&version())), &actor(), &mut executor).await.unwrap();
    assert_eq!(reader.stages(), ["load", "present", "present_complete", "validate"]);
    assert!(reader.calls.lock().unwrap().iter().all(|(_, id)| *id == original));
    assert!(!boundary.is_current(&executor));

    assert_eq!((view.data.total, view.data.page, view.data.page_size), (31, 2, 20));
    assert_eq!(view.scope_version, version());
    assert_eq!(view.data.items, [presented_row()]);
    assert_eq!(view.data.items[0].gross_amount, "113".parse::<Amount>().unwrap());
}

#[tokio::test]
async fn delivery_failure_discards_the_completed_page_and_phase_changes_are_rejected() {
    for reader in [
        ListReader { fail_delivery: true, ..Default::default() },
        ListReader { change_stage: true, ..Default::default() },
    ] {
        let result = read_list(&reader, &params(None), &actor(), &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::ConflictError(reason)) if reason == "列表授权已失效"));
        assert_eq!(reader.stages(), ["load", "present", "present_complete", "validate"]);
    }
}

#[tokio::test]
async fn failed_loading_or_enrichment_stops_the_page_before_delivery() {
    let reader = ListReader { fail_load: true, ..Default::default() };
    let result = read_list(&reader, &params(None), &actor(), &mut SnapshotExecutor::default()).await;
    assert!(matches!(result, Err(Error::Forbidden(reason)) if reason == "列表资格被拒绝"));
    assert_eq!(reader.stages(), ["load"]);

    let reader = ListReader { fail_present: true, ..Default::default() };
    let result = read_list(&reader, &params(None), &actor(), &mut SnapshotExecutor::default()).await;
    assert!(matches!(result, Err(Error::ConflictError(reason)) if reason == "客户事实读取失败"));
    assert_eq!(reader.stages(), ["load", "present"]);
}

#[tokio::test]
async fn a_stale_page_version_stops_before_enrichment_and_unscoped_execution_stops_before_loading() {
    let reader = ListReader::default();
    let result = read_list(&reader, &params(Some("old")), &actor(), &mut SnapshotExecutor::default()).await;
    assert!(matches!(result, Err(Error::ConflictError(_))));
    assert_eq!(reader.stages(), ["load"]);

    let reader = ListReader::default();
    let result = read_list(&reader, &params(None), &actor(), &mut ReadExecutor::default()).await;
    assert!(matches!(result, Err(Error::ValidationError(reason)) if reason == "销售列表必须使用快照事务"));
    assert!(reader.stages().is_empty());
}

#[test]
fn list_version_keeps_the_existing_full_match_hash_and_detects_changes_outside_the_current_page() {
    let mut versions = vec![
        SalesVersion { id: "first-page-sale".into(), version: 1 },
        SalesVersion { id: "another-page-sale".into(), version: 2 },
    ];
    let mut legacy = DefaultHasher::new();
    versions.hash(&mut legacy);
    let version = list_scope_version("scope", &versions);
    assert_eq!(version, format!("scope:{:x}", legacy.finish()));

    versions[1].version += 1;
    assert_ne!(version, list_scope_version("scope", &versions));
    versions[1].version -= 1;
    assert_ne!(version, list_scope_version("changed-scope", &versions));
    assert_ne!(version, list_scope_version("scope", &versions[..1]));
}
