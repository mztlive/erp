use std::str::FromStr;

use database::{AccessControlExt, FulfillmentExt, SalesOrderExt, Transactional};
use entities::common::time::Instant;
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AcceptanceResult, AllocationAction,
    CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine, CustomerAcceptanceLineData,
    CustomerAcceptanceUpdate, FulfillmentFactType,
};
use entities::ids::{
    AcceptanceFulfillmentAllocationId, CustomerAcceptanceId, CustomerAcceptanceLineId, SalesOrderId,
};
use entities::money::Quantity;
use entities::sales_order::{BusinessType, FulfillmentProgress};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

use super::acceptance_eligibility::{build_eligibility_groups, so_line_ids, EligibilityGroupSources};
use super::customer_acceptance::{build_acceptance_lines, register_created_customer_acceptance_document};
use super::{
    AcceptanceAllocationInput, CommitCustomerAcceptanceRequest, CommitCustomerAcceptanceView,
    CustomerAcceptanceView, FulfillmentService, PostAcceptanceLineInput, PostCustomerAcceptanceRequest,
    ReverseCustomerAcceptanceRequest,
};

impl FulfillmentService {
    /// 原子登记并过账客户验收。
    ///
    /// 同一事务内完成草稿创建或完整替换、履约事实分配校验与写入、验收过账、
    /// 销售单履约进度刷新和审计。前端无需先保存草稿或预查询事实类型。
    /// 已按同一验收单号完成过账时直接返回既有结果，支持结果未知后的安全重试。
    ///
    /// # 参数
    /// * `req` - 最终验收表头、行、分配和乐观锁版本
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回已过账验收单以及过账后重新计算的剩余可验收事实。
    ///
    /// # 错误
    /// * `ValidationError` - 行、分配、事实归属或版本参数不合法
    /// * `ConflictError` - 草稿、销售单版本或状态冲突
    /// * `NotFound` - 草稿、销售单或履约事实不存在
    /// * `OutcomeUnknown` - 事务提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_commit",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_commit"
        )
    )]
    pub async fn commit_customer_acceptance(
        &self,
        req: CommitCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CommitCustomerAcceptanceView> {
        req.validate()?;
        if req.acceptance_id.is_some() != req.expected_acceptance_version.is_some() {
            return Err(Error::ValidationError(
                "已有草稿必须同时提交草稿主键和期望版本".to_string(),
            ));
        }
        if req.acceptance_id.is_none()
            && req
                .acceptance_no
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
        {
            return Err(Error::ValidationError("新建客户验收必须提供验收单号".to_string()));
        }

        let acceptance_id = req
            .acceptance_id
            .as_ref()
            .map(|id| CustomerAcceptanceId::new(id.clone()))
            .unwrap_or_else(|| CustomerAcceptanceId::new(next_id()));
        let final_lines = build_acceptance_lines(&acceptance_id, &req.lines)?;
        let sales_order_id = req.sales_order_id.clone();
        let actor = actor.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let existing = if let Some(id) = req.acceptance_id.as_deref() {
                        db.customer_acceptances().find_by_id(id, session).await?
                    } else {
                        let acceptance_no = req.acceptance_no.as_deref().ok_or_else(|| {
                            Error::ValidationError("新建客户验收必须提供验收单号".to_string())
                        })?;
                        db.customer_acceptances()
                            .find_by_acceptance_no(acceptance_no, session)
                            .await?
                    };

                    if let Some(existing) = existing.as_ref() {
                        if !existing
                            .matches_business_identity(&req.sales_order_id, req.acceptance_no.as_deref())
                        {
                            return Err(Error::ConflictError("验收单号已被其他业务占用".to_string()));
                        }
                        if existing.is_posted() {
                            return Ok::<CustomerAcceptance, crate::errors::Error>(existing.clone());
                        }
                    }

                    let order = db
                        .sales_orders()
                        .find_by_id(req.sales_order_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
                    if order.base.version != req.expected_sales_order_version {
                        return Err(Error::ConflictError(
                            "销售单已变化，请刷新履约事实后重试".to_string(),
                        ));
                    }

                    let mut acceptance = match existing {
                        Some(mut acceptance) => {
                            let expected_version = req
                                .expected_acceptance_version
                                .ok_or_else(|| Error::ValidationError("已有草稿缺少期望版本".to_string()))?;
                            acceptance
                                .ensure_draft_version(expected_version)
                                .map_err(|error| Error::ConflictError(error.to_string()))?;
                            acceptance.update(CustomerAcceptanceUpdate {
                                accepted_at: Some(Instant::from_unix_secs(req.accepted_at)),
                                result: Some(req.result),
                            })?;
                            db.customer_acceptances().update(&mut acceptance, session).await?;
                            db.fulfillment()
                                .replace_customer_acceptance_lines(&acceptance_id, &final_lines, session)
                                .await?;
                            acceptance
                        }
                        None => {
                            let acceptance_no = req.acceptance_no.clone().ok_or_else(|| {
                                Error::ValidationError("新建客户验收必须提供验收单号".to_string())
                            })?;
                            let acceptance = CustomerAcceptance::new(
                                acceptance_id.clone(),
                                CustomerAcceptanceData {
                                    acceptance_no,
                                    sales_order_id: req.sales_order_id.clone(),
                                    accepted_at: Instant::from_unix_secs(req.accepted_at),
                                    result: req.result,
                                },
                            )?;
                            register_created_customer_acceptance_document(
                                &db,
                                &rbac,
                                &acceptance,
                                &actor,
                                session,
                            )
                            .await?;
                            db.fulfillment()
                                .create_customer_acceptance_with_lines(&acceptance, &final_lines, session)
                                .await?;
                            acceptance
                        }
                    };

                    for line in &final_lines {
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
                                &db,
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
                    db.customer_acceptances().update(&mut acceptance, session).await?;
                    update_sales_order_fulfillment_progress(
                        &db,
                        session,
                        &acceptance.sales_order_id,
                        actor.id().to_string(),
                    )
                    .await?;
                    let audit = actor.resource_log(
                        "customer_acceptance.commit",
                        "customer_acceptance",
                        acceptance.base.id.clone(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAcceptance, crate::errors::Error>(acceptance)
                })
            })
            .await?;

        let remaining_eligibility = self.acceptance_eligibility(sales_order_id.as_ref()).await?;
        Ok(CommitCustomerAcceptanceView {
            acceptance: posted.into(),
            remaining_eligibility,
        })
    }

    /// 过账客户验收（草稿 → 已过账；§8.2 第 5 条跨集合事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：过账只写履约分配与状态迁移，不得绑定
    /// 定义、启动审批实例或创建审批任务。
    ///
    /// 在同一事务内：锁定验收行与履约事实、校验逐行分配守恒（分配合计等于
    /// 通过数量）、校验每个履约事实的净验收数量不超过净成功履约数量、写
    /// `APPLY` 分配、迁移验收单状态、写审计。重复过账由状态守卫（仅草稿）
    /// 与状态机（`Draft → Posted`）防护。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    /// * `req` - 过账请求（逐行分配）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的验收单视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单/履约事实不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `ValidationError` - 分配不守恒或超上限
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_post",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_post"
        )
    )]
    pub async fn post_customer_acceptance(
        &self,
        id: &str,
        req: PostCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let acceptance_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut acceptance = db
                        .customer_acceptances()
                        .find_by_id(acceptance_id.as_ref(), session)
                        .await?
                        .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
                    acceptance
                        .ensure_draft()
                        .map_err(|error| Error::ConflictError(error.to_string()))?;
                    let lines = db
                        .fulfillment()
                        .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&acceptance_id), session)
                        .await?;
                    acceptance
                        .ensure_posting_lines(&lines)
                        .map_err(|error| Error::ValidationError(error.to_string()))?;
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
                                &db,
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
                    db.customer_acceptances().update(&mut acceptance, session).await?;
                    // 验收通过即履约完成（§4.3.1）：按净已验收汇总刷新销售单履约进度
                    update_sales_order_fulfillment_progress(
                        &db,
                        session,
                        &acceptance.sales_order_id,
                        actor.id().to_string(),
                    )
                    .await?;
                    let audit = actor.resource_log(
                        "customer_acceptance.post",
                        "customer_acceptance",
                        acceptance_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAcceptance, crate::errors::Error>(acceptance)
                })
            })
            .await?;
        Ok(posted.into())
    }

    /// 冲正客户验收（已过账 → 已冲正；§8.2 第 5 条反向分配事务）。
    ///
    /// 客户验收签署为 `NO_APPROVAL`：冲正只追加反向验收事实，不得启动审批
    /// 或创建任务。
    ///
    /// 误录时新增反向验收单：原验收行的通过/短少/拒收数量镜像复制，原
    /// `APPLY` 分配逐条生成 `REVERSE` 分配（引用原分配），新验收单立即过账，
    /// 原验收单登记反向引用并迁移到 `REVERSED`。冲正不覆盖原验收事实。
    ///
    /// # 参数
    /// * `id` - 待冲正验收单主键
    /// * `req` - 冲正请求（期望版本 + 原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建反向验收单的视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `ConflictError` - 版本不符或状态不允许冲正
    /// * `OutcomeUnknown` - 提交结果无法确认
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_reverse",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_reverse"
        )
    )]
    pub async fn reverse_customer_acceptance(
        &self,
        id: &str,
        req: ReverseCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let original_id = CustomerAcceptanceId::new(id.to_string());
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let reversed = client
            .with_transaction(move |session| {
                Box::pin(async move {
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
                        .acceptance_lines_by_acceptance_ids(std::slice::from_ref(&original_id), session)
                        .await?;
                    let original_line_ids: Vec<CustomerAcceptanceLineId> = original_lines
                        .iter()
                        .map(|line| line.base.id.clone().into())
                        .collect();
                    let original_allocations = db
                        .fulfillment()
                        .allocations_by_acceptance_lines(&original_line_ids, session)
                        .await?;
                    if original_allocations.is_empty() {
                        return Err(Error::ValidationError(
                            "原验收单没有可冲正的分配，无法冲正".to_string(),
                        ));
                    }
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
                    for line in &original_lines {
                        reverse_lines.push(
                            CustomerAcceptanceLine::new(
                                CustomerAcceptanceLineId::new(next_id()),
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
                    let reverse_line_ids: Vec<CustomerAcceptanceLineId> = reverse_lines
                        .iter()
                        .map(|line| line.base.id.clone().into())
                        .collect();
                    let mut reverse_allocations = Vec::with_capacity(original_allocations.len());
                    for (index, allocation) in original_allocations
                        .iter()
                        .filter(|allocation| allocation.allocation_action == AllocationAction::Apply)
                        .enumerate()
                    {
                        reverse_allocations.push(
                            AcceptanceFulfillmentAllocation::new(
                                AcceptanceFulfillmentAllocationId::new(next_id()),
                                AcceptanceFulfillmentAllocationData {
                                    customer_acceptance_line_id: reverse_line_ids[index].clone(),
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
                        db.acceptance_fulfillment_allocations()
                            .create(allocation, session)
                            .await?;
                    }
                    let mut reverse_acceptance = reverse_acceptance;
                    reverse_acceptance.mark_posted()?;
                    db.customer_acceptances()
                        .update(&mut reverse_acceptance, session)
                        .await?;
                    original.reverse(reverse_acceptance.base.id.clone().into())?;
                    db.customer_acceptances().update(&mut original, session).await?;
                    // 冲正后净已验收减少：同步刷新销售单履约进度（可能从已完成回退）
                    update_sales_order_fulfillment_progress(
                        &db,
                        session,
                        &original.sales_order_id,
                        actor.id().to_string(),
                    )
                    .await?;
                    let audit = actor.resource_log(
                        "customer_acceptance.reverse",
                        "customer_acceptance",
                        original_id.to_string(),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<CustomerAcceptance, crate::errors::Error>(reverse_acceptance)
                })
            })
            .await?;
        Ok(reversed.into())
    }
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
    session: &mut mongodb::ClientSession,
    line_id: &str,
    allocation: &AcceptanceAllocationInput,
    acceptance_line: &CustomerAcceptanceLine,
    sales_order_id: &entities::ids::SalesOrderId,
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
    db.acceptance_fulfillment_allocations()
        .create(&record, session)
        .await?;
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
    session: &mut mongodb::ClientSession,
    fact_type: FulfillmentFactType,
    fact_id: &str,
    sales_order_id: &entities::ids::SalesOrderId,
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
        }
        FulfillmentFactType::ElectronicDelivery => {
            let record = db
                .electronic_deliveries()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("电子交付事实不存在".to_string()))?;
            record
                .acceptance_quantity(&acceptance_line.sales_order_line_id)
                .map_err(|error| Error::ValidationError(error.to_string()))
        }
        FulfillmentFactType::ServiceFulfillment => {
            let record = db
                .service_fulfillments()
                .find_by_id(fact_id, session)
                .await?
                .ok_or_else(|| Error::ValidationError("服务履约事实不存在".to_string()))?;
            record
                .acceptance_quantity(&acceptance_line.sales_order_line_id)
                .map_err(|error| Error::ValidationError(error.to_string()))
        }
    }
}

/// 验收过账/冲正后刷新销售单履约进度（§4.3.1：实物与服务「客户验收通过即履约完成」）。
///
/// 按销售明细汇总净已验收（APPLY − REVERSE）并与应履约数量比较：
/// 全部明细验收通过 → 已完成；部分通过 → 部分履约；否则 → 未开始。
/// 卡券销售单进度由履约期限到期任务写入（§4.3.1），本函数不触碰。
///
/// # 参数
/// * `db` - 数据库实例
/// * `session` - 事务会话执行器
/// * `sales_order_id` - 销售单
/// * `actor_id` - 审计操作人
///
/// # 返回
/// 无返回值；进度变化时更新销售单并写版本触及。
async fn update_sales_order_fulfillment_progress(
    db: &Database,
    session: &mut mongodb::ClientSession,
    sales_order_id: &SalesOrderId,
    actor_id: String,
) -> Result<()> {
    let order = db
        .sales_orders()
        .find_by_id(sales_order_id.as_ref(), session)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    if order.business_type != BusinessType::GoodsService {
        return Ok(());
    }
    let revision_id = order
        .stable
        .current_revision_id
        .clone()
        .ok_or_else(|| Error::NotFound("销售单没有生效版本".to_string()))?;
    let revision = db
        .sales_order_revisions()
        .find_by_id(&revision_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("销售生效版本不存在".to_string()))?;
    let revision_lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&revision.base.id.clone().into(), session)
        .await?;
    let revision_line_ids: Vec<entities::ids::SalesOrderRevisionLineId> = revision_lines
        .iter()
        .map(|line| line.base.id.clone().into())
        .collect();
    let goods_service_lines = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(&revision_line_ids, session)
        .await?;
    let deliveries = db
        .fulfillment()
        .list_acceptance_eligible_deliveries(sales_order_id, session)
        .await?;
    let delivery_ids: Vec<entities::ids::DeliveryId> = deliveries
        .iter()
        .map(|delivery| delivery.base.id.clone().into())
        .collect();
    let delivery_lines = db
        .fulfillment()
        .delivery_lines_by_delivery_ids(&delivery_ids, session)
        .await?;
    let sales_order_line_ids = so_line_ids(&revision_lines);
    let electronic = db
        .fulfillment()
        .list_confirmed_electronic_deliveries(&sales_order_line_ids, session)
        .await?;
    let service = db
        .fulfillment()
        .list_confirmed_service_fulfillments(&sales_order_line_ids, session)
        .await?;
    let delivery_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::Delivery,
            &delivery_lines
                .iter()
                .map(|line| line.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let electronic_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::ElectronicDelivery,
            &electronic
                .iter()
                .map(|record| record.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let service_allocations = db
        .fulfillment()
        .allocations_by_fulfillment_fact(
            FulfillmentFactType::ServiceFulfillment,
            &service
                .iter()
                .map(|record| record.base.id.clone())
                .collect::<Vec<_>>(),
            session,
        )
        .await?;
    let groups = build_eligibility_groups(EligibilityGroupSources {
        revision_lines: &revision_lines,
        goods_service_lines: &goods_service_lines,
        deliveries: &deliveries,
        delivery_lines: &delivery_lines,
        electronic: &electronic,
        service: &service,
        delivery_allocations: &delivery_allocations,
        electronic_allocations: &electronic_allocations,
        service_allocations: &service_allocations,
    })?;
    if groups.is_empty() {
        return Ok(());
    }
    let mut all_fulfilled = true;
    let mut any_accepted = false;
    for group in &groups {
        let mut net_accepted = Quantity::from_str("0").unwrap();
        for fact in &group.fulfillment_facts {
            net_accepted = Quantity::try_from(
                net_accepted.to_decimal() + fact.net_accepted_allocated_quantity.to_decimal(),
            )
            .unwrap_or_else(|_| Quantity::from_str("0").unwrap());
        }
        if net_accepted != Quantity::from_str("0").unwrap() {
            any_accepted = true;
        }
        if net_accepted.to_decimal() < group.required_quantity.to_decimal() {
            all_fulfilled = false;
        }
    }
    let progress = if all_fulfilled {
        FulfillmentProgress::Completed
    } else if any_accepted {
        FulfillmentProgress::PartiallyFulfilled
    } else {
        FulfillmentProgress::NotStarted
    };
    // 履约进度变化后联动刷新回款/开票进度与关闭状态（§9.3 自动结案）
    crate::sales_order::update_sales_order_money_progress(
        db,
        session,
        sales_order_id,
        actor_id,
        Some(progress),
    )
    .await
}

#[cfg(test)]
mod tests {
    /// 过账与冲正路径不得启动审批、不得创建任务、不得选择定义。
    #[test]
    fn post_does_not_start_approval_or_create_tasks() {
        let production = include_str!("customer_acceptance_posting.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("pub async fn post_customer_acceptance"));
        assert!(production.contains("pub async fn reverse_customer_acceptance"));
        assert!(!production.contains("start_approval"));
        assert!(!production.contains("prepare_start"));
        assert!(!production.contains("WorkItem"));
        assert!(!production.contains("definition_id"));
        assert!(!production.contains("CustomerAcceptanceAdapter"));
        assert!(!production.contains("bind_published_definition_on_document_create"));
        let post = production
            .split("pub async fn post_customer_acceptance")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn reverse_customer_acceptance").next())
            .expect("post_customer_acceptance 生产片段");
        assert!(post.contains("mark_posted"));
        assert!(!post.contains("submit_"));
        assert!(!post.contains("start_approval"));
        let reverse = production
            .split("pub async fn reverse_customer_acceptance")
            .nth(1)
            .and_then(|rest| rest.split("fn ensure_post_lines_match").next())
            .expect("reverse_customer_acceptance 生产片段");
        assert!(reverse.contains("original.reverse"));
        assert!(!reverse.contains("start_approval"));
        assert!(!reverse.contains("WorkItem"));
    }
}
