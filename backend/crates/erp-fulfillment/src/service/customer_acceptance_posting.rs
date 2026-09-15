//! 客户验收单域规则、事务内事实写入和履约投影读取；根事务与销售更新由流程持有。
use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_core::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, SalesOrderId,
};
use erp_core::money::Quantity;
use id_generator::next_id;
use mongodb::Database;

use super::customer_acceptance_lines::acceptance_line_specs;
use crate::dto::{
    AcceptanceAllocationInput, CommitCustomerAcceptanceRequest, PostAcceptanceLineInput,
    PostCustomerAcceptanceRequest, ReverseCustomerAcceptanceRequest,
};
use crate::entity::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceResult, AllocationAction,
    CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine, CustomerAcceptanceLineData,
    CustomerAcceptanceUpdate, FulfillmentFactType,
};
use crate::repository::FulfillmentExt;
use crate::{Error, Result};
impl super::FulfillmentService {
    /// 校验工作台任务身份必须与期望版本成对提供，保持正式命令入口首错。
    pub fn validate_customer_acceptance_task_context(
        work_item_id: Option<&str>,
        expected_task_version: Option<u64>,
    ) -> Result<()> {
        ensure_task_context_pair(work_item_id, expected_task_version)
    }

    /// 在根事务前按原顺序构建验收行与行 ID；正式命令和原草稿共用领域规则。
    pub fn build_customer_acceptance_lines(
        id: CustomerAcceptanceId,
        lines: &[crate::dto::AcceptanceLineInput],
    ) -> Result<Vec<CustomerAcceptanceLine>> {
        crate::entity::fulfillment::CustomerAcceptanceLineBatch::build(id, acceptance_line_specs(lines))
            .map_err(Error::Logic)
    }

