//! 按原蓝票一次开具红票并红冲分配。

use entities::payable::{PurchaseInvoiceAllocation, PurchaseInvoiceAllocationData};
use entities::receivable::{
    AllocationAction, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, RedInvoiceAllocationBasis,
    RedInvoiceAllocationLine, RedInvoiceAllocationPlan, RedInvoiceAllocationPlanError,
    RedInvoiceAllocationReversal, SalesInvoiceAllocation, SalesInvoiceAllocationData,
};
use erp_core::ids::{
    InvoiceId, PayableAccountId, PurchaseInvoiceAllocationId, ReceivableAccountId, SalesInvoiceAllocationId,
};
use erp_core::money::Amount;
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{CommitRedInvoiceRequest, InvoiceView};
use super::invoice::register_created_invoice_document;
use super::mapping::zero_amount;
use super::{invoice_task, ReceivableService};
use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

use database::{PayableExt, ReceivableExt};
use erp_audit::AuditExt;
use persistence_core::Transactional;
use std::collections::HashMap;

impl ReceivableService {
    /// 按原蓝票一次开具红票并红冲（§8.3-3 事务不变量）。
    ///
    /// 服务端在同一事务内读取原票的有效分配、计算本次反向行、创建红票、
    /// 冲减应收或应付子账进度并写审计。客户端不得提交分配 ID、净额或税额。
    /// 部分红冲时原蓝票保持已登记；全部剩余金额红冲后才置为已红冲。
    ///
    /// # 参数
    /// * `id` - 原蓝票 ID
    /// * `req` - 红票业务意图与幂等键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建红票视图。
    ///
    /// # 错误
    /// * `NotFound` - 原蓝票或有效分配不存在
    /// * `ConflictError` - 红票号码重复
    /// * `BusinessLogicError` - 红冲累计超过原分配或超额红冲
    ///
    /// # 约束
    /// 领域计划只计算金额；ID 生成、事务、写入、任务同步和审计继续由 Service 持有。
    pub async fn issue_red_invoice(
        &self,
        id: &str,
        req: CommitRedInvoiceRequest,
        actor: &AuditActor,
    ) -> Result<InvoiceView> {
        req.validate()?;
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = std::sync::Arc::clone(&self.object_read);
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let actor_id = actor.id().to_string();
        let digest = hex::encode(Sha256::digest(
            format!("{}|{}|{}", actor.id(), id, req.idempotency_key.trim()).as_bytes(),
        ));
        let red_no = req
            .invoice_no
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("HT-{}", &digest[..12]));
        let requested_amount = req.amount;
        let reason = req.reason.trim().to_string();
        let original_id = id.to_string();
        let red_invoice_id = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let original = db
                        .invoices()
                        .find_by_id(&original_id, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("原蓝票不存在".to_string()))?;
                    if !original.is_registered() || original.invoice_kind != InvoiceKind::Blue {
                        return Err(Error::BusinessLogicError(
                            "只有已登记的蓝票可以被红冲".to_string(),
                        ));
                    }
                    let allocation_plan = match original.invoice_direction {
                        InvoiceDirection::Sales => {
                            let blue = db
                                .sales_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    session,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| line.allocation_action == AllocationAction::Apply)
                                .map(|line| line.receivable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .sales_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, session)
                                .await?;
                            sales_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        }
                        InvoiceDirection::Purchase => {
                            let blue = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_invoices(
                                    &[InvoiceId::new(original.base.id.clone())],
                                    session,
                                )
                                .await?;
                            let account_ids = blue
                                .iter()
                                .filter(|line| {
                                    line.allocation_action == entities::payable::AllocationAction::Apply
                                })
                                .map(|line| line.payable_account_id.clone())
                                .collect::<Vec<_>>();
                            let related = db
                                .purchase_invoice_allocations()
                                .find_allocations_by_accounts(&account_ids, session)
                                .await?;
                            purchase_red_invoice_allocation_plan(&blue, &related, requested_amount)?
                        }
                    };
                    let (red_gross, red_net, red_tax) = allocation_plan.totals();

                    if let Some(existing) = db
                        .invoices()
                        .find_by_direction_and_normalized_no(
                            original.invoice_direction,
                            &red_no.to_uppercase(),
                            session,
                        )
                        .await?
                    {
                        if existing.invoice_kind == InvoiceKind::Red
                            && existing.original_invoice_id.as_ref()
                                == Some(&InvoiceId::new(original.base.id.clone()))
                            && existing.gross_amount == red_gross
                            && existing.net_amount == red_net
                            && existing.tax_amount == red_tax
                        {
                            return Ok::<String, crate::errors::Error>(existing.base.id);
                        }
                        return Err(Error::ConflictError("红票号码已登记，请勿重复提交".to_string()));
                    }

                    let red_invoice_id = InvoiceId::new(next_id());
                    let mut red_mut = Invoice::new(
                        red_invoice_id.clone(),
                        InvoiceData {
                            invoice_direction: original.invoice_direction,
                            invoice_kind: InvoiceKind::Red,
                            party_id: original.party_id.clone(),
                            invoice_code: original.invoice_code.clone(),
                            invoice_no: red_no.clone(),
                            invoice_date: erp_core::common::time::BusinessDate::today(),
                            gross_amount: red_gross,
                            net_amount: red_net,
                            tax_amount: red_tax,
                            rounding_adjustment_amount: zero_amount(),
                            rounding_reason: None,
                            original_invoice_id: Some(original.base.id.clone().into()),
                        },
                        &actor_id,
                    )?;
                    red_mut.mark_registered(&actor_id)?;
                    let mut original_mut = original;
                    register_created_invoice_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        &red_mut,
                        &actor_owned,
                        session,
                    )
                    .await?;
                    db.invoices().create(&red_mut, session).await?;
                    if allocation_plan.is_full_reversal() {
                        original_mut.mark_red_invoiced(&actor_id)?;
                        db.invoices().update(&mut original_mut, session).await?;
                    }

                    let mut sales_order_account_ids = Vec::new();
                    // FIN-R11：按 direction 与 account 聚合 reversal delta 后批量
                    // 条件更新与批量插入；同一 account 多行只更新一次。
                    let reversal_deltas = aggregate_reversal_deltas(allocation_plan.lines());
                    match original_mut.invoice_direction {
                        InvoiceDirection::Sales => {
                            let deltas = reversal_deltas
                                .iter()
                                .map(|(account_id, gross)| {
                                    (ReceivableAccountId::new(account_id.clone()), *gross)
                                })
                                .collect::<Vec<_>>();
                            let reverted = db
                                .receivable_accounts()
                                .revert_invoicings_many(&deltas, &actor_id, session)
                                .await?;
                            if !reverted.rejected.is_empty() {
                                return Err(Error::BusinessLogicError("红冲金额超过已开票进度".to_string()));
                            }
                            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
                            for (index, line) in allocation_plan.lines().iter().enumerate() {
                                new_allocations.push(SalesInvoiceAllocation::new(
                                    SalesInvoiceAllocationId::new(next_id()),
                                    SalesInvoiceAllocationData {
                                        invoice_id: red_invoice_id.clone(),
                                        receivable_account_id: ReceivableAccountId::new(
                                            line.account_id.clone(),
                                        ),
                                        allocation_seq: (index as u32) + 1,
                                        allocation_action: AllocationAction::Reverse,
                                        allocated_gross_amount: line.gross,
                                        allocated_net_amount: line.net,
                                        allocated_tax_amount: line.tax,
                                        reverses_allocation_id: Some(SalesInvoiceAllocationId::new(
                                            line.original_allocation_id.clone(),
                                        )),
                                    },
                                )?);
                            }
                            db.receivable()
                                .create_sales_invoice_allocations_many(&new_allocations, session)
                                .await?;
                            sales_order_account_ids
                                .extend(reversal_deltas.iter().map(|(account_id, _)| account_id.clone()));
                        }
                        InvoiceDirection::Purchase => {
                            let deltas = reversal_deltas
                                .iter()
                                .map(|(account_id, gross)| {
                                    (PayableAccountId::new(account_id.clone()), *gross)
                                })
                                .collect::<Vec<_>>();
                            let reverted = db
                                .payable_accounts()
                                .revert_invoicings_many(&deltas, &actor_id, session)
                                .await?;
                            if !reverted.rejected.is_empty() {
                                return Err(Error::BusinessLogicError("红冲金额超过已收票进度".to_string()));
                            }
                            let mut new_allocations = Vec::with_capacity(allocation_plan.lines().len());
                            for (index, line) in allocation_plan.lines().iter().enumerate() {
                                new_allocations.push(PurchaseInvoiceAllocation::new(
                                    PurchaseInvoiceAllocationId::new(next_id()),
                                    PurchaseInvoiceAllocationData {
                                        invoice_id: red_invoice_id.clone(),
                                        payable_account_id: PayableAccountId::new(line.account_id.clone()),
                                        allocation_seq: (index as u32) + 1,
                                        allocation_action: entities::payable::AllocationAction::Reverse,
                                        allocated_gross_amount: line.gross,
                                        allocated_net_amount: line.net,
                                        allocated_tax_amount: line.tax,
                                        reverses_allocation_id: Some(PurchaseInvoiceAllocationId::new(
                                            line.original_allocation_id.clone(),
                                        )),
                                    },
                                )?);
                            }
                            db.payable()
                                .create_purchase_invoice_allocations_many(&new_allocations, session)
                                .await?;
                        }
                    }
                    let audit = actor_owned.clone().resource_log_with_message(
                        "invoice.red_issue",
                        "invoice",
                        red_mut.base.id.clone(),
                        Some(reason.clone()),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    if original_mut.invoice_direction == InvoiceDirection::Sales {
                        sales_order_account_ids.sort();
                        sales_order_account_ids.dedup();
                        for account_id in &sales_order_account_ids {
                            invoice_task::sync_sales_invoice_task(
                                &db,
                                &ReceivableAccountId::new(account_id.clone()),
                                invoice_task::SalesInvoiceTaskChange::RedInvoiceIssued,
                                session,
                            )
                            .await?;
                        }
                        let mut sales_order_ids = Vec::new();
                        for account in db
                            .receivable_accounts()
                            .find_accounts_by_ids(&sales_order_account_ids, session)
                            .await?
                        {
                            sales_order_ids.push(account.sales_order_id.to_string());
                        }
                        sales_order_ids.sort();
                        sales_order_ids.dedup();
                        for sales_order_id in sales_order_ids {
                            crate::sales_order::update_sales_order_money_progress(
                                &db,
                                session,
                                &erp_core::ids::SalesOrderId::new(sales_order_id),
                                actor_id.clone(),
                                None,
                            )
                            .await?;
                        }
                    }
                    Ok::<String, crate::errors::Error>(red_invoice_id.to_string())
                })
            })
            .await?;

        self.invoice_detail(&red_invoice_id).await
    }
}

