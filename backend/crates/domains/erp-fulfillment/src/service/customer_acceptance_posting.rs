//! 客户验收单域规则、事务内事实写入和履约投影读取；根事务与销售更新由流程持有。
use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_core::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, SalesOrderId,
    SalesOrderLineId,
};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;

use crate::dto::{
    AcceptanceAllocationInput, AcceptanceLineInput, CommitCustomerAcceptanceRequest, PostAcceptanceLineInput,
    PostCustomerAcceptanceRequest, ReverseCustomerAcceptanceRequest,
};
use crate::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceResult, AllocationAction,
    CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine, CustomerAcceptanceLineBatch,
    CustomerAcceptanceLineData, CustomerAcceptanceLineSpec, CustomerAcceptanceUpdate, FulfillmentFactType,
};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
impl super::FulfillmentService {
    /// 先校验工作台任务 ID 与期望任务版本是否同时提供或同时省略，避免把只携带一项的请求当作无任务登记。
    /// 两项缺一时立即返回 `ValidationError`，保留正式命令的校验顺序。
    pub fn validate_customer_acceptance_task_context(
        work_item_id: Option<&str>,
        expected_task_version: Option<u64>,
    ) -> Result<()> {
        if work_item_id.is_some() == expected_task_version.is_some() {
            return Ok(());
        }
        Err(Error::ValidationError("客户验收任务主键和期望版本必须同时提供".to_string()))
    }

    /// 正式登记和草稿共用同一验收行规则。在根事务前为输入逐行分配 ID 并保持原顺序；批量构造失败映射为 `Logic`。
    pub fn build_customer_acceptance_lines(
        id: CustomerAcceptanceId,
        lines: &[AcceptanceLineInput],
    ) -> Result<Vec<CustomerAcceptanceLine>> {
        let specs = lines
            .iter()
            .map(|input| CustomerAcceptanceLineSpec {
                line_id: CustomerAcceptanceLineId::new(next_id()),
                sales_order_line_id: input.sales_order_line_id.clone(),
                accepted_quantity: input.accepted_quantity,
                short_quantity: input.short_quantity,
                rejected_quantity: input.rejected_quantity,
                reason: input.reason.clone(),
            })
            .collect();
        CustomerAcceptanceLineBatch::build(id, specs).map_err(Error::Logic)
    }

