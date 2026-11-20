//! 文件任务的查询、权限和格式编排；查询适配器由应用组合根装配。
use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, internal, work_item};
use erp_supplier_catalog::supplier_portal::SupplierPortalProcess;
use erp_supplier_catalog::supply_governance::SupplierOfferingProcess;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use storage::S3Storage;

use super::csv::render as render_csv;
use super::master::rows as master_rows;
use super::{TransferContext, TransferHandler, TransferKind, TransferResult};
use crate::sales_inquiry_import::SalesInquiryImportProcess;
use crate::{Error, Result};

/// 组合根将固定业务查询接回原有 Service/ReadModel，不允许自由查询数据库或 URL。
#[async_trait]
pub trait TransferReader: Send + Sync {
    /// 读取与业务页面同口径的一页。
    ///
    /// 固定任务类型、服务端分页参数与认证人。
    ///
    /// 原接口 JSON 分页视图。
    ///
    /// 原业务权限、版本和查询错误原样保留。
    async fn page(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value>;
    /// 在原功能授权快照中读取商品池或报价来源页。
    ///
    /// kind、input 为固定商品池或报价任务，actor 为当前身份，executor 为原事务。
    ///
    /// 与业务页面相同的已授权分页视图。
    ///
    /// 不支持的任务、原业务授权或数据库失败时返回错误，不切换执行器。
    async fn page_with(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Value>;
    /// 沿独立的申请详情权限查询与 `work_item_id` 精确绑定的开票申请。
    /// 未找到可读取的详情时返回空，不公开绑定是否存在。
    async fn linked_invoice_request(
        &self,
        work_item_id: &str,
        receivable_account_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<Value>>;
    /// 读取原报表的完整 CSV。
    ///
    /// 固定报表类型、条件与认证人。
    ///
    /// 原报表导出结果。
    ///
    /// 原报表错误原样保留。
    async fn report(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value>;
}

/// 文件任务消费的已装配业务命令，具体领域服务和授权 Port 由应用提供。
#[derive(Clone)]
pub struct BusinessTransferServices {
    pub inquiry: SalesInquiryImportProcess,
    pub offerings: Arc<SupplierOfferingProcess>,
    pub portal: Arc<SupplierPortalProcess>,
}

/// 默认导入导出业务实现。
#[derive(Clone)]
pub struct BusinessTransfers {
    pub(super) db: Database,
    pub(super) authorization: Arc<AuthorizationService>,
    pub(super) reader: Arc<dyn TransferReader>,
    pub(super) storage: S3Storage,
    pub(super) services: BusinessTransferServices,
}
impl BusinessTransfers {
    /// 装配当前身份源及固定业务查询适配器。
    ///
    /// 数据库、RBAC、原业务查询适配器。
    ///
    /// 未执行 I/O 的任务处理器。
    pub fn new(
        db: Database,
        authorization: Arc<AuthorizationService>,
        reader: Arc<dyn TransferReader>,
        storage: S3Storage,
        services: BusinessTransferServices,
    ) -> Self {
        Self { db, authorization, reader, storage, services }
    }
    // 在独立快照中预检当前功能或门户会话资格。
    //
    // kind、input 为固定任务，actor 为当前身份。
    //
    // 本次预检通过时完成；实际业务读取和写入仍重新授权。
    //
    // 事务、资格或身份读取失败时返回错误。
    pub(super) async fn access(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<()> {
        let service = self.clone();
        let input = input.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.access_with(kind, &input, &actor, executor).await })
            })
            .await
    }
    // 在调用方执行器核验本任务原有功能资格，目标条件由所属业务入口执行。
    //
    // kind、input 为固定业务任务，actor 为当前身份，executor 为本次原执行器。
    //
    // 原功能权限或门户写资格成立时成功。
    //
    // 任务类型、身份或功能资格无效时拒绝。
    pub(super) async fn access_with(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if actor.kind() == AccountKind::Supplier && kind != TransferKind::PortalBatch {
            return Err(Error::Forbidden("此任务仅供管理端使用".into()));
        }
        if kind == TransferKind::PortalBatch {
            return self.portal_actor_with(input, actor, executor).await?.require_write().map_err(Into::into);
        }
        function_access(&self.authorization, kind, input, actor, executor).await
    }
    pub(super) async fn collect(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<Vec<Value>> {
        Ok(self.collect_export(kind, input, actor, 100_000).await?.0)
    }

    pub(super) async fn collect_export(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        limit: u64,
    ) -> Result<(Vec<Value>, Value)> {
        let mut query = input.clone();
        let mut rows = Vec::new();
        let mut identities = HashSet::new();
        let mut expected = None;

        for page in 1..=1000 {
            query["page"] = json!(page);
            query["page_size"] = json!(100);
            let response = self.export_page(kind, &query, actor).await?;
            if response["empty_reason"] == "no_scope" {
                return Err(Error::Forbidden("当前无可导出范围".into()));
            }

            let total =
                response["total"].as_u64().ok_or_else(|| Error::Internal("导出列表缺少总数".into()))?;
            if total > 100_000 || expected.is_some_and(|value| value != total) {
                return Err(Error::ConflictError("结果已变化或超过十万行，请缩小范围".into()));
            }
            if total > limit {
                return Err(Error::ValidationError(format!("导出超过 {limit} 条记录，请缩小范围")));
            }
            expected = Some(total);
            let items =
                response["items"].as_array().ok_or_else(|| Error::Internal("导出列表格式错误".into()))?;
            bind_scope(&mut query, &response)?;

            for row in items {
                let id = row
                    .get("id")
                    .or_else(|| row.get("sku_id"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::Internal("导出行缺少身份".into()))?;
                if !identities.insert(id.to_string()) {
                    return Err(Error::ConflictError("导出结果重复，请重新查询".into()));
                }
                rows.push(row.clone());
            }

            if u64::try_from(rows.len()).ok() == expected {
                query["page"] = json!(1);
                self.check_final_scope(kind, &query, actor, expected).await?;
                return Ok((
                    rows,
                    json!({"scope_version": query["scope_version"],
                    "queue_context_id": query["queue_context_id"]}),
                ));
            }
            if items.is_empty() {
                return Err(Error::ConflictError("导出结果不完整".into()));
            }
        }
        Err(Error::ValidationError("导出超过十万行，请缩小范围".into()))
    }
    // 商品池及报价页的附加资格与实际业务读取使用同一快照。
    //
    // kind、query 为固定业务分页，actor 为当前身份。
    //
    // 原业务已完成授权的分页结果。
    //
    // 功能资格、数据授权或业务读取失败时拒绝交付。
    pub(super) async fn export_page(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
    ) -> Result<Value> {
        if snapshot_page(kind, query) {
            let authorization = self.authorization.clone();
            let reader = self.reader.clone();
            let query = query.clone();
            let actor = actor.clone();
            return self
                .db
                .client()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        authorized_page(&authorization, reader.as_ref(), kind, &query, &actor, executor).await
                    })
                })
                .await;
        }
        self.reader.page(kind, query, actor).await
    }

    pub(super) async fn check_final_scope(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
        expected: Option<u64>,
    ) -> Result<()> {
        let last = self.export_page(kind, query, actor).await?;
        if last["total"].as_u64() != expected
            || last["empty_reason"] == "no_scope"
            || ["scope_version", "queue_context_id"].into_iter().any(|key| {
                let expected = query.get(key).filter(|value| !value.is_null());
                let actual = last.get(key).filter(|value| !value.is_null());
                let version_required = matches!(
                    kind,
                    TransferKind::InvoiceRequestsExcel | TransferKind::WorkbenchInvoiceExecutionExcel
                );
                (version_required || actual.is_some()) && expected != actual
            })
        {
            return Err(Error::ConflictError("导出范围已变化，请重新查询".into()));
        }
        Ok(())
    }
    async fn csv(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<TransferResult> {
        let (name, content, metadata) = if is_report(kind) {
            let report = self.reader.report(kind, input, actor).await?;
            let name = report["fileName"].as_str().ok_or_else(|| Error::Internal("报表文件名缺失".into()))?;
            let csv = report["csvContent"].as_str().ok_or_else(|| Error::Internal("报表内容缺失".into()))?;
            (name.to_string(), csv.to_string(), report.clone())
        } else {
            let rows = self.collect(kind, input, actor).await?;
            let rows = if kind == TransferKind::MasterData { master_rows(input, rows) } else { rows };
            {
                let (name, content) = render_csv(kind, input, &rows)?;
                (name, content, json!({"row_count":rows.len(), "scope_version":input.get("scope_version")}))
            }
        };

        let signed = if is_report(kind) {
            content
                .lines()
                .filter(|line| {
                    !(line.contains("生成时点")
                        && line.trim_start_matches(['\u{feff}', '"']).starts_with("期间口径"))
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            content.clone()
        };
        let hash = hex::encode(Sha256::digest(signed.as_bytes()));

        let mut result = TransferResult::file(
            name,
            "text/csv;charset=utf-8",
            content.into_bytes(),
            json!({"content_hash":hash}),
        )?;
        result.metadata = metadata;
        if let Some(map) = result.metadata.as_object_mut() {
            map.remove("csvContent");
        }
        Ok(result)
    }
}
fn bind_scope(query: &mut Value, response: &Value) -> Result<()> {
    for key in ["scope_version", "queue_context_id"] {
        if let Some(version) = response.get(key).filter(|value| !value.is_null()) {
            if query.get(key).is_some_and(|old| !old.is_null() && old != version) {
                return Err(Error::ConflictError("导出范围已变化，请重新查询".into()));
            }
            query[key] = version.clone();
        }
    }
    Ok(())
}

fn reset_workbench_page(input: &mut Value) -> Result<()> {
    // 原页面版本可能包含其他任务类型；导出从独立的队列快照开始。
    let query = input.as_object_mut().ok_or_else(|| Error::ValidationError("任务查询条件格式无效".into()))?;
    for key in ["queue_context_id", "scope_version", "current_work_item_id", "cursor"] {
        query.remove(key);
    }
    Ok(())
}

#[async_trait]
impl TransferHandler for BusinessTransfers {
    async fn authorize_read(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.read_access(kind, input, actor, executor).await
    }
    async fn authorize_submission(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.submission_access(kind, input, actor, executor).await
    }
    async fn context(
        &self,
        kind: TransferKind,
        input: &Value,
        result: Option<&Value>,
        actor: &AuditActor,
    ) -> Result<TransferContext> {
        self.task_context(kind, input, result, actor).await
    }

    async fn recover(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value> {
        self.access(kind, input, actor).await?;
        match kind {
            TransferKind::SupplyBatch => self.supply_batch(input, actor, true).await,
            TransferKind::PortalBatch => self.portal_recover(input, actor).await,
            _ => Err(Error::ValidationError("该任务不支持批量结果核对".into())),
        }
    }

    async fn prepare(&self, kind: TransferKind, mut input: Value, actor: &AuditActor) -> Result<Value> {
        self.access(kind, &input, actor).await?;
        if kind == TransferKind::WorkbenchInvoiceExecutionExcel {
            reset_workbench_page(&mut input)?;
        }
        if !kind.is_export() {
            return self.prepare_import(kind, input, actor).await;
        }

        if !is_report(kind)
            && !matches!(kind, TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive)
        {
            input["page"] = json!(1);
            input["page_size"] = json!(100);
            let first = self.export_page(kind, &input, actor).await?;
            if first["empty_reason"] == "no_scope" {
                return Err(Error::Forbidden("当前无可导出范围".into()));
            }
            bind_scope(&mut input, &first)?;
        }
        Ok(input)
    }
    async fn execute(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<TransferResult> {
        self.access(kind, input, actor).await?;
        match kind {
            TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive => {
                self.workbook(kind, input, actor).await
            },
            TransferKind::InvoiceRequestsExcel | TransferKind::WorkbenchInvoiceExecutionExcel => {
                self.invoice_workbook(kind, input, actor).await
            },
            kind if !kind.is_export() => self.import(kind, input, actor).await,
            _ => self.csv(kind, input, actor).await,
        }
    }
    async fn authorize_result(
        &self,
        kind: TransferKind,
        input: &Value,
        authorization: &Value,
        actor: &AuditActor,
    ) -> Result<()> {
        self.access(kind, input, actor).await?;
        if !kind.is_export() {
            return self.authorize_import(kind, input, authorization, actor).await;
        }

        let fresh = match kind {
            TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive => {
                self.workbook_authorization(kind, input, actor).await?
            },
            TransferKind::InvoiceRequestsExcel | TransferKind::WorkbenchInvoiceExecutionExcel => {
                self.invoice_export_source(kind, input, actor).await?.authorization()?
            },
            _ => self.csv(kind, input, actor).await?.authorization,
        };
        if fresh != *authorization {
            return Err(Error::ConflictError("数据或授权范围已变化，请重新导出".into()));
        }
        Ok(())
    }
}
// 固定任务资格只通过统一授权服务检查，不在文件任务中组合政策。
async fn function_access(
    authorization: &AuthorizationService,
    kind: TransferKind,
    input: &Value,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if actor.kind() != AccountKind::Admin {
        return Err(Error::Forbidden("此任务仅供管理端使用".into()));
    }
    if kind == TransferKind::WorkbenchInvoiceExecutionExcel {
        let access = authorization.query(actor, &work_item::LIST, executor).await?;
        authorization.validate(&access, executor).await?;
        return Ok(());
    }
    let permission = transfer_permission(kind, input)?;
    authorization.require(actor, &permission, AccessTarget::Operation, executor).await?;
    Ok(())
}

// 读取前在原快照核对全部功能资格，再委托实际业务数据授权。
async fn authorized_page(
    authorization: &AuthorizationService,
    reader: &dyn TransferReader,
    kind: TransferKind,
    query: &Value,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Value> {
    if !snapshot_page(kind, query) {
        return Err(Error::ValidationError("此任务不使用附加资格快照读取".into()));
    }
    function_access(authorization, kind, query, actor, executor).await?;
    reader.page_with(kind, query, actor, executor).await
}

// 需要与附加功能资格共用读取事务的固定任务类型。
fn snapshot_page(kind: TransferKind, input: &Value) -> bool {
    matches!(
        kind,
        TransferKind::ProductPoolExcel
            | TransferKind::SupplierQuoteArchive
            | TransferKind::SupplierQuotePreview
    ) || (kind == TransferKind::MasterData && input["resource"] == "sellable")
}

pub(super) fn is_report(kind: TransferKind) -> bool {
    matches!(
        kind,
        TransferKind::ProfitLoss
            | TransferKind::CustomerQualityCurrent
            | TransferKind::CustomerQualityHistory
    )
}
// 选择固定任务入口；完整功能资格由 identity 定义，业务输入只选择既有用例。
fn transfer_permission(kind: TransferKind, input: &Value) -> Result<Permission> {
    use TransferKind::*;
    Ok(match kind {
        ProductPoolExcel => internal::DATA_TRANSFER_PRODUCT_POOL,
        SupplierQuoteArchive | SupplierQuotePreview => internal::DATA_TRANSFER_SUPPLIER_QUOTES,
        SalesOrders => internal::DATA_TRANSFER_SALES_ORDERS,
        PurchaseOrders => internal::DATA_TRANSFER_PURCHASE_ORDERS,
        Contracts => internal::DATA_TRANSFER_CONTRACTS,
        Customers if input["scope"] == "all_authorized" => internal::DATA_TRANSFER_ALL_CUSTOMERS,
        Customers => internal::DATA_TRANSFER_CUSTOMERS,
        InventoryBalance => internal::DATA_TRANSFER_STOCK_BALANCE,
        InventoryMovement => internal::DATA_TRANSFER_STOCK_MOVEMENT,
        InventoryAdjustment => internal::DATA_TRANSFER_STOCK_ADJUSTMENT,
        InventoryReservation => internal::DATA_TRANSFER_STOCK_RESERVATION,
        SupplierOrders => internal::DATA_TRANSFER_SUPPLIER_ORDERS,
        Receivables => receivable_entry(input),
        SupplierPayables => payable_entry(input),
        InvoiceRequests | InvoiceRequestsExcel => internal::DATA_TRANSFER_INVOICE_REQUESTS,
        WorkbenchInvoiceExecutionExcel => {
            return Err(Error::ValidationError("工作台导出使用待办列表权限".into()));
        },
        ProfitLoss => internal::DATA_TRANSFER_PROFIT_LOSS,
        CustomerQualityCurrent | CustomerQualityHistory => internal::DATA_TRANSFER_CUSTOMER_QUALITY,
        MasterData => master_entry(input)?,
        InquiryPreview => internal::DATA_TRANSFER_INQUIRY_PREVIEW,
        InquiryCreationPreview => internal::DATA_TRANSFER_INQUIRY_CREATION,
        SupplyBatch => supply_batch_entry(input)?,
        PortalBatch => return Err(Error::ValidationError("门户任务使用会话写入资格".into())),
    })
}

fn receivable_entry(input: &Value) -> Permission {
    match input["view"].as_str() {
        Some("receipt") => internal::DATA_TRANSFER_RECEIPTS,
        Some("sales_invoice") => internal::DATA_TRANSFER_INVOICES,
        _ => internal::DATA_TRANSFER_RECEIVABLES,
    }
}

fn payable_entry(input: &Value) -> Permission {
    match input["view"].as_str() {
        Some("payment") => internal::DATA_TRANSFER_PAYMENTS,
        Some("allocation") => internal::DATA_TRANSFER_INVOICE_ALLOCATIONS,
        _ => internal::DATA_TRANSFER_PAYABLES,
    }
}

fn master_entry(input: &Value) -> Result<Permission> {
    match input["resource"].as_str() {
        Some("product") => Ok(internal::DATA_TRANSFER_PRODUCTS),
        Some("supplier") => Ok(internal::DATA_TRANSFER_SUPPLIERS),
        Some("sellable") => Ok(internal::DATA_TRANSFER_PRODUCT_POOL),
        _ => Err(Error::ValidationError("该目录不支持后台导出".into())),
    }
}

fn supply_batch_entry(input: &Value) -> Result<Permission> {
    match input["mode"].as_str() {
        Some("revise") => Ok(internal::DATA_TRANSFER_OFFERING_REVISE),
        Some("availability") => Ok(internal::DATA_TRANSFER_OFFERING_AVAILABILITY),
        _ => Err(Error::ValidationError("仅支持批量调价或更新可供情况，批量建档请使用商品导入".into())),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;
    use serde_json::json;

    use super::*;

    #[derive(Default)]
    struct Reader(AtomicUsize);

    #[async_trait]
    impl TransferReader for Reader {
        async fn page(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<Value> {
            panic!("附加资格读取不得切换到独立业务入口")
        }

        async fn page_with(
            &self,
            _: TransferKind,
            _: &Value,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> Result<Value> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(json!({"items":[],"total":0}))
        }

        async fn linked_invoice_request(&self, _: &str, _: &str, _: &AuditActor) -> Result<Option<Value>> {
            panic!("此查询不读取开票申请")
        }

        async fn report(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<Value> {
            panic!("商品和报价页不得切换到报表入口")
        }
    }

    #[tokio::test]
    async fn supplemental_pages_reject_non_snapshot_or_unavailable_identity_before_business_reads() {
        let authorization = unavailable_authorization();
        let reader = Reader::default();
        let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
        for (kind, query) in [
            (TransferKind::ProductPoolExcel, json!({})),
            (TransferKind::MasterData, json!({"resource":"sellable"})),
            (TransferKind::SupplierQuoteArchive, json!({"source":"products"})),
            (TransferKind::SupplierQuotePreview, json!({"sku_id":"sku"})),
        ] {
            let error =
                authorized_page(&authorization, &reader, kind, &query, &actor, &mut ReadExecutor::default())
                    .await
                    .unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(message) if message == "无对象操作及其业务读取必须使用快照事务")
            );

            let error = authorized_page(
                &authorization,
                &reader,
                kind,
                &query,
                &actor,
                &mut SnapshotExecutor::default(),
            )
            .await
            .unwrap_err();
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
        assert_eq!(reader.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn unrelated_pages_and_supplier_identity_cannot_use_supplemental_read_entry() {
        let authorization = unavailable_authorization();
        let reader = Reader::default();
        let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
        let error = authorized_page(
            &authorization,
            &reader,
            TransferKind::SalesOrders,
            &json!({}),
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message == "此任务不使用附加资格快照读取")
        );

        let supplier = AuditActor::new("supplier".into(), "login".into(), AccountKind::Supplier);
        let error = authorized_page(
            &authorization,
            &reader,
            TransferKind::ProductPoolExcel,
            &json!({}),
            &supplier,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Forbidden(message) if message == "此任务仅供管理端使用"));
        assert_eq!(reader.0.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn export_scope_pins_queue_and_range_versions_without_reusing_visible_page_focus() {
        let mut input = json!({"scope":"history", "filters":{"statuses":["COMPLETED"]}, "q":"task",
            "scope_version":"visible-range", "queue_context_id":"visible-queue", "current_work_item_id":"task"});
        reset_workbench_page(&mut input).unwrap();
        assert_eq!(input, json!({"scope":"history", "filters":{"statuses":["COMPLETED"]}, "q":"task"}));
        let first = json!({"scope_version":"export-range", "queue_context_id":"export-queue"});
        bind_scope(&mut input, &first).unwrap();
        bind_scope(&mut input, &first).unwrap();
        for changed in [
            json!({"scope_version":"export-range", "queue_context_id":"changed"}),
            json!({"scope_version":"changed", "queue_context_id":"export-queue"}),
        ] {
            assert!(matches!(bind_scope(&mut input, &changed), Err(Error::ConflictError(_))));
        }
    }

    #[test]
    fn finance_excel_reuses_finance_list_qualification_and_keeps_legacy_csv_kind() {
        assert_eq!(
            transfer_permission(TransferKind::InvoiceRequestsExcel, &json!({})).unwrap(),
            transfer_permission(TransferKind::InvoiceRequests, &json!({})).unwrap()
        );
        assert_eq!(serde_json::to_value(TransferKind::InvoiceRequests).unwrap(), "invoice_requests");
        assert!(!TransferKind::WorkbenchInvoiceExecutionExcel.allows_shared_result());
    }

    #[test]
    fn supply_batch_permissions_allow_maintenance_and_reject_creation() {
        for (mode, permission) in [
            ("revise", internal::DATA_TRANSFER_OFFERING_REVISE),
            ("availability", internal::DATA_TRANSFER_OFFERING_AVAILABILITY),
        ] {
            assert_eq!(
                transfer_permission(TransferKind::SupplyBatch, &json!({ "mode": mode })).unwrap(),
                permission
            );
        }

        for input in [json!({ "mode": "create" }), json!({ "mode": "invalid" }), json!({})] {
            assert!(matches!(
                transfer_permission(TransferKind::SupplyBatch, &input),
                Err(Error::ValidationError(_))
            ));
        }
    }
}
