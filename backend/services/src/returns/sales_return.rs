use super::ReturnsService;
use super::dto::{
    CreateSalesReturnCaseRequest, PageView, SalesReturnCaseListParams, SalesReturnCaseView, SortDir,
};
use crate::approval::binding::{
    BindPublishedDefinitionCommand, BindingDecision, bind_published_definition_on_document_create,
    binding_decision,
};
use crate::approval::business_adapter::{BindingRevalidationContext, adapter_spec_of};
use crate::approval::policy::{DocumentApprovalPolicy, policy_of};
use crate::audit::AuditActor;
use crate::document_registry::{new_registered_document, persist_registered_document};
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use database::{AccessControlExt, Executor, NoTransaction, ReturnsExt, Transactional};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::ids::{SalesReturnCaseId, SalesReturnLineId};
use entities::returns::{SalesReturnCase, SalesReturnCaseData, SalesReturnLine, SalesReturnLineData};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

/// 销售退货处理单列表筛选条件类型（经 `ReturnsExt` 关联类型跨 crate 可达）。
type SalesReturnCaseFilter = <mongodb::Database as ReturnsExt>::SalesReturnCaseFilter;

impl ReturnsService {
    // -----------------------------------------------------------------------
    // 销售退货/拒收处理单
    // -----------------------------------------------------------------------

