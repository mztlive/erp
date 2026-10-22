use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::Error as ContractError;
use erp_identity::{Permission, subject};
use erp_processes::adapters::{
    MongoCustomerDataScope, MongoOfferingDataScope, MongoPurchaseDataScope, funds_access_with_rbac,
    purchase_access,
};
use erp_processes::data_transfer::{TransferKind, TransferReader};
use erp_processes::{Error, Result};
use erp_read_models::customer_quality::CustomerQualityReadModel;
use erp_read_models::finance::actual_profit_loss::{ActualProfitLossReadModel, ProfitLossAccess};
use erp_read_models::purchase_center::PurchaseOrderReadService;
use erp_read_models::sales_center::order::SalesOrderReadService;
use erp_read_models::supplier_center::{MongoOfferingProcurementOwners, SupplierOfferingReadService};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::app_state::AppState;

pub(super) struct Reads(pub AppState);
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
    async fn page(&self, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
        use TransferKind::*;
        let state = &self.0;
        match kind {
            SalesOrders => sales(state, query, actor).await,
            PurchaseOrders => output(
                PurchaseOrderReadService::with_scope(
                    state.db(),
                    MongoPurchaseDataScope::shared(state.db(), state.rbac()),
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
                    .supplier_fulfillment_process()
                    .supplier_fulfillment_order_list(&input(query)?, actor)
                    .await?,
            ),
            InvoiceRequests => output(
                funds_access_with_rbac(state.db(), state.rbac())
                    .request_list_scoped(&input(query)?, actor)
                    .await?,
            ),
            Receivables | SupplierPayables => funds(state, kind, query, actor).await,
            SupplierQuoteArchive | SupplierQuotePreview => quotes(state, query, actor).await,
            ProductPoolExcel => output(state.catalog_center().sellable_sku_list(&input(query)?).await?),
            MasterData => master(state, query, actor).await,
            _ => Err(Error::ValidationError("此任务不是列表查询".into())),
        }
    }
    async fn report(&self, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
        let state = &self.0;
        match kind {
            TransferKind::ProfitLoss => {
                let can_drill_cost = state
                    .rbac()
                    .enforce(&subject(actor.kind(), actor.id()), &Permission::parse("cost_entry:detail")?)
                    .await?;
                output(
                    ActualProfitLossReadModel::new(state.db(), state.rbac())
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
fn quality(state: &AppState) -> CustomerQualityReadModel {
    CustomerQualityReadModel::new(
        state.db(),
        state.rbac(),
        MongoCustomerDataScope::shared(state.db(), state.rbac()),
    )
}
async fn master(state: &AppState, query: &Value, actor: &AuditActor) -> Result<Value> {
    match query["resource"].as_str() {
        Some("supplier") => output(state.supplier_service().supplier_list(&input(query)?, actor).await?),
        Some("product") => output(state.catalog_center().product_list(&input(query)?, actor).await?),
        Some("sellable") => output(state.catalog_center().sellable_sku_list(&input(query)?).await?),
        _ => Err(Error::ValidationError("未知的主数据目录".into())),
    }
}
async fn funds(state: &AppState, kind: TransferKind, query: &Value, actor: &AuditActor) -> Result<Value> {
    let service = funds_access_with_rbac(state.db(), state.rbac());
    let purchase = purchase_access(state.db(), state.rbac());
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
            output(service.invoice_list_scoped(&input(query)?, actor, &purchase).await?)
        },
        (TransferKind::SupplierPayables, Some("payable")) => {
            output(service.payable_account_list_scoped(&input(query)?, actor, &purchase).await?)
        },
        (TransferKind::SupplierPayables, Some("payment")) => {
            output(service.supplier_payment_list_scoped(&input(query)?, actor, &purchase).await?)
        },
        (TransferKind::SupplierPayables, Some("allocation")) => {
            output(service.purchase_invoice_allocation_list_scoped(&input(query)?, actor, &purchase).await?)
        },
        _ => Err(Error::ValidationError("未知的往来视图".into())),
    }
}

async fn quotes(state: &AppState, query: &Value, actor: &AuditActor) -> Result<Value> {
    if query["source"] == "products" {
        output(state.catalog_center().sellable_sku_list(&input(query)?).await?)
    } else {
        let mut page = SupplierOfferingReadService::new(
            state.db(),
            MongoOfferingDataScope::shared(state.db(), state.rbac()),
            MongoOfferingProcurementOwners::shared(state.db()),
        )
        .list(&input(query)?, actor)
        .await?;
        for item in &mut page.data.page.items {
            item.redact_costs();
        }
        output(page)
    }
}

async fn sales(state: &AppState, query: &Value, actor: &AuditActor) -> Result<Value> {
    let mut page = output(
        SalesOrderReadService::with_rbac(state.db(), state.rbac())
            .sales_order_list(&input(query)?, actor)
            .await?,
    )?;
    let can_read = state
        .rbac()
        .enforce(&subject(actor.kind(), actor.id()), &Permission::parse("contract:detail")?)
        .await?;
    if let Some(rows) = page["items"].as_array_mut() {
        for row in rows {
            let Some(id) = row["contract_id"].as_str().filter(|s| !s.is_empty()) else {
                continue;
            };
            let number = if can_read {
                match state.contract_service().contract_detail(id, actor).await {
                    Ok(detail) => detail.contract_no,
                    Err(ContractError::Forbidden(_)) | Err(ContractError::NotFound(_)) => {
                        "合同资料不可读取".into()
                    },
                    Err(error) => return Err(error.into()),
                }
            } else {
                "合同资料不可读取".into()
            };
            row["contract_no"] = Value::String(number);
        }
    }
    Ok(page)
}
