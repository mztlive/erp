use database::{AccessControlExt, Executor, FulfillmentExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::fulfillment::{
    AcceptanceFulfillmentAllocation, CustomerAcceptance, CustomerAcceptanceData, CustomerAcceptanceLine,
    CustomerAcceptanceLineData,
};
use entities::ids::{CustomerAcceptanceId, CustomerAcceptanceLineId};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::approval::binding::{
    bind_published_definition_on_document_create, binding_decision, BindPublishedDefinitionCommand,
    BindingDecision,
};
use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
use crate::approval::policy::{policy_of, DocumentApprovalPolicy};
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

use super::dto::SortDir;
use super::{
    AcceptanceAllocationView, AcceptanceLineInput, CreateCustomerAcceptanceRequest,
    CustomerAcceptanceDetailView, CustomerAcceptanceLineView, CustomerAcceptanceListParams,
    CustomerAcceptanceView, FulfillmentService, PageView,
};

/// 客户验收单列表筛选条件类型。
type CustomerAcceptanceFilter = <mongodb::Database as FulfillmentExt>::CustomerAcceptanceFilter;

impl FulfillmentService {
    // ---------------------------------------------------------- customer_acceptance

    /// 分页查询客户验收单列表（W06 验收历史视图）。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_list",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_list"
        )
    )]
    pub async fn customer_acceptance_list(
        &self,
        params: &CustomerAcceptanceListParams,
    ) -> Result<PageView<CustomerAcceptanceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = CustomerAcceptanceFilter {
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .customer_acceptances()
            .search_customer_acceptances(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| CustomerAcceptanceView {
                id: row.id,
                acceptance_no: row.acceptance_no,
                sales_order_id: row.sales_order_id.to_string(),
                accepted_at: row.accepted_at.unix_secs(),
                result: row.result,
                status: row.status,
                reversal_of_acceptance_id: row.reversal_of_acceptance_id.map(|id| id.to_string()),
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询客户验收单详情（表头 + 行 + 分配）。
    ///
    /// # 参数
    /// * `id` - 验收单主键
    ///
    /// # 返回
    /// 返回验收单详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 验收单不存在
    /// * `RepositoryError` - 数据库查询失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_detail",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_detail"
        )
    )]
    pub async fn customer_acceptance_detail(&self, id: &str) -> Result<CustomerAcceptanceDetailView> {
        let acceptance = self
            .db
            .customer_acceptances()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("客户验收单不存在".to_string()))?;
        let lines = self
            .db
            .fulfillment()
            .acceptance_lines_by_acceptance_ids(&[acceptance.base.id.clone().into()], &mut NoTransaction)
            .await?;
        let line_ids: Vec<CustomerAcceptanceLineId> =
            lines.iter().map(|line| line.base.id.clone().into()).collect();
        let allocations = self
            .db
            .fulfillment()
            .allocations_by_acceptance_lines(&line_ids, &mut NoTransaction)
            .await?;
        Ok(CustomerAcceptanceDetailView {
            acceptance: acceptance.into(),
            lines: lines.into_iter().map(Into::into).collect(),
            allocations: allocations.into_iter().map(Into::into).collect(),
        })
    }

    /// 创建客户验收单（草稿，跨集合：表头 + 行 + 审计）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。客户验收为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。创建阶段不写验收分配；分配在过账时按行守恒与履约
    /// 事实上限校验后写入。
    ///
    /// # 参数
    /// * `req` - 创建请求（表头 + 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建验收单的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 单号重复（唯一索引透出）
    /// * `RepositoryError` - 数据库写入失败
    #[tracing::instrument(
        name = "fulfillment.customer_acceptance_create",
        skip_all,
        fields(
            layer = "service",
            domain = "fulfillment",
            operation = "customer_acceptance_create"
        )
    )]
    pub async fn create_customer_acceptance(
        &self,
        req: CreateCustomerAcceptanceRequest,
        actor: &AuditActor,
    ) -> Result<CustomerAcceptanceView> {
        req.validate()?;
        let id = CustomerAcceptanceId::new(next_id());
        let acceptance = CustomerAcceptance::new(
            id.clone(),
            CustomerAcceptanceData {
                acceptance_no: super::document_number::next_customer_acceptance_no(&self.db).await?,
                sales_order_id: req.sales_order_id,
                accepted_at: Instant::from_unix_secs(req.accepted_at),
                result: req.result,
            },
        )?;
        let lines = build_acceptance_lines(&id, &req.lines)?;
        persist_created_customer_acceptance(&self.db, &self.rbac, acceptance.clone(), lines, actor.clone())
            .await?;
        Ok(acceptance.into())
    }
}

