//! 采购提交与变更提交行构造；请求类型化由 DTO 负责，金额规则由领域负责。

use erp_core::ids::{
    PurchaseChangeSubmissionId, PurchaseChangeSubmissionLineId, PurchaseOrderSubmissionId,
    PurchaseOrderSubmissionLineId,
};
use erp_core::money::Amount;
use id_generator::next_id;

use crate::entity::purchase_order::{
    LineAmountViolation, PurchaseChangeSubmissionLine, PurchaseLineInput, PurchaseOrderSubmissionLine,
    compute_header_totals,
};
use crate::{Error, Result};

/// 从类型化输入构建采购提交行。
///
/// Service 仅分配行 ID 与行号；金额由领域方法计算，行实体构造完成快照规范化、
/// 字段归属与金额三元组守恒校验。
///
/// # 参数
/// * `submission_id` - 所属提交稳定身份
/// * `inputs` - 类型化行输入集合
///
/// # 返回
/// 返回行号从 1 递增的提交行实体集合。
///
/// # 错误
/// 行金额输入非法或行实体校验失败时返回对应错误。
pub fn build_submission_lines(
    submission_id: &PurchaseOrderSubmissionId,
    inputs: &[PurchaseLineInput],
) -> Result<Vec<PurchaseOrderSubmissionLine>> {
    build_lines(inputs, |input, line_no| {
        let data = input
            .into_submission_line_data(submission_id.clone(), line_no)
            .map_err(map_line_amount_violation)?;
        PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new(next_id()), data)
            .map_err(Into::into)
    })
}

/// 从类型化输入构建采购变更提交行。
///
/// Service 仅分配行 ID 与行号；金额由领域方法计算，行实体构造完成快照规范化、
/// 字段归属与金额三元组守恒校验。
///
/// # 参数
/// * `submission_id` - 所属变更提交稳定身份（字符串形态）
/// * `inputs` - 类型化行输入集合
///
/// # 返回
/// 返回行号从 1 递增的变更提交行实体集合。
///
/// # 错误
/// 行金额输入非法或行实体校验失败时返回对应错误。
pub fn build_change_submission_lines(
    submission_id: &str,
    inputs: &[PurchaseLineInput],
) -> Result<Vec<PurchaseChangeSubmissionLine>> {
    build_lines(inputs, |input, line_no| {
        let data = input
            .into_change_submission_line_data(
                PurchaseChangeSubmissionId::new(submission_id.to_string()),
                line_no,
            )
            .map_err(map_line_amount_violation)?;
        PurchaseChangeSubmissionLine::new(PurchaseChangeSubmissionLineId::new(next_id()), data)
            .map_err(Into::into)
    })
}

/// 按请求顺序为类型化输入分配行号并逐行构造实体。
fn build_lines<Line>(
    inputs: &[PurchaseLineInput],
    mut make: impl FnMut(PurchaseLineInput, u32) -> Result<Line>,
) -> Result<Vec<Line>> {
    let mut result = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        result.push(make(input.clone(), (index + 1) as u32)?);
    }
    Ok(result)
}

/// 计算请求行的表头金额汇总。
///
/// # 参数
/// * `inputs` - 类型化行输入集合；空集合返回零三元组
///
/// # 返回
/// 返回 `(gross, net, tax)` 表头汇总。
///
/// # 错误
/// 任一行金额输入非法（缺数量/单价/物流金额）时返回 `ValidationError`。
pub fn compute_request_totals(inputs: &[PurchaseLineInput]) -> Result<(Amount, Amount, Amount)> {
    compute_header_totals(inputs).map_err(map_line_amount_violation)
}

/// 映射行金额领域校验失败为服务错误。
///
/// # 参数
/// * `violation` - 领域金额校验失败原因
///
/// # 返回
/// 返回保持稳定文案的参数验证错误。
///
/// # 错误
/// 无；本方法只转换错误分类。
pub fn map_line_amount_violation(violation: LineAmountViolation) -> Error {
    match violation {
        LineAmountViolation::MissingQuantity
        | LineAmountViolation::MissingUnitCostGross
        | LineAmountViolation::MissingGrossAmount => Error::ValidationError(violation.to_string()),
    }
}
