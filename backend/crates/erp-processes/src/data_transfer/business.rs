//! 文件任务的查询、权限和格式编排；查询适配器由应用组合根装配。
use std::collections::HashSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use erp_identity::{AccessControlExt, Permission, SharedRbacService, subject};
use mongodb::Database;
use persistence_core::NoTransaction;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use storage::S3Storage;

use super::csv::render as render_csv;
use super::master::rows as master_rows;
use super::{TransferContext, TransferHandler, TransferKind, TransferResult};
use crate::{Error, Result};

/// 组合根将固定业务查询接回原有 Service/ReadModel，不允许自由查询数据库或 URL。
#[async_trait]
pub trait TransferReader: Send + Sync {
    /// 读取与业务页面同口径的一页。
    /// # 参数
    /// 固定任务类型、服务端分页参数与认证人。
    /// # 返回
    /// 原接口 JSON 分页视图。
    /// # 错误
    /// 原业务权限、版本和查询错误原样保留。
    async fn page(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value>;
    /// 读取原报表的完整 CSV。
    /// # 参数
    /// 固定报表类型、条件与认证人。
    /// # 返回
    /// 原报表导出结果。
    /// # 错误
    /// 原报表错误原样保留。
    async fn report(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<Value>;
}

/// 默认导入导出业务实现。
pub struct BusinessTransfers {
    pub(super) db: Database,
    pub(super) rbac: SharedRbacService,
    pub(super) reader: Arc<dyn TransferReader>,
    pub(super) storage: S3Storage,
}
impl BusinessTransfers {
    /// 装配当前身份源及固定业务查询适配器。
    /// # 参数
    /// 数据库、RBAC、原业务查询适配器。
    /// # 返回
    /// 未执行 I/O 的任务处理器。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        reader: Arc<dyn TransferReader>,
        storage: S3Storage,
    ) -> Self {
        Self { db, rbac, reader, storage }
    }
    pub(super) async fn access(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<()> {
        if actor.kind() == AccountKind::Supplier && kind != TransferKind::PortalBatch {
            return Err(Error::Forbidden("此任务仅供管理端使用".into()));
        }
        self.db
            .accounts()
            .find_by_id(actor.id(), &mut NoTransaction)
            .await?
            .filter(|account| account.kind == actor.kind() && account.can_login())
            .ok_or_else(|| Error::Forbidden("操作账号已停用或身份变化".into()))?;
        for code in permissions(kind, input)? {
            if !self.rbac.enforce(&subject(actor.kind(), actor.id()), &Permission::parse(code)?).await? {
                return Err(Error::Forbidden(format!("当前账号无权执行此任务（{code}）")));
            }
        }
        Ok(())
    }
    pub(super) async fn collect(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
    ) -> Result<Vec<Value>> {
        let mut query = input.clone();
        let mut rows = Vec::new();
        let mut identities = HashSet::new();
        let mut expected = None;
        for page in 1..=1000 {
            query["page"] = json!(page);
            query["page_size"] = json!(100);
            self.access(kind, &query, actor).await?;
            let response = self.reader.page(kind, &query, actor).await?;
            if response["empty_reason"] == "no_scope" {
                return Err(Error::Forbidden("当前无可导出范围".into()));
            }
            let total =
                response["total"].as_u64().ok_or_else(|| Error::Internal("导出列表缺少总数".into()))?;
            if total > 100_000 || expected.is_some_and(|value| value != total) {
                return Err(Error::ConflictError("结果已变化或超过十万行，请缩小范围".into()));
            }
            expected = Some(total);
            let items =
                response["items"].as_array().ok_or_else(|| Error::Internal("导出列表格式错误".into()))?;
            if let Some(version) = response.get("scope_version").filter(|v| !v.is_null()) {
                if query.get("scope_version").is_some_and(|old| !old.is_null() && old != version) {
                    return Err(Error::ConflictError("导出范围已变化".into()));
                }
                query["scope_version"] = version.clone();
            }
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
                return Ok(rows);
            }
            if items.is_empty() {
                return Err(Error::ConflictError("导出结果不完整".into()));
            }
        }
        Err(Error::ValidationError("导出超过十万行，请缩小范围".into()))
    }
    async fn check_final_scope(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
        expected: Option<u64>,
    ) -> Result<()> {
        let last = self.reader.page(kind, query, actor).await?;
        if last["total"].as_u64() != expected
            || last["empty_reason"] == "no_scope"
            || last
                .get("scope_version")
                .is_some_and(|v| !v.is_null() && Some(v) != query.get("scope_version"))
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
#[async_trait]
impl TransferHandler for BusinessTransfers {
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
        if !kind.is_export() {
            return self.prepare_import(kind, input, actor).await;
        }
        if !is_report(kind)
            && !matches!(kind, TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive)
        {
            input["page"] = json!(1);
            input["page_size"] = json!(100);
            let first = self.reader.page(kind, &input, actor).await?;
            if first["empty_reason"] == "no_scope" {
                return Err(Error::Forbidden("当前无可导出范围".into()));
            }
            if let Some(version) = first.get("scope_version") {
                input["scope_version"] = version.clone();
            }
        }
        Ok(input)
    }
    async fn execute(&self, kind: TransferKind, input: &Value, actor: &AuditActor) -> Result<TransferResult> {
        self.access(kind, input, actor).await?;
        match kind {
            TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive => {
                self.workbook(kind, input, actor).await
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
        let fresh = if matches!(kind, TransferKind::ProductPoolExcel | TransferKind::SupplierQuoteArchive) {
            self.workbook_authorization(kind, input, actor).await?
        } else {
            self.csv(kind, input, actor).await?.authorization
        };
        if fresh != *authorization {
            return Err(Error::ConflictError("数据或授权范围已变化，请重新导出".into()));
        }
        Ok(())
    }
}
pub(super) fn is_report(kind: TransferKind) -> bool {
    matches!(
        kind,
        TransferKind::ProfitLoss
            | TransferKind::CustomerQualityCurrent
            | TransferKind::CustomerQualityHistory
    )
}
fn permissions(kind: TransferKind, input: &Value) -> Result<Vec<&'static str>> {
    use TransferKind::*;
    Ok(match kind {
        ProductPoolExcel => vec!["sellable_sku:list"],
        SupplierQuoteArchive | SupplierQuotePreview => {
            vec!["procurement_inquiry:internal_quote", "sellable_sku:list", "supplier_offering:list"]
        },
        SalesOrders => vec!["sales_order:list"],
        PurchaseOrders => vec!["purchase_order:list"],
        Contracts => vec!["contract:list"],
        Customers => {
            if input["scope"] == "all_authorized" {
                vec!["customer:list", "customer_scope:detail"]
            } else {
                vec!["customer:list"]
            }
        },
        InventoryBalance => vec!["stock_balance:list"],
        InventoryMovement => vec!["stock_movement:list"],
        InventoryAdjustment => vec!["stock_adjustment:list"],
        InventoryReservation => vec!["stock_reservation:list"],
        SupplierOrders => vec!["supplier_fulfillment_order:list"],
        Receivables => vec![match input["view"].as_str() {
            Some("receipt") => "customer_receipt:list",
            Some("sales_invoice") => "invoice:list",
            _ => "receivable_account:list",
        }],
        SupplierPayables => vec![match input["view"].as_str() {
            Some("payment") => "supplier_payment:list",
            Some("allocation") => "purchase_invoice_allocation:list",
            _ => "payable_account:list",
        }],
        InvoiceRequests => vec!["sales_invoice_request:list"],
        ProfitLoss => vec!["cost_entry:list", "sales_order:list"],
        CustomerQualityCurrent | CustomerQualityHistory => vec!["customer:list", "sales_order:list"],
        MasterData => vec![match input["resource"].as_str() {
            Some("product") => "product:list",
            Some("supplier") => "supplier:list",
            Some("sellable") => "sellable_sku:list",
            _ => return Err(Error::ValidationError("该目录不支持后台导出".into())),
        }],
        InquiryPreview => vec!["sales_inquiry:import"],
        InquiryCreationPreview => vec!["sales_inquiry:create", "sales_inquiry:import", "sellable_sku:list"],
        SupplyBatch => vec![match input["mode"].as_str() {
            Some("create") => "supplier_offering:create",
            Some("revise") => "supplier_offering:update",
            Some("availability") => "supplier_offering_availability:update",
            _ => return Err(Error::ValidationError("供给批量模式无效".into())),
        }],
        PortalBatch => vec![],
    })
}