/// 构建验收行实体集合（行号从 1 递增）。
///
/// # 参数
/// * `acceptance_id` - 验收单主键
/// * `inputs` - 行输入
///
/// # 返回
/// 返回行实体集合。
///
/// # 错误
/// 行数量为负或说明超长时返回错误（实体构造）。
pub(super) fn build_acceptance_lines(
    acceptance_id: &CustomerAcceptanceId,
    inputs: &[AcceptanceLineInput],
) -> Result<Vec<CustomerAcceptanceLine>> {
    let mut lines = Vec::with_capacity(inputs.len());
    for (index, input) in inputs.iter().enumerate() {
        lines.push(
            CustomerAcceptanceLine::new(
                CustomerAcceptanceLineId::new(next_id()),
                CustomerAcceptanceLineData {
                    customer_acceptance_id: acceptance_id.clone(),
                    line_no: index as u32 + 1,
                    sales_order_line_id: input.sales_order_line_id.clone(),
                    accepted_quantity: input.accepted_quantity,
                    short_quantity: input.short_quantity,
                    rejected_quantity: input.rejected_quantity,
                    reason: input.reason.clone(),
                    evidence_attachment_id: None,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    Ok(lines)
}

impl From<CustomerAcceptance> for CustomerAcceptanceView {
    /// 从验收单实体构造视图。
    fn from(acceptance: CustomerAcceptance) -> Self {
        Self {
            id: acceptance.base.id,
            acceptance_no: acceptance.acceptance_no,
            sales_order_id: acceptance.sales_order_id.to_string(),
            accepted_at: acceptance.accepted_at.unix_secs(),
            result: acceptance.result,
            status: acceptance.status,
            reversal_of_acceptance_id: acceptance.reversal_of_acceptance_id.map(|id| id.to_string()),
            version: acceptance.base.version,
            created_at: acceptance.base.created_at,
        }
    }
}

impl From<CustomerAcceptanceLine> for CustomerAcceptanceLineView {
    /// 从验收行实体构造视图。
    fn from(line: CustomerAcceptanceLine) -> Self {
        Self {
            id: line.base.id,
            line_no: line.line_no,
            sales_order_line_id: line.sales_order_line_id.to_string(),
            accepted_quantity: line.accepted_quantity,
            short_quantity: line.short_quantity,
            rejected_quantity: line.rejected_quantity,
            reason: line.reason,
        }
    }
}

impl From<AcceptanceFulfillmentAllocation> for AcceptanceAllocationView {
    /// 从验收履约分配实体构造视图。
    fn from(allocation: AcceptanceFulfillmentAllocation) -> Self {
        Self {
            id: allocation.base.id,
            customer_acceptance_line_id: allocation.customer_acceptance_line_id.to_string(),
            fulfillment_fact_type: allocation.fulfillment_fact_type,
            fulfillment_line_id: allocation.fulfillment_line_id,
            allocation_action: allocation.allocation_action,
            allocated_quantity: allocation.allocated_quantity,
            reverses_allocation_id: allocation.reverses_allocation_id.map(|id| id.to_string()),
        }
    }
}

/// 客户验收创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn customer_acceptance_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::CustomerAcceptance)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::CustomerAcceptance {
                return Err(Error::Internal("客户验收政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "客户验收必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认客户验收创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_customer_acceptance_skips_approval_binding() -> Result<BindingDecision> {
    let decision = customer_acceptance_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("客户验收创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 客户验收不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_customer_acceptance_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::CustomerAcceptance).is_ok() {
        return Err(Error::Internal("客户验收不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 验收所属销售单作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `acceptance` - 待登记客户验收单
///
/// # 返回
/// 返回非空销售单标识。
///
/// # 错误
/// 销售单为空时返回校验错误。
fn customer_acceptance_binding_organization_id(acceptance: &CustomerAcceptance) -> Result<String> {
    let org = acceptance.sales_order_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "客户验收单缺少销售单，无法构造绑定上下文".to_string(),
        ));
    }
    Ok(org)
}

/// 构造客户验收创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `acceptance` - 待登记客户验收单
/// * `creator_id` - 创建人
///
/// # 错误
/// 销售单为空时返回校验错误。
fn customer_acceptance_bind_command(
    acceptance: &CustomerAcceptance,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::CustomerAcceptance,
        business_object_id: acceptance.base.id.clone(),
        business_object_version: acceptance.base.version,
        context: BindingRevalidationContext {
            organization_id: customer_acceptance_binding_organization_id(acceptance)?,
            creator_id: creator_id.to_string(),
        },
    })
}

/// 将绑定端口返回值落实为客户验收注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 客户验收注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_customer_acceptance_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal(
            "客户验收为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("客户验收注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::CustomerAcceptance {
        return Err(Error::Internal(
            "客户验收创建只能注册 CustomerAcceptance 单据".to_string(),
        ));
    }
    Ok(None)
}

/// 在调用方事务内登记客户验收单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_customer_acceptance_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_customer_acceptance_skips_approval_binding()?;
    ensure_customer_acceptance_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_customer_acceptance_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造客户验收登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
pub(super) async fn register_created_customer_acceptance_document(
    db: &Database,
    rbac: &SharedRbacService,
    acceptance: &CustomerAcceptance,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = customer_acceptance_bind_command(acceptance, actor.id())?;
    let document = new_registered_document(
        &acceptance.base.id,
        DocumentType::CustomerAcceptance,
        acceptance.acceptance_no.clone(),
    )?;
    persist_unbound_customer_acceptance_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入客户验收草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或验收单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_customer_acceptance(
    db: &Database,
    rbac: &SharedRbacService,
    acceptance: CustomerAcceptance,
    lines: Vec<CustomerAcceptanceLine>,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "customer_acceptance.create",
        "customer_acceptance",
        acceptance.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_customer_acceptance_document(&db, &rbac, &acceptance, &actor, session)
                    .await?;
                db.fulfillment()
                    .create_customer_acceptance_with_lines(&acceptance, &lines, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::build_acceptance_lines;
    use crate::fulfillment::AcceptanceLineInput;
    use entities::ids::{CustomerAcceptanceId, SalesOrderLineId};
    use entities::money::Quantity;
    use std::str::FromStr;

    #[test]
    fn acceptance_lines_are_built_and_validated() {
        let lines = build_acceptance_lines(
            &CustomerAcceptanceId::new("acc-1"),
            &[AcceptanceLineInput {
                sales_order_line_id: SalesOrderLineId::new("so-line-1"),
                accepted_quantity: Quantity::from_str("9").unwrap(),
                short_quantity: Quantity::from_str("1").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                reason: None,
                allocations: vec![],
            }],
        )
        .unwrap();
        assert_eq!(lines[0].accepted_quantity, Quantity::from_str("9").unwrap());
    }
}

#[cfg(test)]
mod customer_acceptance_no_approval_tests {
    use super::{
        apply_customer_acceptance_create_binding, customer_acceptance_bind_command,
        customer_acceptance_create_binding_decision, ensure_customer_acceptance_has_no_adapter,
        ensure_customer_acceptance_skips_approval_binding, policy_of, BindingDecision, CustomerAcceptance,
        CustomerAcceptanceData, DocumentApprovalPolicy, DocumentType,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ids::ApprovalProcessDefinitionId;
    use bpm::ProcessKind;
    use entities::common::time::Instant;
    use entities::fulfillment::AcceptanceResult;
    use entities::ids::{CustomerAcceptanceId, SalesOrderId};

    fn draft_acceptance() -> CustomerAcceptance {
        CustomerAcceptance::new(
            CustomerAcceptanceId::new("ca-1"),
            CustomerAcceptanceData {
                acceptance_no: "CA-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                accepted_at: Instant::from_unix_secs(1_700_000_000),
                result: AcceptanceResult::Passed,
            },
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn customer_acceptance_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::CustomerAcceptance).expect("客户验收政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("客户验收必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::CustomerAcceptance);
        assert_eq!(no_approval.process_kind, ProcessKind::CustomerAcceptance);
        assert_eq!(
            customer_acceptance_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_customer_acceptance_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_customer_acceptance_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let acceptance = draft_acceptance();
        let command = customer_acceptance_bind_command(&acceptance, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::CustomerAcceptance);
        assert_eq!(command.business_object_id, acceptance.base.id);
        assert_eq!(command.context.organization_id, "so-1");

        let mut document = new_registered_document(
            &acceptance.base.id,
            DocumentType::CustomerAcceptance,
            acceptance.acceptance_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_customer_acceptance_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_customer_acceptance_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("customer_acceptance.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_customer_acceptance"));
        assert!(production.contains("register_created_customer_acceptance_document"));
        assert!(production.contains("persist_unbound_customer_acceptance_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::CustomerAcceptance"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_customer_acceptance_skips_approval_binding"));
        assert!(production.contains("ensure_customer_acceptance_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_customer_acceptance"));
        assert!(!production.contains("start_customer_acceptance_approval"));
        assert!(!production.contains("CustomerAcceptanceAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_customer_acceptance")
            .nth(1)
            .and_then(|rest| rest.split("fn build_acceptance_lines").next())
            .expect("create_customer_acceptance 生产片段");
        assert!(create.contains("persist_created_customer_acceptance"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
