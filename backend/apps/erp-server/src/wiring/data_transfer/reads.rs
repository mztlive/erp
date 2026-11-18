use application_core::AuditActor;
use async_trait::async_trait;
use erp_data_import::data_transfer::{TransferKind, TransferReader};
use erp_permissions::cost;
use erp_procurement::PurchaseDataScope;
use erp_supply::OfferingAccess;
use persistence_core::{Executor, NoTransaction};

use crate::wiring::{customer_access, funds_access};
mod error;
use erp_read_models::customer_quality::CustomerQualityReadModel;
use erp_read_models::finance::actual_profit_loss::{ActualProfitLossReadModel, ProfitLossAccess};
use erp_read_models::purchase_center::PurchaseOrderReadService;
use erp_read_models::sales_center::order::SalesOrderReadService;
use erp_read_models::supplier_center::{MongoOfferingProcurementOwners, SupplierOfferingReadService};
use error::{ReadError as Error, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::wiring::Application;

pub(crate) struct Reads(pub Application);
fn input<T: DeserializeOwned>(value: &Value) -> Result<T> {
    serde_json::from_value({
        let mut value = value.clone();
        if let Some(map) = value.as_object_mut() {
            for key in ["source", "resource", "_filters"] {
                map.remove(key);
            }
        }
        value
    })
    .map_err(|_| Error::ValidationError("任务查询条件格式无效".into()))
}
fn output<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::Internal("任务查询结果无法编码".into()))
}
#[async_trait]
impl TransferReader for Reads {
    async fn page(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
    ) -> erp_data_import::Result<Value> {
        self.read_page(kind, query, actor).await.map_err(Into::into)
    }
    async fn page_with(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_data_import::Result<Value> {
        self.read_page_with(kind, query, actor, executor).await.map_err(Into::into)
    }
    async fn report(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
    ) -> erp_data_import::Result<Value> {
        self.read_report(kind, query, actor).await.map_err(Into::into)
    }
}
impl Reads {
    async fn read_page(&self, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
        use TransferKind::*;
        let state = &self.0;
        match kind {
            SalesOrders => sales(state, query, actor).await,
            PurchaseOrders => output(
                PurchaseOrderReadService::new(
                    state.db(),
                    state.authorization(),
                    PurchaseDataScope::shared(state.db(), state.authorization()),
                )
                .purchase_order_list(&input(query)?, actor)
                .await?,
            ),
            Contracts => output(state.contract_service().contract_list(&input(query)?, actor).await?),
            Customers => output(state.customer_service().customer_list(&input(query)?, actor).await?),
            InventoryBalance => {
                output(state.inventory_service().stock_balance_list(&input(query)?, actor).await?)
            },
            InventoryMovement => {
                output(state.inventory_service().stock_movement_list(&input(query)?, actor).await?)
            },
            InventoryAdjustment => {
                output(state.inventory_service().stock_adjustment_list(&input(query)?, actor).await?)
            },
            InventoryReservation => {
                output(state.inventory_service().stock_reservation_list(&input(query)?, actor).await?)
            },
            SupplierOrders => output(
                state
                    .supplier_fulfillment_list_read_service()
                    .supplier_fulfillment_order_list(&input(query)?, actor)
                    .await?,
            ),
            InvoiceRequests => output(
                funds_access(state.db(), state.authorization())
                    .request_list_scoped(&input(query)?, actor)
                    .await?,
            ),
            Receivables | SupplierPayables => funds(state, kind, query, actor).await,
            SupplierQuoteArchive | SupplierQuotePreview | ProductPoolExcel => {
                Err(Error::ValidationError("此任务必须沿原功能授权快照读取".into()))
            },
            MasterData => master(state, query, actor).await,
            _ => Err(Error::ValidationError("此任务不是列表查询".into())),
        }
    }
    async fn read_report(&self, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
        let state = &self.0;
        match kind {
            TransferKind::ProfitLoss => {
                let can_drill_cost = state
                    .authorization()
                    .has_permission(actor, &cost::ENTRY_DETAIL, &mut NoTransaction)
                    .await?;
                output(
                    ActualProfitLossReadModel::new(state.db(), state.authorization())
                        .export(input(query)?, ProfitLossAccess { can_drill_cost }, actor)
                        .await?,
                )
            },
            TransferKind::CustomerQualityCurrent => {
                output(quality(state).export_current(input(query)?, actor).await?)
            },
            TransferKind::CustomerQualityHistory => {
                output(quality(state).export_history(input(query)?, actor).await?)
            },
            _ => Err(Error::ValidationError("此任务不是经营报表".into())),
        }
    }
}
fn quality(state: &Application) -> CustomerQualityReadModel {
    CustomerQualityReadModel::new(
        state.db(),
        state.authorization(),
        customer_access(state.db(), state.authorization()),
    )
}
async fn master(state: &Application, query: &Value, actor: &AuditActor) -> Result<Value> {
    match query["resource"].as_str() {
        Some("supplier") => output(state.supplier_service().supplier_list(&input(query)?, actor).await?),
        Some("product") => output(state.catalog_center().product_list(&input(query)?, actor).await?),
        Some("sellable") => Err(Error::ValidationError("商品池任务必须沿原功能授权快照读取".into())),
        _ => Err(Error::ValidationError("未知的主数据目录".into())),
    }
}
async fn funds(state: &Application, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
    let service = funds_access(state.db(), state.authorization());
    let view = query["view"].as_str();
    let mut filtered = query.clone();
    if let Some(map) = filtered.as_object_mut() {
        map.remove("view");
    }
    let query = &filtered;
    match (kind, view) {
        (TransferKind::Receivables, Some("receivable")) => {
            output(service.receivable_account_list_scoped(&input(query)?, actor).await?)
        },
        (TransferKind::Receivables, Some("receipt")) => {
            output(service.customer_receipt_list_scoped(&input(query)?, actor).await?)
        },
        (TransferKind::Receivables, Some("sales_invoice")) => {
            output(service.invoice_list_scoped(&input(query)?, actor).await?)
        },
        (TransferKind::SupplierPayables, Some("payable")) => {
            output(service.payable_account_list_scoped(&input(query)?, actor).await?)
        },
        (TransferKind::SupplierPayables, Some("payment")) => {
            output(service.supplier_payment_list_scoped(&input(query)?, actor).await?)
        },
        (TransferKind::SupplierPayables, Some("allocation")) => {
            output(service.purchase_invoice_allocation_list_scoped(&input(query)?, actor).await?)
        },
        _ => Err(Error::ValidationError("未知的往来视图".into())),
    }
}

impl Reads {
    // 为商品池和报价任务导出沿原功能资格快照分页，继续执行对应业务数据范围校验。
    async fn read_page_with(
        &self,
        kind: TransferKind,
        query: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        match kind {
            TransferKind::ProductPoolExcel => self.product_page(query, actor, executor).await,
            TransferKind::MasterData if query["resource"] == "sellable" => {
                self.product_page(query, actor, executor).await
            },
            TransferKind::SupplierQuoteArchive | TransferKind::SupplierQuotePreview => {
                if query["source"] == "products" {
                    self.product_page(query, actor, executor).await
                } else {
                    self.quote_page(query, actor, executor).await
                }
            },
            _ => Err(Error::ValidationError("此任务不支持原功能授权快照分页".into())),
        }
    }

    // 导出商品页时在同一次业务读取中检查商品资格并取得分页、总数及供应商编号。
    async fn product_page(
        &self,
        query: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        output(self.0.catalog_center().sellable_sku_list_with(&input(query)?, actor, executor).await?)
    }

    // 导出报价时在原授权快照读取相关供给，保持成本字段按既有资格脱敏。
    async fn quote_page(
        &self,
        query: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Value> {
        let state = &self.0;
        let mut page = SupplierOfferingReadService::new(
            state.db(),
            OfferingAccess::new(state.db(), state.authorization()),
            MongoOfferingProcurementOwners::shared(state.db()),
        )
        .list_with(&input(query)?, actor, executor)
        .await?;
        for item in &mut page.data.page.items {
            item.redact_costs();
        }
        output(page)
    }
}

async fn sales(state: &Application, query: &Value, actor: &AuditActor) -> Result<Value> {
    output(
        SalesOrderReadService::new(state.db(), state.authorization())
            .sales_order_export_page(&input(query)?, actor)
            .await?,
    )
}