    /// 加载可继续登记的草稿；新登记返回 None，不提前读取销售或任务。
    pub async fn load_customer_acceptance_commit_draft(
        db: &Database,
        req: &CommitCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<Option<CustomerAcceptance>> {
        let existing = if let Some(id) = req.acceptance_id.as_deref() {
            Some(
                db.customer_acceptances()
                    .find_by_id(id, session)
                    .await?
                    .ok_or_else(|| Error::NotFound("客户验收草稿不存在".to_string()))?,
            )
        } else {
            None
        };
        if let Some(existing) = existing.as_ref() {
            ensure_existing_acceptance_draft(existing, &req.sales_order_id, req.expected_acceptance_version)?;
        }
        Ok(existing)
    }

    /// 任务身份准备成功后构造或更新验收表头；返回是否需先注册新的业务单据。
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
                    },
                )?;
                Ok((acceptance, true))
            },
        }
    }

    /// 写入登记表头/行、逐条履约分配与过账状态；新表头已由根流程完成无绑定注册。
    /// 验证和 ID 生成仍穿插在原逐行写入位置，不提前生成全部分配。
    pub async fn persist_customer_acceptance_commit(
        db: &Database,
        acceptance: &mut CustomerAcceptance,
        is_new: bool,
        final_lines: &[CustomerAcceptanceLine],
        req: &CommitCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        let acceptance_id = CustomerAcceptanceId::new(acceptance.base.id.clone());
        if is_new {
            db.fulfillment().create_customer_acceptance_with_lines(acceptance, final_lines, session).await?;
        } else {
            db.customer_acceptances().update(acceptance, session).await?;
            db.fulfillment().replace_customer_acceptance_lines(&acceptance_id, final_lines, session).await?;
        }
        for line in final_lines {
            let allocations = req
                .lines
                .iter()
                .find(|input| input.sales_order_line_id == line.sales_order_line_id)
                .map(|input| input.allocations.as_slice())
                .ok_or_else(|| Error::ValidationError("登记请求缺少验收行".to_string()))?;
            if allocations.is_empty() {
                return Err(Error::ValidationError("验收行缺少履约分配".to_string()));
            }
            line.ensure_allocation_conserved(
                allocations.iter().map(|allocation| allocation.allocated_quantity),
            )
            .map_err(|error| Error::ValidationError(error.to_string()))?;
            for allocation in allocations {
                write_acceptance_allocation(
                    db,
                    session,
                    &line.base.id,
                    allocation,
                    line,
                    &acceptance.sales_order_id,
                )
                .await?;
            }
        }
        acceptance.mark_posted()?;
        db.customer_acceptances().update(acceptance, session).await?;
        Ok(())
    }

    /// 过账前读取并拒绝非草稿验收；任务读取必须位于本守卫之后。
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

    /// 当前责任任务准备成功后，校验并持久化草稿的履约分配及过账状态。
    pub async fn persist_customer_acceptance_post(
        db: &Database,
        acceptance: &mut CustomerAcceptance,
        req: &PostCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        let acceptance_id = CustomerAcceptanceId::new(acceptance.base.id.clone());
        let lines = db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&acceptance_id), session)
            .await?;
        acceptance.ensure_posting_lines(&lines).map_err(|error| Error::ValidationError(error.to_string()))?;
        ensure_post_lines_match(&lines, &req.lines)?;
        for line in &lines {
            let allocations = req
                .lines
                .iter()
                .find(|input| input.sales_order_line_id == line.sales_order_line_id)
                .map(|input| input.allocations.clone())
                .ok_or_else(|| Error::ValidationError("过账分配缺少验收行".to_string()))?;
            line.ensure_allocation_conserved(
                allocations.iter().map(|allocation| allocation.allocated_quantity),
            )
            .map_err(|error| Error::ValidationError(error.to_string()))?;
            for allocation in &allocations {
                write_acceptance_allocation(
                    db,
                    session,
                    &line.base.id,
                    allocation,
                    line,
                    &acceptance.sales_order_id,
                )
                .await?;
            }
        }
        acceptance.mark_posted()?;
        db.customer_acceptances().update(acceptance, session).await?;
        Ok(())
    }

    /// 校验原验收并按原顺序写反向验收、反向分配和原单冲正状态。
    /// 返回原单和新反向单，供根流程继续刷新销售与任务；本接口不写审计或销售。
    pub async fn persist_customer_acceptance_reverse(
        db: &Database,
        original_id: &CustomerAcceptanceId,
        req: &ReverseCustomerAcceptanceRequest,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<(CustomerAcceptance, CustomerAcceptance)> {
        let mut original = db
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
            },
        )?;
        let mut reverse_lines = Vec::with_capacity(original_lines.len());
        let mut reverse_line_by_original = HashMap::with_capacity(original_lines.len());
        for line in &original_lines {
            let reverse_line_id = CustomerAcceptanceLineId::new(next_id());
            reverse_line_by_original.insert(line.base.id.clone(), reverse_line_id.clone());
            reverse_lines.push(
                CustomerAcceptanceLine::new(
                    reverse_line_id,
                    CustomerAcceptanceLineData {
                        customer_acceptance_id: reverse_acceptance.base.id.clone().into(),
                        line_no: line.line_no,
                        sales_order_line_id: line.sales_order_line_id.clone(),
                        accepted_quantity: line.accepted_quantity,
                        short_quantity: line.short_quantity,
                        rejected_quantity: line.rejected_quantity,
                        reason: Some(req.reason_text.clone()),
                        evidence_attachment_id: None,
                    },
                )
                .map_err(Error::Logic)?,
            );
        }
        let mut reverse_allocations = Vec::with_capacity(original_allocations.len());
        for allocation in &original_allocations {
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
        db.fulfillment()
            .create_customer_acceptance_with_lines(&reverse_acceptance, &reverse_lines, session)
            .await?;
        for allocation in &reverse_allocations {
            db.acceptance_fulfillment_allocations().create(allocation, session).await?;
        }
        let mut reverse_acceptance = reverse_acceptance;
        reverse_acceptance.mark_posted()?;
        db.customer_acceptances().update(&mut reverse_acceptance, session).await?;
        original.reverse(reverse_acceptance.base.id.clone().into())?;
        db.customer_acceptances().update(&mut original, session).await?;
        Ok((original, reverse_acceptance))
    }
}

/// 校验显式提交的既有验收单仍是当前销售单的可编辑草稿。
///
/// # 参数
/// * `existing` - 按草稿主键找到的既有验收单
/// * `sales_order_id` - 当前提交所属销售单
/// * `expected_version` - 客户端提交的草稿期望版本
///
/// # 错误
/// 验收单不属于当前销售单、不是草稿、缺少版本或版本冲突时返回错误。
fn ensure_existing_acceptance_draft(
    existing: &CustomerAcceptance,
    sales_order_id: &SalesOrderId,
    expected_version: Option<u64>,
) -> Result<()> {
    if &existing.sales_order_id != sales_order_id {
        return Err(Error::ConflictError("客户验收草稿不属于当前销售单".to_string()));
    }
    let expected_version =
        expected_version.ok_or_else(|| Error::ValidationError("已有草稿缺少期望版本".to_string()))?;
    existing
        .ensure_draft_version(expected_version)
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    Ok(())
}

