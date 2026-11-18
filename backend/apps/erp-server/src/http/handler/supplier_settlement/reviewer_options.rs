//! 结算复核候选查询的协议适配，复用提交权限。
use erp_supply_settlement::supply_settlement::SettlementReviewerOption;

use super::*;

/// 在为结算单选择复核人前列出可处理该单的人选；流程核验当前经办身份与候选资格。
pub async fn supplier_settlement_reviewer_options(
    State(state): State<HttpState>,
    Extension(actor): Extension<AuditActor>,
    Path(id): Path<String>,
) -> Result<Vec<SettlementReviewerOption>> {
    let options = settlement_process(&state).reviewer_options(&id, &actor).await?;
    Ok(ApiResponse::ok_with_data(options))
}
