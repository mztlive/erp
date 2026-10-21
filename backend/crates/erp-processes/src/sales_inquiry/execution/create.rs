//! 创建可信客户、负责人和业务组织头信息。

use application_core::AuditActor;
use erp_customer::{CustomerAccountStatus, CustomerExt};
use erp_party::PartyExt;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use id_generator::next_id;
use persistence_core::Executor;

use crate::adapters::customer_access;
use crate::business_ownership::required_business_org;
use crate::sales_inquiry::SalesInquiryProcess;
use crate::sales_inquiry::access::SalesInquiryAccess;
use crate::sales_inquiry::dto::CreateInquiryCommand;
use crate::sales_inquiry::eligibility::normalize_requirement;
use crate::{Error, Result};

impl SalesInquiryProcess {
    /// 在调用方事务内创建带可信客户及责任范围的草稿。
    /// # 参数
    /// `actor` 为当前操作者，`req` 为创建数据，`executor` 为用例事务。
    /// # 返回
    /// 返回已持久化的询价草稿。
    /// # 错误
    /// 客户、商品或创建权限无效及存储失败时拒绝。
    pub(crate) async fn create_inquiry(
        &self,
        actor: &AuditActor,
        req: &CreateInquiryCommand,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let access = self.access().resolve_selection(actor, "create", executor).await?;
        let mut data = req.data.clone();
        data.sales_owner_user_id = actor.id().to_string();
        data.business_org_unit_id = required_business_org(&self.db, actor.id(), executor).await?;
        data.customer_name = self.require_customer(actor, &data.customer_id, executor).await?;
        if let Some(source) = &data.copied_from_inquiry_id {
            self.access().require_sales(actor, "detail", source, executor).await?;
        }
        normalize_requirement(&self.db, &mut data.draft, executor).await?;
        let id = next_id();
        let inquiry = SalesInquiry::create(id.clone(), format!("IQ{id}"), data.clone())?;
        if !SalesInquiryAccess::allows_sales(&access, &inquiry) {
            return Err(Error::Forbidden("没有该业务责任范围的询价创建权限".into()));
        }
        Ok(SalesInquiryService::new(self.db.clone())
            .create(id.clone(), format!("IQ{id}"), data, executor)
            .await?)
    }

    /// 读取当前授权且有效的客户名称。
    /// # 参数
    /// `actor`、`id` 为访问身份与客户，`executor` 为当前读取边界。
    /// # 返回
    /// 返回客户当前主体修订的法定名称。
    /// # 错误
    /// 客户越权、停用或缺少有效主体资料时拒绝。
    pub(crate) async fn require_customer(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        customer_access(self.db.clone(), self.rbac.clone()).require_current(actor, id, executor).await?;
        let customer = self
            .db
            .customer_accounts()
            .find_by_id(id, executor)
            .await?
            .filter(|customer| customer.stable.status == CustomerAccountStatus::Active)
            .ok_or_else(|| Error::NotFound("客户不存在或当前不可选用".into()))?;
        let party = self
            .db
            .parties()
            .find_by_id(customer.party_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户主体不存在".into()))?;
        let revision_id = party
            .stable
            .current_revision_id
            .ok_or_else(|| Error::BusinessLogicError("客户主体缺少有效名称".into()))?;
        let revision = self
            .db
            .party_revisions()
            .find_by_id(&revision_id, executor)
            .await?
            .ok_or_else(|| Error::BusinessLogicError("客户主体修订不存在".into()))?;
        Ok(revision.legal_name)
    }
}
