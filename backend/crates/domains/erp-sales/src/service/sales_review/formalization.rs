// ---------------------------------------------------------------------------
// 聚合构造与校验（纯内存，不依赖仓储）
// ---------------------------------------------------------------------------

use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::money::Amount;

use crate::entity::sales_order::{
    FormalRevisionContext, RevisionSource, SalesOrder, SalesOrderRevisionAggregate,
};
use crate::entity::sales_review::{SalesChangeSubmission, SalesChangeSubmissionLine};
use crate::service::sales_order::formalize::allocate_formal_revision_identities;
use crate::{Error, Result};

/// 销售版本聚合载体（版本头 + 公共行 + 子类型行）。
pub(super) type RevisionAggregate = SalesOrderRevisionAggregate;

/// 汇总版本聚合的含税金额（取公共行合计；与版本头金额一致由实体保证）。
///
/// # 参数
/// * `revision` - 版本聚合
///
/// # 返回
/// 返回公共行含税合计；没有公共行时返回零。
///
/// # 错误
/// 函数体不产生 `Err`。
///
/// # Panics
/// `Amount::from_str("0.00")` 失败时 panic；该字面量应为合法静态零值。
pub(super) fn revision_gross(revision: &RevisionAggregate) -> Result<Amount> {
    let zero = Amount::from_str("0.00").expect("静态零值必须合法");
    Ok(revision.lines.iter().fold(zero, |acc, line| acc.checked_add(line.gross_amount)))
}

/// 从变更提交构建正式版本聚合（§8.1.3 变更生效）。
///
/// # 参数
/// * `order` - 销售单
/// * `submission` - 变更提交
/// * `submission_lines` - 变更提交行
/// * `revision_no` - 目标版本号（既有最大版本号 + 1）
/// * `effective_at` - 生效时间
///
/// # 返回
/// 返回版本聚合。
///
/// # 错误
/// 行字段组缺失或版本字段校验失败时返回错误。
pub(super) fn build_change_revision(
    order: &SalesOrder,
    submission: &SalesChangeSubmission,
    submission_lines: &[SalesChangeSubmissionLine],
    revision_no: u32,
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    SalesOrderRevisionAggregate::from_sales_change_submission(
        allocate_formal_revision_identities(submission_lines.iter().map(|line| line.line_type)),
        FormalRevisionContext::new(
            revision_no,
            RevisionSource::SalesChange,
            order.stable.current_revision_id.clone().map(Into::into),
            order.business_type,
            effective_at,
        ),
        submission,
        submission_lines,
    )
    .map_err(Error::Logic)
}