/// 校验统一工作台任务身份和乐观锁版本必须成对出现。
fn ensure_task_context_pair(work_item_id: Option<&str>, expected_task_version: Option<u64>) -> Result<()> {
    if work_item_id.is_some() == expected_task_version.is_some() {
        return Ok(());
    }
    Err(Error::ValidationError("客户验收任务主键和期望版本必须同时提供".to_string()))
}

/// 校验过账分配与草稿验收行一一对应且数量一致（§8.2 第 5 条「锁定验收行」）。
///
/// # 参数
/// * `lines` - 草稿验收行
/// * `inputs` - 过账请求行
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 行集合不一致时返回 `ValidationError`。
fn ensure_post_lines_match(
    lines: &[CustomerAcceptanceLine],
    inputs: &[PostAcceptanceLineInput],
) -> Result<()> {
    if lines.len() != inputs.len() {
        return Err(Error::ValidationError("过账分配与验收行数量不一致".to_string()));
    }
    for line in lines {
        let input = inputs
            .iter()
            .find(|input| input.sales_order_line_id == line.sales_order_line_id)
            .ok_or_else(|| Error::ValidationError("过账分配缺少验收行".to_string()))?;
        if input.allocations.is_empty() {
            return Err(Error::ValidationError("验收行缺少履约分配".to_string()));
        }
    }
    Ok(())
}

/// 写入单条验收履约分配并校验净验收上限（§8.2 第 5 条，位于调用方事务内）。
///
/// 校验履约事实存在、属于同一销售明细且处于有效状态；净验收（既有 APPLY −
/// REVERSE + 本次）不得超过该事实的净成功履约数量。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `line_id` - 验收行主键
/// * `allocation` - 分配输入
/// * `acceptance_line` - 验收行（销售明细归属）
/// * `sales_order_id` - 销售单（校验事实归属）
///
/// # 返回
/// 无返回值；写入失败时返回错误。
///
/// # 错误
/// 事实不存在/状态无效/归属不符或净验收超上限时返回 `ValidationError`。
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

/// 加载履约事实的净成功数量并校验事实存在与状态有效。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `fact_type` - 履约事实类型
/// * `fact_id` - 履约事实行主键
/// * `sales_order_id` - 销售单（校验归属）
/// * `acceptance_line` - 验收行（校验销售稳定明细归属）
///
/// # 返回
/// 返回净成功履约数量。
///
/// # 错误
/// 事实不存在或状态无效时返回 `ValidationError`。
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
    use crate::entity::fulfillment::{AcceptanceResult, CustomerAcceptance, CustomerAcceptanceData};

    /// 构造最小客户验收草稿用于提交状态分流测试。
    fn draft_acceptance(id: &str, acceptance_no: &str) -> CustomerAcceptance {
        CustomerAcceptance::new(
            CustomerAcceptanceId::new(id),
            CustomerAcceptanceData {
                acceptance_no: acceptance_no.to_string(),
                sales_order_id: SalesOrderId::new("sales-order-1"),
                accepted_at: Instant::from_unix_secs(1_700_000_000),
                result: AcceptanceResult::Passed,
            },
        )
        .expect("测试验收草稿应合法")
    }

    /// 仅当前销售单的显式草稿可以继续登记；已过账记录由命令收据回放。
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

    /// 已冲正记录不得被当作草稿继续登记。
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

    /// 过账与冲正可以同步 W06 责任，但不得启动审批或选择审批定义。
    #[test]
    fn post_does_not_start_approval() {
        let production =
            include_str!("customer_acceptance_posting.rs").split("#[cfg(test)]").next().expect("生产代码");
        let post =
            include_str!("../../../erp-processes/src/fulfillment_execution/customer_acceptance/post.rs");
        let reverse =
            include_str!("../../../erp-processes/src/fulfillment_execution/customer_acceptance/reverse.rs");
        assert!(post.contains("pub async fn post_customer_acceptance"));
        assert!(reverse.contains("pub async fn reverse_customer_acceptance"));
        for source in [production, post, reverse] {
            assert!(!source.contains("start_approval"));
            assert!(!source.contains("prepare_start"));
            assert!(!source.contains("definition_id"));
            assert!(!source.contains("CustomerAcceptanceAdapter"));
            assert!(!source.contains("bind_published_definition_on_document_create"));
        }
        assert!(post.contains("prepare_customer_acceptance_task_command"));
        assert!(production.contains("mark_posted"));
        assert!(production.contains("original.reverse"));
    }
}