/// 将销项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换持久化事实形态，不实现或复制红冲计算规则。
fn sales_red_invoice_allocation_plan(
    blue: &[SalesInvoiceAllocation],
    related: &[SalesInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = sales_red_invoice_allocation_bases(blue);
    let reversals = sales_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Sales, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将销项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部销项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制事实字段，不扣减历史红冲或执行金额计算。
fn sales_red_invoice_allocation_bases(blue: &[SalesInvoiceAllocation]) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.receivable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将销项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应收账户下的全部相关销项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，由领域计划只匹配原分配身份。
fn sales_red_invoice_allocation_reversals(
    related: &[SalesInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 按账户聚合红票 reversal 含税增量（FIN-R11）。
///
/// 同一账户多行求和为一条 delta，保持首次出现顺序；聚合总额与计划行合计
/// 守恒。方向决策、事务与写入仍由 Service 持有，Repository 只执行返回的计划。
///
/// # 参数
/// * `lines` - 本次红票计划的反向分配行
///
/// # 返回
/// 返回按账户去重、首次出现顺序的 `(account_id, gross合计)`。
fn aggregate_reversal_deltas(lines: &[RedInvoiceAllocationLine]) -> Vec<(String, Amount)> {
    let mut order = Vec::new();
    let mut sums: HashMap<String, Amount> = HashMap::new();
    for line in lines {
        sums.entry(line.account_id.clone())
            .and_modify(|total| *total = total.checked_add(line.gross))
            .or_insert_with(|| {
                order.push(line.account_id.clone());
                line.gross
            });
    }
    order
        .into_iter()
        .map(|account_id| {
            let total = sums.remove(&account_id).expect("聚合账户必须存在");
            (account_id, total)
        })
        .collect()
}

/// 将进项分配事实适配为领域红票规划输入并构建计划。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
/// * `related` - 同一批账户下可能引用原分配的全部相关事实
/// * `requested` - 可选本次红冲含税金额
///
/// # 返回
/// 返回完成历史净额、顺序消费和比例税额舍入的领域计划。
///
/// # 错误
/// 领域规划失败时映射为与既有服务相同的业务、内部或金额错误。
///
/// # 约束
/// 本函数只转换 D19 持久化事实形态，不将进项实体依赖反向引入 D18 发票模型。
fn purchase_red_invoice_allocation_plan(
    blue: &[PurchaseInvoiceAllocation],
    related: &[PurchaseInvoiceAllocation],
    requested: Option<Amount>,
) -> Result<RedInvoiceAllocationPlan> {
    let basis = purchase_red_invoice_allocation_bases(blue);
    let reversals = purchase_red_invoice_allocation_reversals(related);
    RedInvoiceAllocationPlan::build(InvoiceDirection::Purchase, basis, &reversals, requested)
        .map_err(map_red_invoice_allocation_plan_error)
}

/// 将进项蓝票正向分配转换为领域原始基数输入。
///
/// # 参数
/// * `blue` - 原蓝票查询得到的全部进项分配事实
///
/// # 返回
/// 返回保持查询顺序的正向分配基数，非 `APPLY` 事实被忽略。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 只复制 D19 事实字段，不在 Service 内扣减历史红冲或执行金额计算。
fn purchase_red_invoice_allocation_bases(
    blue: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationBasis> {
    blue.iter()
        .filter(|line| line.allocation_action == entities::payable::AllocationAction::Apply)
        .map(|line| RedInvoiceAllocationBasis {
            original_allocation_id: line.base.id.clone(),
            account_id: line.payable_account_id.to_string(),
            allocation_seq: line.allocation_seq,
            gross: line.allocated_gross_amount,
            net: line.allocated_net_amount,
            tax: line.allocated_tax_amount,
        })
        .collect()
}

/// 将进项历史反向分配转换为领域红冲事实输入。
///
/// # 参数
/// * `related` - 同一批应付账户下的全部相关进项分配事实
///
/// # 返回
/// 返回所有携带原分配引用的 `REVERSE` 事实，保持查询顺序。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 缺少反向引用的损坏事实沿用旧逻辑忽略，且 D19 实体不会进入 D18 领域模型。
fn purchase_red_invoice_allocation_reversals(
    related: &[PurchaseInvoiceAllocation],
) -> Vec<RedInvoiceAllocationReversal> {
    related
        .iter()
        .filter(|line| line.allocation_action == entities::payable::AllocationAction::Reverse)
        .filter_map(|line| {
            line.reverses_allocation_id
                .as_ref()
                .map(|original_id| RedInvoiceAllocationReversal {
                    original_allocation_id: original_id.to_string(),
                    gross: line.allocated_gross_amount,
                    net: line.allocated_net_amount,
                    tax: line.allocated_tax_amount,
                })
        })
        .collect()
}

/// 将领域红票规划错误映射回冻结的服务错误分类和文案。
///
/// # 参数
/// * `error` - 领域计划构建失败原因
///
/// # 返回
/// 返回与迁移前相同的 `BusinessLogicError`、`Internal` 或 `Logic` 服务错误。
///
/// # 错误
/// 本函数只构造错误值，不再失败。
///
/// # 约束
/// 不解析字符串决定分类；每个领域变体显式保持既有外部错误语义。
fn map_red_invoice_allocation_plan_error(error: RedInvoiceAllocationPlanError) -> Error {
    match error {
        error @ (RedInvoiceAllocationPlanError::SalesHistoricalOverReversal
        | RedInvoiceAllocationPlanError::PurchaseHistoricalOverReversal
        | RedInvoiceAllocationPlanError::NoRemainingAllocation
        | RedInvoiceAllocationPlanError::InvalidRequestedAmount) => {
            Error::BusinessLogicError(error.to_string())
        }
        RedInvoiceAllocationPlanError::UncoveredRequest => {
            Error::Internal("红票反向分配计划未覆盖请求金额".to_string())
        }
        RedInvoiceAllocationPlanError::InvalidAmount(error) => Error::Logic(error),
    }
}

#[cfg(test)]
mod red_invoice_reversal_tests {
    use super::aggregate_reversal_deltas;
    use entities::receivable::RedInvoiceAllocationLine;
    use erp_core::money::Amount;
    use std::str::FromStr;

    fn line(account: &str, gross: &str) -> RedInvoiceAllocationLine {
        RedInvoiceAllocationLine {
            original_allocation_id: format!("base-{account}-{gross}"),
            account_id: account.to_string(),
            gross: Amount::from_str(gross).unwrap(),
            net: Amount::from_str("0").unwrap(),
            tax: Amount::from_str("0").unwrap(),
        }
    }

    /// 同账户多行聚合为一条并保持首次出现顺序，总额守恒。
    #[test]
    fn same_account_lines_aggregate_with_stable_order_and_conservation() {
        let lines = [
            line("acc-a", "100.00"),
            line("acc-b", "50.00"),
            line("acc-a", "30.00"),
        ];
        let deltas = aggregate_reversal_deltas(&lines);
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].0, "acc-a");
        assert_eq!(deltas[0].1, Amount::from_str("130.00").unwrap());
        assert_eq!(deltas[1].0, "acc-b");
        assert_eq!(deltas[1].1, Amount::from_str("50.00").unwrap());
        let plan_total: Amount = lines.iter().fold(Amount::from_str("0").unwrap(), |sum, line| {
            sum.checked_add(line.gross)
        });
        let delta_total = deltas
            .iter()
            .fold(Amount::from_str("0").unwrap(), |sum, (_, gross)| {
                sum.checked_add(*gross)
            });
        assert_eq!(plan_total, delta_total);
    }

    /// 空计划聚合为空，不触发写入。
    #[test]
    fn empty_plan_aggregates_to_empty() {
        assert!(aggregate_reversal_deltas(&[]).is_empty());
    }
}
