//! 结算复核候选查询的协议适配，复用提交权限。
use erp_supply_settlement::supply_settlement::SettlementReviewerOption;

use super::*;

/// 返回可处理本单的复核人员；经办身份和候选资格由流程层验证。
pub async fn supplier_settlement_reviewer_options(
    State(state): State<AppState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SettlementReviewerOption>> {
    let options = settlement_process(&state).reviewer_options(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(options))
}