    /// 登记带原验收单 ID 时，沿根事务读取该销售单的草稿并核对期望版本；新登记直接返回 `None`，不抢先读取销售或责任任务。
    /// 草稿不存在返回 `NotFound`，缺少期望版本返回 `ValidationError`，销售归属、草稿状态或版本不符返回 `ConflictError`；传播读取错误。
    pub async fn load_customer_acceptance_commit_draft(
        db: &Database,
        req: &CommitCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<Option<CustomerAcceptance>> {
        let Some(id) = req.acceptance_id.as_deref() else {
            return Ok(None);
        };
        let existing = db
            .customer_acceptances()
            .find_by_id(id, session)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收草稿不存在".to_string()))?;
        ensure_existing_acceptance_draft(&existing, &req.sales_order_id, req.expected_acceptance_version)?;
        Ok(Some(existing))
    }

    /// 任务身份检查通过后，更新已加载草稿的验收时间、结果和凭证，或用服务端分配的身份及单号新建表头。
    /// 返回值的布尔位标识新建；为 `true` 时根流程须先注册业务单据。新建缺少单号返回 `Internal`，非法实体变更返回原错误。
    pub fn prepare_customer_acceptance_commit(
        existing: Option<CustomerAcceptance>,
        acceptance_id: &CustomerAcceptanceId,
        req: &CommitCustomerAcceptanceRequest,
        generated_acceptance_no: Option<String>,
    ) -> Result<(CustomerAcceptance, bool)> {
        match existing {
            Some(mut acceptance) => {
                acceptance.update(CustomerAcceptanceUpdate {
                    accepted_at: Some(Instant::from_unix_secs(req.accepted_at)),
                    result: Some(req.result),
                    evidence_attachment_id: req.evidence_attachment_id.clone(),
                })?;
                Ok((acceptance, false))
            },
            None => {
                let acceptance_no = generated_acceptance_no
                    .ok_or_else(|| Error::Internal("新建客户验收缺少服务端单号".to_string()))?;
                let acceptance = CustomerAcceptance::new(
                    acceptance_id.clone(),
                    CustomerAcceptanceData {
                        acceptance_no,
                        sales_order_id: req.sales_order_id.clone(),
                        accepted_at: Instant::from_unix_secs(req.accepted_at),
                        result: req.result,
                        evidence_attachment_id: req.evidence_attachment_id.clone(),
                    },
                )?;
                Ok((acceptance, true))
            },
        }
    }

    /// 登记提交时，在调用方事务先要求签收凭证，再创建表头和明细或更新草稿并替换明细；新单应已由根流程注册。
    /// 按验收行顺序校验并写入履约分配，每条分配在其写入位置生成 ID，最后标记表头过账。
    /// 凭证缺失、分配非法、无法过账或任一仓储操作失败时返回错误，由根流程决定提交或回滚。
    pub async fn persist_customer_acceptance_commit(
        db: &Database,
        acceptance: &mut CustomerAcceptance,
        is_new: bool,
        final_lines: &[CustomerAcceptanceLine],
        req: &CommitCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        acceptance.require_evidence()?;

        let acceptance_id = CustomerAcceptanceId::new(acceptance.base.id.clone());
        if is_new {
            db.fulfillment().create_customer_acceptance_with_lines(acceptance, final_lines, session).await?;
        } else {
            db.customer_acceptances().update(acceptance, session).await?;
            db.fulfillment().replace_customer_acceptance_lines(&acceptance_id, final_lines, session).await?;
        }

        write_matched_line_allocations(
            db,
            session,
            final_lines,
            req.lines.iter().map(|input| (&input.sales_order_line_id, input.allocations.as_slice())),
            "登记请求缺少验收行",
            &acceptance.sales_order_id,
        )
        .await?;

        acceptance.mark_posted()?;
        db.customer_acceptances().update(acceptance, session).await?;
        Ok(())
    }

    /// 普通过账在加载任务前先读取原验收单并要求草稿状态，沿调用方事务保留校验顺序。
    /// 找不到验收单返回 `NotFound`，不是草稿返回 `ConflictError`；传播查询错误。
    pub async fn load_customer_acceptance_for_post(
        db: &Database,
        acceptance_id: &CustomerAcceptanceId,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<CustomerAcceptance> {
        let acceptance = db
            .customer_acceptances()
            .find_by_id(acceptance_id.as_ref(), session)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
        acceptance.ensure_draft().map_err(|error| Error::ConflictError(error.to_string()))?;
        Ok(acceptance)
    }

    /// 责任任务准备完成后，沿调用方事务校验草稿的签收凭证、已保存验收行以及与过账请求的一致性。
    /// 逐行校验守恒并写入履约分配，再标记验收单过账；凭证、行、分配、状态或数据库操作不满足时中止并返回原错误。
    pub async fn persist_customer_acceptance_post(
        db: &Database,
        acceptance: &mut CustomerAcceptance,
        req: &PostCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        acceptance.require_evidence()?;
        let acceptance_id = CustomerAcceptanceId::new(acceptance.base.id.clone());
        let lines = db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&acceptance_id), session)
            .await?;
        acceptance.ensure_posting_lines(&lines).map_err(|error| Error::ValidationError(error.to_string()))?;
        ensure_post_lines_match(&lines, &req.lines)?;

        write_matched_line_allocations(
            db,
            session,
            &lines,
            req.lines.iter().map(|input| (&input.sales_order_line_id, input.allocations.as_slice())),
            "过账分配缺少验收行",
            &acceptance.sales_order_id,
        )
        .await?;

        acceptance.mark_posted()?;
        db.customer_acceptances().update(acceptance, session).await?;
        Ok(())
    }

