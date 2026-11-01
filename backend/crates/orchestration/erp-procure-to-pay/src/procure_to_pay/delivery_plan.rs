//! 采购草稿保存、提交与采购变更冻结和生效时复验交付安排，沿用销售 guard 与调用方执行器。
use std::collections::HashSet;

use erp_core::ids::SalesOrderRevisionId;
use erp_procurement::entity::purchase_order::{
    DeliveryEdit, DeliveryFields, ProcurementCoverageFacts, PurchaseLineInput, PurchaseOrder,
    PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine, RowCheck, delivery_edits,
};
use erp_read_models::purchase_center::repository::load_procurement_coverage_facts;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 用销售当前版本和其他有效采购安排复验本单交付调整。
/// # 参数
/// `db`、`executor` 沿用调用方读取边界；`order` 为采购单，`edits` 为待复验的交付调整。
/// # 返回
/// 预计交付日、当前地址、单位精度和地址剩余量均合法时成功。
/// # 错误
/// 来源失效、交付日晚于履约期限、地址被修改或其他采购已占用目标数量时拒绝。
pub(super) async fn validate_delivery(
    db: &Database,
    order: &PurchaseOrder,
    edits: &[DeliveryEdit<'_>],
    executor: &mut dyn Executor,
) -> Result<()> {
    if edits.is_empty() {
        return Ok(());
    }
    let sales = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    let revision_id =
        sales.stable.current_revision_id.ok_or_else(|| Error::ConflictError("销售单缺少生效版本".into()))?;
    let facts = load_procurement_coverage_facts(
        db,
        &SalesOrderRevisionId::new(revision_id),
        &order.sales_order_id,
        executor,
    )
    .await?;
    validate_edits(order, edits, &facts)
}

/// 收件分配引用的销售行必须仍在当前版本，再按领域规则复验。
fn validate_edits(
    order: &PurchaseOrder,
    edits: &[DeliveryEdit<'_>],
    facts: &ProcurementCoverageFacts,
) -> Result<()> {
    let removed = edits.iter().any(|edit| {
        edit.delivery_allocations.is_some_and(|rows| !rows.is_empty())
            && facts.current_goods_line(edit.sales_order_line_id).is_none()
    });
    if removed {
        return Err(Error::ConflictError("销售商品行已变化，请重新核对交付安排".into()));
    }
    Ok(facts.validate_delivery_edits(order, edits)?)
}

/// 复验采购版本行的全部收件分配，用于采购变更生效前的分配重建。
/// # 参数
/// `db`、`executor` 为调用方执行器；`order` 为采购单；`lines` 为待生效的版本行。
/// # 返回
/// 全部收件分配通过复验时成功。
/// # 错误
/// 地址、数量、来源或数据读取失败时拒绝。
pub(super) async fn validate_revision_delivery(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let fields = lines.iter().filter_map(DeliveryFields::of_revision).collect::<Vec<_>>();
    validate_delivery(db, order, &delivery_edits(&fields, &[], RowCheck::All), executor).await
}

/// 采购改单目标转为待复验交付调整：收件分配全部复验，预计交付日复验改动行和因
/// 履约期限提前受影响的行；已登记维持的旧交期不阻断其他调整。
///
/// # 参数
/// * `db`、`executor` - 调用方执行器
/// * `order` - 来源采购单
/// * `inputs` - 本次改单目标行
/// * `base` - 采购当前生效版本行
/// * `recheck` - 须复验预计交付日的稳定销售行
///
/// # 返回
/// 完整通过交付来源校验时成功。
///
/// # 错误
/// 交付日、地址、数量、来源或数据读取失败时拒绝。
pub(super) async fn validate_change_input_delivery(
    db: &Database,
    order: &PurchaseOrder,
    inputs: &[PurchaseLineInput],
    base: &[PurchaseOrderRevisionLine],
    recheck: &HashSet<String>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let fields = inputs.iter().filter_map(DeliveryFields::of_input).collect::<Vec<_>>();
    validate_change_fields(db, order, &fields, base, recheck, executor).await
}

/// 采购改单生效时按新版本行复验交付调整，规则同 [`validate_change_input_delivery`]。
///
/// # 参数
/// * `db`、`executor` - 生效事务执行器
/// * `order` - 尚未切换版本的来源采购单
/// * `lines` - 即将写入的新版本行
/// * `base` - 采购当前生效版本行
/// * `recheck` - 须复验预计交付日的稳定销售行
///
/// # 返回
/// 完整通过交付来源校验时成功。
///
/// # 错误
/// 交付日、地址、数量、来源或数据读取失败时拒绝。
pub(super) async fn validate_change_revision_delivery(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderRevisionLine],
    base: &[PurchaseOrderRevisionLine],
    recheck: &HashSet<String>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let fields = lines.iter().filter_map(DeliveryFields::of_revision).collect::<Vec<_>>();
    validate_change_fields(db, order, &fields, base, recheck, executor).await
}

/// 以基准版本为对照复验改单交付调整；须复验的行不进入对照，视为改动。
async fn validate_change_fields(
    db: &Database,
    order: &PurchaseOrder,
    fields: &[DeliveryFields<'_>],
    base: &[PurchaseOrderRevisionLine],
    recheck: &HashSet<String>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let previous = base
        .iter()
        .filter_map(DeliveryFields::of_revision)
        .filter(|line| !recheck.contains(line.sales_order_line_id))
        .collect::<Vec<_>>();
    validate_delivery(db, order, &delivery_edits(fields, &previous, RowCheck::All), executor).await
}

/// 采购草稿或提交行相对原草稿的交付调整复验。
/// # 参数
/// `db`、`executor` 为调用方执行器；`order` 为采购单，其当前提交仍指向被替换的原草稿；
/// `lines` 为待写入的完整行；`previous` 为原草稿行。
/// # 返回
/// 改动的预计交付日与收件分配通过复验时成功；未改动的部分不复验。
/// # 错误
/// 交付日晚于履约期限、地址不在销售当前收件资料中、超过地址剩余量、精度非法或数据读取失败时拒绝。
pub(super) async fn validate_submission_delivery(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    previous: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let fields = lines.iter().filter_map(DeliveryFields::of_submission).collect::<Vec<_>>();
    let before = previous.iter().filter_map(DeliveryFields::of_submission).collect::<Vec<_>>();
    validate_delivery(db, order, &delivery_edits(&fields, &before, RowCheck::Changed), executor).await
}
