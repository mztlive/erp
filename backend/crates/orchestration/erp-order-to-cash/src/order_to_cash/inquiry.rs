//! 询价转销售加入现有建单事务，来源数量与销售草稿共同提交。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::SharedRbacService;
use erp_sales::dto::sales_order::inquiry_basis::InquirySalesBasis;
use erp_sales::dto::sales_order::{CreateSalesOrderRequest, SalesOrderEditableDraftRequest};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::entity::sales_order::inquiry_source::{InquirySalesSourceRequest, SalesOrderInquirySource};
use erp_sales::entity::sales_order::{BusinessType, SalesOrder, SalesOrderWorkingCopyLine};
use erp_sales::repository::sales_order::inquiry_source::inquiry_sources;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::SalesOrderCommandProcess;
use crate::sales_inquiry::access::SalesInquiryAccess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 读取当前已接受结果的安全开单依据与可转数量，提交时继续在事务内重验。
    /// # 参数
    /// `inquiry_id`、`round` 为明确来源，`actor` 为当前操作人。
    /// # 返回
    /// 返回不含供应商和内部报价身份的商品、地址和供给参考价。
    /// # 错误
    /// 无转销售权限、已关闭、未接受或无适用可转量时拒绝。
    pub async fn inquiry_sales_basis(
        &self,
        inquiry_id: &str,
        round: u32,
        actor: &AuditActor,
    ) -> Result<InquirySalesBasis> {
        let inquiry = SalesInquiryAccess::new(self.db.clone(), self.require_rbac()?.clone())
            .require_sales(actor, "convert_sales", inquiry_id, &mut NoTransaction)
            .await?;
        Ok(InquirySalesBasis::from_inquiry(&inquiry, round, Instant::now())?)
    }

    /// 在原建单准备阶段复制已接受的需求；正式写入仍须在事务内重验。
    /// # 参数
    /// `request` 为完整销售命令，`actor` 为当前销售，`executor` 为准备阶段执行器。
    /// # 返回
    /// 普通开单返回原草稿，询价开单返回服务端冻结需求的草稿。
    /// # 错误
    /// 无询价转单资格、客户不一致、未接受或来源过期时拒绝。
    pub(super) async fn inquiry_sales_draft(
        &self,
        request: &CreateSalesOrderRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesOrderEditableDraftRequest> {
        let Some(source) = request.inquiry_source.as_ref() else {
            return Ok(request.draft.clone());
        };
        if request.business_type != BusinessType::GoodsService || request.selection_proposal_id.is_some() {
            return Err(Error::ValidationError(
                "询价来源只适用于实物与服务销售，不能同时使用选品方案来源".into(),
            ));
        }
        let inquiry = SalesInquiryAccess::new(self.db.clone(), self.require_rbac()?.clone())
            .require_sales(actor, "convert_sales", &source.inquiry_id, executor)
            .await?;
        if request.customer_id.as_ref().is_some_and(|id| id.as_ref() != inquiry.customer_id) {
            return Err(Error::ValidationError("销售客户必须与询价客户一致".into()));
        }
        Ok(request.draft.with_inquiry_source(&inquiry, source, Instant::now())?)
    }
}

/// 在现有建单执行器内重验询价、占用数量并保存稳定销售来源。
/// # 参数
/// `db`、`rbac` 为组合根依赖；`request` 为明确来源；`order` 与 `lines` 为服务端销售事实。
/// # 返回
/// 数量台账与销售来源已写入同一事务。
/// # 错误
/// 权限、已接受结果、客户、版本或剩余数量不匹配时整单拒绝。
pub(super) async fn reserve_inquiry_creation(
    db: &Database,
    rbac: &SharedRbacService,
    request: Option<&InquirySalesSourceRequest>,
    order: &SalesOrder,
    lines: &[SalesOrderWorkingCopyLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(request) = request else {
        return Ok(());
    };
    let mut inquiry = SalesInquiryAccess::new(db.clone(), rbac.clone())
        .require_sales(actor, "convert_sales", &request.inquiry_id, executor)
        .await?;
    if order.customer_id.as_ref() != inquiry.customer_id {
        return Err(Error::ValidationError("销售客户必须与询价客户一致".into()));
    }
    let source = SalesOrderInquirySource::from_inquiry(
        order.base.id.clone(),
        &inquiry,
        request,
        lines,
        Instant::now(),
    )?;
    reserve(&mut inquiry, &source)?;
    SalesInquiryService::new(db.clone()).save(&mut inquiry, executor).await?;
    inquiry_sources(db).create(&source, executor).await?;
    Ok(())
}

fn reserve(inquiry: &mut SalesInquiry, source: &SalesOrderInquirySource) -> Result<()> {
    inquiry.reserve_quantities(
        &source.base.id,
        &source.base.id,
        source.feedback_round,
        source.selections(),
        Instant::now(),
    )?;
    Ok(())
}