    /// 在调用方事务按请求版本校验原验收单和来源分配，先写反向单及逐条反向分配，再把原单标为冲正。
    /// 返回已冲正原单和已过账反向单，供根流程刷新销售与任务；本方法不写销售或审计。
    /// 原单不存在返回 `NotFound`，版本、状态或来源分配不可冲正返回 `ConflictError`；构造或仓储失败返回原错误。
    pub async fn persist_customer_acceptance_reverse(
        db: &Database,
        original_id: &CustomerAcceptanceId,
        req: &ReverseCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<(CustomerAcceptance, CustomerAcceptance)> {
        let original = db
            .customer_acceptances()
            .find_by_id(original_id.as_ref(), session)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
        original
            .ensure_reversible(req.expected_version)
            .map_err(|error| Error::ConflictError(error.to_string()))?;

        let original_lines = db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(std::slice::from_ref(original_id), session)
            .await?;
        let original_line_ids: Vec<CustomerAcceptanceLineId> =
            original_lines.iter().map(|line| line.base.id.clone().into()).collect();
        let original_allocations =
            db.fulfillment().allocations_by_acceptance_lines(&original_line_ids, session).await?;
        AcceptanceFulfillmentAllocation::ensure_reversible_source(&original_allocations)
            .map_err(|error| Error::ConflictError(error.to_string()))?;

        let reverse_acceptance = CustomerAcceptance::new(
            CustomerAcceptanceId::new(next_id()),
            CustomerAcceptanceData {
                acceptance_no: format!("REV-{}", original.acceptance_no),
                sales_order_id: original.sales_order_id.clone(),
                accepted_at: Instant::now(),
                result: AcceptanceResult::Rejected,
                evidence_attachment_id: original.evidence_attachment_id.clone(),
            },
        )?;
        let (reverse_lines, reverse_allocations) = build_reverse_lines_and_allocations(
            &reverse_acceptance.base.id,
            &original_lines,
            &original_allocations,
            &req.reason_text,
        )?;

        persist_reverse_acceptance(
            db,
            session,
            original,
            reverse_acceptance,
            &reverse_lines,
            &reverse_allocations,
        )
        .await
    }
}

// 按原行号、数量和集合顺序构造反向行，记录本次冲正原因；每条 `Reverse` 分配引用对应的原 `Apply` 分配。
// 调用方先校验原分配；这里只在内存中按顺序分配新 ID，原分配缺少对应行或实体非法时返回错误。
fn build_reverse_lines_and_allocations(
    reverse_acceptance_id: &str,
    original_lines: &[CustomerAcceptanceLine],
    original_allocations: &[AcceptanceFulfillmentAllocation],
    reason_text: &str,
) -> Result<(Vec<CustomerAcceptanceLine>, Vec<AcceptanceFulfillmentAllocation>)> {
    let mut reverse_lines = Vec::with_capacity(original_lines.len());
    let mut reverse_line_by_original = HashMap::with_capacity(original_lines.len());
    for line in original_lines {
        let reverse_line_id = CustomerAcceptanceLineId::new(next_id());
        reverse_line_by_original.insert(line.base.id.clone(), reverse_line_id.clone());
        reverse_lines.push(
            CustomerAcceptanceLine::new(
                reverse_line_id,
                CustomerAcceptanceLineData {
                    customer_acceptance_id: reverse_acceptance_id.to_string().into(),
                    line_no: line.line_no,
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    accepted_quantity: line.accepted_quantity,
                    short_quantity: line.short_quantity,
                    rejected_quantity: line.rejected_quantity,
                    reason: Some(reason_text.to_string()),
                    evidence_attachment_id: None,
                },
            )
            .map_err(Error::Logic)?,
        );
    }

    let mut reverse_allocations = Vec::with_capacity(original_allocations.len());
    for allocation in original_allocations {
        let reverse_line_id = reverse_line_by_original
            .get(allocation.customer_acceptance_line_id.as_ref())
            .cloned()
            .ok_or_else(|| Error::Internal("原验收分配没有对应验收行".to_string()))?;
        reverse_allocations.push(
            AcceptanceFulfillmentAllocation::new(
                AcceptanceFulfillmentAllocationId::new(next_id()),
                AcceptanceFulfillmentAllocationData {
                    customer_acceptance_line_id: reverse_line_id,
                    fulfillment_fact_type: allocation.fulfillment_fact_type,
                    fulfillment_line_id: allocation.fulfillment_line_id.clone(),
                    allocation_action: AllocationAction::Reverse,
                    allocated_quantity: allocation.allocated_quantity,
                    reverses_allocation_id: Some(allocation.base.id.clone().into()),
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok((reverse_lines, reverse_allocations))
}

// 冲正校验完成后沿原事务写入反向表头、验收行和反向分配，先过账反向单，再登记原单的反向引用。
// 返回两张单的更新后实体；传播状态迁移及仓储错误。
async fn persist_reverse_acceptance(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    mut original: CustomerAcceptance,
    mut reverse_acceptance: CustomerAcceptance,
    reverse_lines: &[CustomerAcceptanceLine],
    reverse_allocations: &[AcceptanceFulfillmentAllocation],
) -> Result<(CustomerAcceptance, CustomerAcceptance)> {
    db.fulfillment()
        .create_customer_acceptance_with_lines(&reverse_acceptance, reverse_lines, session)
        .await?;

    for allocation in reverse_allocations {
        db.acceptance_fulfillment_allocations().create(allocation, session).await?;
    }

    reverse_acceptance.mark_posted()?;
    db.customer_acceptances().update(&mut reverse_acceptance, session).await?;

    original.reverse(reverse_acceptance.base.id.clone().into())?;
    db.customer_acceptances().update(&mut original, session).await?;
    Ok((original, reverse_acceptance))
}

// 验收过账时按已保存行的顺序，以销售稳定明细匹配首个请求行，并在原事务逐行写分配。
// 请求缺行、分配为空、数量不守恒、履约事实归属或写入失败时停止并返回原错误。
async fn write_matched_line_allocations<'a>(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    lines: &[CustomerAcceptanceLine],
    inputs: impl Iterator<Item = (&'a SalesOrderLineId, &'a [AcceptanceAllocationInput])> + Clone,
    missing_line_message: &str,
    sales_order_id: &SalesOrderId,
) -> Result<()> {
    for line in lines {
        let (_, allocations) = inputs
            .clone()
            .find(|(sales_order_line_id, _)| *sales_order_line_id == &line.sales_order_line_id)
            .ok_or_else(|| Error::ValidationError(missing_line_message.to_string()))?;
        write_single_line_allocations(db, session, line, allocations, sales_order_id).await?;
    }
    Ok(())
}

// 先要求当前验收行至少一条分配且分配数量守恒，再沿原事务逐条校验归属并写入。
// 空分配、数量不守恒或任一分配校验与写入失败时停止。
async fn write_single_line_allocations(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    line: &CustomerAcceptanceLine,
    allocations: &[AcceptanceAllocationInput],
    sales_order_id: &SalesOrderId,
) -> Result<()> {
    if allocations.is_empty() {
        return Err(Error::ValidationError("验收行缺少履约分配".to_string()));
    }
    line.ensure_allocation_conserved(allocations.iter().map(|allocation| allocation.allocated_quantity))
        .map_err(|error| Error::ValidationError(error.to_string()))?;

    for allocation in allocations {
        write_acceptance_allocation(db, session, &line.base.id, allocation, line, sales_order_id).await?;
    }
    Ok(())
}

// 校验显式提交的既有验收单仍是当前销售单的可编辑草稿。
// `existing` 按草稿主键找到的既有验收单；`sales_order_id` 为当前提交所属销售单。
// `expected_version` 为客户端提交的草稿期望版本。
// 验收单不属于当前销售单、不是草稿、缺少版本或版本冲突时返回错误。
fn ensure_existing_acceptance_draft(
    existing: &CustomerAcceptance,
    sales_order_id: &SalesOrderId,
    expected_version: Option<u64>,
) -> Result<()> {
    if &existing.sales_order_id != sales_order_id {
        return Err(Error::ConflictError("客户验收草稿不属于当前销售单".to_string()));
    }
    let expected_version =
        expected_version.ok_or_else(|| Error::ValidationError("请打开已有草稿并刷新后再提交".to_string()))?;
    existing
        .ensure_draft_version(expected_version)
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    Ok(())
}

// 校验过账分配与草稿验收行一一对应且数量一致（§8.2 第 5 条「锁定验收行」）。
// 一致返回 `Ok(())`。
// `lines` 为草稿验收行；`inputs` 为过账请求行。
// 行集合不一致时返回 `ValidationError`。
fn ensure_post_lines_match(
    lines: &[CustomerAcceptanceLine],
    inputs: &[PostAcceptanceLineInput],
) -> Result<()> {
    let projected: Vec<(erp_core::ids::SalesOrderLineId, usize)> =
        inputs.iter().map(|input| (input.sales_order_line_id.clone(), input.allocations.len())).collect();
    CustomerAcceptanceLine::ensure_post_inputs_match(lines, &projected)
        .map_err(|error| Error::ValidationError(error.to_string()))
}

// 写入单条验收履约分配并校验净验收上限（§8.2 第 5 条，位于调用方事务内）。
// 校验履约事实存在、属于同一销售明细且处于有效状态；净验收（既有 APPLY − REVERSE + 本次）不得超过该事实的净成功履约数量。
// 写入失败时返回错误。
// 沿用调用方当前事务的执行器 `session`；`line_id` 为验收行主键。
// `allocation` 为分配输入；`acceptance_line` 为验收行（销售明细归属）。
// `sales_order_id` 为销售单（校验事实归属）。
// 事实不存在/状态无效/归属不符或净验收超上限时返回 `ValidationError`。
async fn write_acceptance_allocation(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    line_id: &str,
    allocation: &AcceptanceAllocationInput,
    acceptance_line: &CustomerAcceptanceLine,
    sales_order_id: &erp_core::ids::SalesOrderId,
) -> Result<()> {
    let net_successful = load_fulfillment_fact(
        db,
        session,
        allocation.fulfillment_fact_type,
        &allocation.fulfillment_line_id,
        sales_order_id,
        acceptance_line,
    )
    .await?;

    let existing = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            allocation.fulfillment_fact_type,
            std::slice::from_ref(&allocation.fulfillment_line_id),
            session,
        )
        .await?;
    AcceptanceFulfillmentAllocation::ensure_apply_within_successful_quantity(
        net_successful,
        &existing,
        &allocation.fulfillment_line_id,
        allocation.allocated_quantity,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;

    let record = AcceptanceFulfillmentAllocation::new(
        AcceptanceFulfillmentAllocationId::new(next_id()),
        AcceptanceFulfillmentAllocationData {
            customer_acceptance_line_id: line_id.to_string().into(),
            fulfillment_fact_type: allocation.fulfillment_fact_type,
            fulfillment_line_id: allocation.fulfillment_line_id.clone(),
            allocation_action: AllocationAction::Apply,
            allocated_quantity: allocation.allocated_quantity,
            reverses_allocation_id: None,
        },
    )?;
    db.acceptance_fulfillment_allocations().create(&record, session).await?;
    Ok(())
}

// 加载履约事实的净成功数量并校验事实存在与状态有效。
// 返回净成功履约数量。
// 沿用调用方当前事务的执行器 `session`；`fact_type` 为履约事实类型。
// `fact_id` 为履约事实行主键；`sales_order_id` 为销售单（校验归属）。
// `acceptance_line` 为验收行（校验销售稳定明细归属）。
// 事实不存在或状态无效时返回 `ValidationError`。
async fn load_fulfillment_fact(
    db: &Database,
    session: &mut dyn persistence_core::Executor,
    fact_type: FulfillmentFactType,
    fact_id: &str,
    sales_order_id: &erp_core::ids::SalesOrderId,
    acceptance_line: &CustomerAcceptanceLine,
) -> Result<Quantity> {
    match fact_type {
        FulfillmentFactType::Delivery => {
            let line = db
                .delivery_lines()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("发货事实不存在".to_string()))?;
            let delivery = db
                .deliveries()
                .find_by_id(line.delivery_id.as_ref(), session)
                .await?
                .ok_or_else(|| Error::ValidationError("发货单不存在".to_string()))?;
            delivery
                .acceptance_quantity(&line, sales_order_id, &acceptance_line.sales_order_line_id)
                .map_err(|error| Error::ValidationError(error.to_string()))
        },
        FulfillmentFactType::ElectronicDelivery => {
            let record = db
                .electronic_deliveries()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("电子交付事实不存在".to_string()))?;
            record
                .acceptance_quantity(&acceptance_line.sales_order_line_id)
                .map_err(|error| Error::ValidationError(error.to_string()))
        },
        FulfillmentFactType::ServiceFulfillment => {
            let record = db
                .service_fulfillments()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("服务履约事实不存在".to_string()))?;
            record
                .acceptance_quantity(&acceptance_line.sales_order_line_id)
                .map_err(|error| Error::ValidationError(error.to_string()))
        },
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{CustomerAcceptanceId, SalesOrderId};

    use super::ensure_existing_acceptance_draft;
    use crate::Error;
    use crate::entity::fulfillment::{AcceptanceResult, CustomerAcceptance, CustomerAcceptanceData};
    use crate::service::FulfillmentService;

    #[test]
    fn acceptance_task_requires_both_id_and_version_or_neither() {
        let validate = FulfillmentService::validate_customer_acceptance_task_context;
        assert!(validate(None, None).is_ok());
        assert!(validate(Some("task-1"), Some(1)).is_ok());
        for (id, version) in [(Some("task-1"), None), (None, Some(1))] {
            assert!(
                matches!(validate(id, version), Err(Error::ValidationError(message)) if message == "客户验收任务主键和期望版本必须同时提供")
            );
        }
    }

    // 构造最小客户验收草稿用于提交状态分流测试。
    fn draft_acceptance(id: &str, acceptance_no: &str) -> CustomerAcceptance {
        CustomerAcceptance::new(
            CustomerAcceptanceId::new(id),
            CustomerAcceptanceData {
                acceptance_no: acceptance_no.to_string(),
                sales_order_id: SalesOrderId::new("sales-order-1"),
                accepted_at: Instant::from_unix_secs(1_700_000_000),
                result: AcceptanceResult::Passed,
                evidence_attachment_id: None,
            },
        )
        .expect("测试验收草稿应合法")
    }

    // 验证仅当前销售单的显式草稿可以继续登记；已过账记录由命令收据回放。
    #[test]
    fn existing_acceptance_draft_guard_accepts_only_current_draft() {
        let mut posted = draft_acceptance("acceptance-posted", "YS-POSTED");
        posted.mark_posted().expect("测试验收应可过账");
        assert!(
            ensure_existing_acceptance_draft(
                &posted,
                &SalesOrderId::new("sales-order-1"),
                Some(posted.base.version),
            )
            .expect_err("已过账记录不得作为草稿登记")
            .to_string()
            .contains("草稿")
        );

        let draft = draft_acceptance("acceptance-draft", "YS-DRAFT");
        ensure_existing_acceptance_draft(
            &draft,
            &SalesOrderId::new("sales-order-1"),
            Some(draft.base.version),
        )
        .expect("正确版本的当前销售单草稿应可继续登记");

        assert!(
            ensure_existing_acceptance_draft(
                &draft,
                &SalesOrderId::new("sales-order-other"),
                Some(draft.base.version),
            )
            .expect_err("其他销售单不得复用草稿")
            .to_string()
            .contains("不属于当前销售单")
        );

        assert!(
            ensure_existing_acceptance_draft(&draft, &SalesOrderId::new("sales-order-1"), None)
                .expect_err("显式草稿缺少版本必须拒绝")
                .to_string()
                .contains("缺少期望版本")
        );
        assert!(
            ensure_existing_acceptance_draft(
                &draft,
                &SalesOrderId::new("sales-order-1"),
                Some(draft.base.version + 1),
            )
            .expect_err("过期草稿版本必须拒绝")
            .to_string()
            .contains("草稿已变化")
        );
    }

    // 验证已冲正记录不得被当作草稿继续登记。
    #[test]
    fn existing_acceptance_draft_guard_rejects_reversed_record() {
        let mut reversed = draft_acceptance("acceptance-reversed", "YS-REVERSED");
        reversed.mark_posted().expect("测试验收应可过账");
        reversed.reverse(CustomerAcceptanceId::new("acceptance-reversal")).expect("测试验收应可冲正");

        let error = ensure_existing_acceptance_draft(
            &reversed,
            &SalesOrderId::new("sales-order-1"),
            Some(reversed.base.version),
        )
        .expect_err("已冲正记录不得作为草稿登记");
        assert!(error.to_string().contains("草稿"));
    }
}