    /// 分页查询销售退货/拒收处理单列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`return_no`/`sales_order_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    pub async fn sales_return_case_list(
        &self,
        params: &SalesReturnCaseListParams,
    ) -> Result<PageView<SalesReturnCaseView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SalesReturnCaseFilter {
            return_no: query.return_no,
            sales_order_id: query.sales_order_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sales_return_cases()
            .search_sales_return_cases(&filter, &mut NoTransaction)
            .await?;
        let mut views = Vec::with_capacity(page.items.len());
        for row in page.items {
            views.push(self.sales_return_case_view(row.id).await?);
        }
        Ok(PageView {
            items: views,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询销售退货/拒收处理单详情（处理单 + 明细行）。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    pub async fn sales_return_case_detail(&self, id: &str) -> Result<SalesReturnCaseView> {
        self.sales_return_case_view(id.to_string()).await
    }

    /// 建立销售退货/拒收处理单与明细行（跨集合事务写入）。
    ///
    /// 同一事务注册 `BusinessDocument` 并调用统一绑定端口。销售退货为
    /// `NO_APPROVAL`：返回空绑定，不查询已发布定义，不启动审批实例，
    /// 不创建审批任务。本期不新建正式化命令。
    ///
    /// `return_no` 全局唯一（唯一索引）构成幂等去重；同事务写入处理单与
    /// 明细行（`ReturnsRepository::create_sales_return_with_line`）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建处理单视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - 退货处理号重复
    /// * `RepositoryError` - 数据库写入失败
    pub async fn create_sales_return_case(
        &self,
        req: CreateSalesReturnCaseRequest,
        actor: &AuditActor,
    ) -> Result<SalesReturnCaseView> {
        req.validate()?;
        let (case, line) = build_sales_return_case_and_line(req, actor.id())?;
        persist_created_sales_return_case(&self.db, &self.rbac, case.clone(), line, actor.clone()).await?;
        self.sales_return_case_detail(&case.base.id).await
    }

    // -----------------------------------------------------------------------
    // 私有视图装配
    // -----------------------------------------------------------------------

    /// 装配销售退货/拒收处理单视图。
    ///
    /// # 参数
    /// * `id` - 处理单 ID
    ///
    /// # 返回
    /// 返回完整处理单视图。
    ///
    /// # 错误
    /// * `NotFound` - 处理单不存在
    async fn sales_return_case_view(&self, id: String) -> Result<SalesReturnCaseView> {
        let case = self
            .db
            .sales_return_cases()
            .find_by_id(&id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售退货处理单不存在".to_string()))?;
        let lines = self
            .db
            .sales_return_lines()
            .find_lines_by_cases(&[case.base.id.clone().into()], &mut NoTransaction)
            .await?
            .into_iter()
            .map(|line| crate::returns::dto::SalesReturnLineView {
                id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.to_string(),
                requested_quantity: line.requested_quantity,
                received_quantity: line.received_quantity,
                quality_result: line.quality_result.map(|result| result.as_str().to_string()),
                restockable_quantity: line.restockable_quantity,
            })
            .collect();
        Ok(SalesReturnCaseView {
            id: case.base.id.clone(),
            return_no: case.return_no,
            sales_order_id: case.sales_order_id.to_string(),
            acceptance_id: case.acceptance_id.map(|id| id.to_string()),
            case_type: case.case_type,
            reason: case.reason,
            discovered_at: case.discovered_at,
            return_route: case.return_route,
            status: case.stable.status(),
            version: case.base.version,
            created_at: case.base.created_at,
            lines,
        })
    }
}

/// 由创建请求构造处理单与首条明细。
///
/// 明细验收字段在创建时为空；累计有效退回数量由后续验收事务校验。
///
/// # 参数
/// * `req` - 已通过 `Validate` 的创建请求
/// * `created_by` - 创建人
///
/// # 返回
/// 返回草稿处理单与对应明细。
///
/// # 错误
/// 处理号/原因为空超长，或申请数量非正时返回校验错误。
fn build_sales_return_case_and_line(
    req: CreateSalesReturnCaseRequest,
    created_by: &str,
) -> Result<(SalesReturnCase, SalesReturnLine)> {
    let case_id = SalesReturnCaseId::new(next_id());
    let case = SalesReturnCase::new(
        case_id.clone(),
        SalesReturnCaseData {
            return_no: req.return_no,
            sales_order_id: req.sales_order_id,
            acceptance_id: req.acceptance_id,
            case_type: req.case_type,
            reason: req.reason,
            discovered_at: req.discovered_at,
            return_route: req.return_route,
        },
        created_by,
    )?;
    let line = SalesReturnLine::new(
        SalesReturnLineId::new(next_id()),
        SalesReturnLineData {
            sales_return_case_id: case_id,
            sales_order_line_id: req.lines[0].sales_order_line_id.clone(),
            requested_quantity: req.lines[0].requested_quantity,
            received_quantity: None,
            quality_result: None,
            restockable_quantity: None,
        },
    )?;
    Ok((case, line))
}

/// 销售退货创建必须跳过绑定：政策只能是 `NO_APPROVAL`。
///
/// # 返回
/// 返回 `SkipNoApproval`。
///
/// # 错误
/// 政策缺失或误登记为必须审批时返回部署不变量错误。
fn sales_return_case_create_binding_decision() -> Result<BindingDecision> {
    let policy = policy_of(DocumentType::SalesReturnCase)?;
    match &policy {
        DocumentApprovalPolicy::NoApproval(no_approval) => {
            if no_approval.document_type != DocumentType::SalesReturnCase {
                return Err(Error::Internal("销售退货政策类型不匹配".to_string()));
            }
            Ok(binding_decision(policy.requirement()))
        }
        DocumentApprovalPolicy::ProcessRequired(_) => Err(Error::Internal(
            "销售退货必须是 NO_APPROVAL，不得绑定流程".to_string(),
        )),
    }
}

/// 确认销售退货创建路径不得查询发布定义。
///
/// # 错误
/// 绑定决定不是跳过时返回错误。
fn ensure_sales_return_case_skips_approval_binding() -> Result<BindingDecision> {
    let decision = sales_return_case_create_binding_decision()?;
    if decision != BindingDecision::SkipNoApproval {
        return Err(Error::Internal("销售退货创建必须跳过审批绑定".to_string()));
    }
    Ok(decision)
}

/// 销售退货不得注册空审批适配器。
///
/// # 错误
/// 适配器登记存在时返回部署不变量错误。
fn ensure_sales_return_case_has_no_adapter() -> Result<()> {
    if adapter_spec_of(DocumentType::SalesReturnCase).is_ok() {
        return Err(Error::Internal("销售退货不得注册审批适配器".to_string()));
    }
    Ok(())
}

/// 原销售单作为绑定上下文组织，不得用空串补位。
///
/// # 参数
/// * `case` - 待登记销售退货处理单
///
/// # 返回
/// 返回非空销售单标识。
///
/// # 错误
/// 销售单为空时返回校验错误。
fn sales_return_case_binding_organization_id(case: &SalesReturnCase) -> Result<String> {
    let org = case.sales_order_id.to_string();
    if org.trim().is_empty() {
        return Err(Error::ValidationError(
            "销售退货缺少原销售单，无法构造绑定上下文".to_string(),
        ));
    }
    Ok(org)
}

/// 构造销售退货创建绑定命令。客户端不得提交定义 ID。
///
/// # 参数
/// * `case` - 待登记销售退货处理单
/// * `creator_id` - 创建人
///
/// # 错误
/// 原销售单为空时返回校验错误。
fn sales_return_case_bind_command(
    case: &SalesReturnCase,
    creator_id: &str,
) -> Result<BindPublishedDefinitionCommand> {
    Ok(BindPublishedDefinitionCommand {
        document_type: DocumentType::SalesReturnCase,
        business_object_id: case.base.id.clone(),
        business_object_version: case.base.version,
        context: BindingRevalidationContext {
            organization_id: sales_return_case_binding_organization_id(case)?,
            creator_id: creator_id.to_string(),
        },
    })
}

/// 将绑定端口返回值落实为销售退货注册行：空绑定保持未绑定。
///
/// # 参数
/// * `document` - 销售退货注册行
/// * `binding` - 统一绑定端口返回值
///
/// # 返回
/// 固定返回 `None`。
///
/// # 错误
/// 端口返回绑定或注册行已预置绑定时返回错误。
fn apply_sales_return_case_create_binding(
    document: &mut BusinessDocument,
    binding: Option<ApprovalDefinitionBinding>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    if binding.is_some() {
        return Err(Error::Internal(
            "销售退货为 NO_APPROVAL，不得写入审批绑定".to_string(),
        ));
    }
    if document.approval_binding.is_some() {
        return Err(Error::Internal("销售退货注册行不得预置审批绑定".to_string()));
    }
    if document.document_type != DocumentType::SalesReturnCase {
        return Err(Error::Internal(
            "销售退货创建只能注册 SalesReturnCase 单据".to_string(),
        ));
    }
    Ok(None)
}

/// 在调用方事务内登记销售退货单据并证明空绑定。
///
/// 必须先确认政策跳过，再调用统一绑定端口；不得查询发布定义后假装成功。
///
/// # 错误
/// 政策非无审批、端口返回绑定或写入失败时返回错误。
async fn persist_unbound_sales_return_document(
    db: &Database,
    rbac: &SharedRbacService,
    mut document: BusinessDocument,
    bind_command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = ensure_sales_return_case_skips_approval_binding()?;
    ensure_sales_return_case_has_no_adapter()?;
    let binding =
        bind_published_definition_on_document_create(db, rbac, bind_command, actor, executor).await?;
    apply_sales_return_case_create_binding(&mut document, binding)?;
    persist_registered_document(db, &document, executor).await
}

/// 为已构造销售退货登记 `BusinessDocument` 并调用统一绑定端口。
///
/// # 错误
/// 绑定端口或注册写入失败时返回错误。
async fn register_created_sales_return_document(
    db: &Database,
    rbac: &SharedRbacService,
    case: &SalesReturnCase,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let bind_command = sales_return_case_bind_command(case, actor.id())?;
    let document = new_registered_document(
        &case.base.id,
        DocumentType::SalesReturnCase,
        case.return_no.clone(),
    )?;
    persist_unbound_sales_return_document(db, rbac, document, &bind_command, actor, executor).await
}

/// 在创建事务内写入销售退货草稿并登记无绑定单据。
///
/// # 错误
/// 绑定、注册或处理单写入失败时返回错误，调用方必须视作整体回滚。
async fn persist_created_sales_return_case(
    db: &Database,
    rbac: &SharedRbacService,
    case: SalesReturnCase,
    line: SalesReturnLine,
    actor: AuditActor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "sales_return_case.create",
        "sales_return_case",
        case.base.id.clone(),
    )?;
    let db = db.clone();
    let rbac = rbac.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                register_created_sales_return_document(&db, &rbac, &case, &actor, session).await?;
                db.returns()
                    .create_sales_return_with_line(&case, &line, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod sales_return_case_no_approval_tests {
    use super::{
        BindingDecision, DocumentApprovalPolicy, DocumentType, SalesReturnCase, SalesReturnCaseData,
        apply_sales_return_case_create_binding, ensure_sales_return_case_has_no_adapter,
        ensure_sales_return_case_skips_approval_binding, policy_of, sales_return_case_bind_command,
        sales_return_case_create_binding_decision,
    };
    use crate::approval::binding::binding_from_published;
    use crate::document_registry::new_registered_document;
    use bpm::ProcessKind;
    use bpm::ids::ApprovalProcessDefinitionId;
    use entities::common::time::Instant;
    use entities::ids::{SalesOrderId, SalesReturnCaseId};
    use entities::returns::{CaseType, ReturnRoute};

    fn draft_case() -> SalesReturnCase {
        SalesReturnCase::new(
            SalesReturnCaseId::new("src-1"),
            SalesReturnCaseData {
                return_no: "SR-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                acceptance_id: None,
                case_type: CaseType::Return,
                reason: "破损".into(),
                discovered_at: Instant::from_unix_secs(1_700_000_000),
                return_route: ReturnRoute::CompanyWarehouse,
            },
            "admin-1",
        )
        .expect("草稿必须可构造")
    }

    /// 政策仅含 document_type、approval_requirement、process_kind，不得注册空 Adapter。
    #[test]
    fn sales_return_case_policy_is_no_approval_identity_only() {
        let policy = policy_of(DocumentType::SalesReturnCase).expect("销售退货政策必须存在");
        let DocumentApprovalPolicy::NoApproval(no_approval) = &policy else {
            panic!("销售退货必须是 NO_APPROVAL");
        };
        assert_eq!(no_approval.document_type, DocumentType::SalesReturnCase);
        assert_eq!(no_approval.process_kind, ProcessKind::SalesReturnCase);
        assert_eq!(
            sales_return_case_create_binding_decision().expect("绑定决定"),
            BindingDecision::SkipNoApproval
        );
        assert_eq!(
            ensure_sales_return_case_skips_approval_binding().expect("必须跳过"),
            BindingDecision::SkipNoApproval
        );
        ensure_sales_return_case_has_no_adapter().expect("不得注册空适配器");
    }

    /// 创建必须注册 BusinessDocument，绑定端口返回空，禁止写入绑定。
    #[test]
    fn create_registers_document_and_returns_empty_binding() {
        let case = draft_case();
        let command = sales_return_case_bind_command(&case, "admin-1").expect("绑定命令");
        assert_eq!(command.document_type, DocumentType::SalesReturnCase);
        assert_eq!(command.business_object_id, case.base.id);
        assert_eq!(command.context.organization_id, "so-1");

        let mut document = new_registered_document(
            &case.base.id,
            DocumentType::SalesReturnCase,
            case.return_no.clone(),
        )
        .expect("可注册");
        assert!(document.approval_binding.is_none());
        let empty = apply_sales_return_case_create_binding(&mut document, None).expect("空绑定");
        assert!(empty.is_none());
        assert!(document.approval_binding.is_none());

        let forged = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .expect("测试绑定");
        assert!(apply_sales_return_case_create_binding(&mut document, Some(forged)).is_err());
    }

    /// 创建路径调用统一绑定端口，不查询发布定义、不启动实例、不建任务。
    #[test]
    fn create_does_not_query_definition_or_start_instance() {
        let production = include_str!("sales_return.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(production.contains("persist_created_sales_return_case"));
        assert!(production.contains("register_created_sales_return_document"));
        assert!(production.contains("persist_unbound_sales_return_document"));
        assert!(production.contains("bind_published_definition_on_document_create"));
        assert!(production.contains("DocumentType::SalesReturnCase"));
        assert!(production.contains("new_registered_document"));
        assert!(production.contains("ensure_sales_return_case_skips_approval_binding"));
        assert!(production.contains("ensure_sales_return_case_has_no_adapter"));
        assert!(!production.contains("pub async fn submit_sales_return_case"));
        assert!(!production.contains("start_sales_return_approval"));
        assert!(!production.contains("SalesReturnCaseAdapter"));
        assert!(!production.contains("load_published_graph"));
        let create = production
            .split("pub async fn create_sales_return_case")
            .nth(1)
            .and_then(|rest| rest.split("async fn sales_return_case_view").next())
            .expect("create_sales_return_case 生产片段");
        assert!(create.contains("persist_created_sales_return_case"));
        assert!(!create.contains("prepare_start"));
        assert!(!create.contains("attach_published_binding"));
        assert!(!create.contains("WorkItem"));
        assert!(!create.contains("start_approval"));
    }
}
