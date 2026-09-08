//! 结算复核候选查询的协议适配，复用提交权限。
use super::*;
use erp_processes::supply_settlement::SettlementReviewerOption;

#[permission_macros::permission(
    group = "供应商结算",
    group_desc = "供应商周期结算单、明细与差异管理",
    desc = "提交供应商结算复核",
    resource = "supplier_settlement_statement",
    action = "submit"
)]
/// 返回可处理本单的复核人员；经办身份和候选资格由流程层验证。
pub async fn supplier_settlement_reviewer_options(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SettlementReviewerOption>> {
    let options = SupplierSettlementProcess::new(state.db())
        .reviewer_options(&id, &actor)
        .await?;
    Ok(ApiResponse::ok_with_data(options))
}
