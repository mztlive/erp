//! 公开询价命令统一进入原子执行器。

use application_core::AuditActor;

use super::SalesInquiryProcess;
use super::dto::*;
use super::execution::InquiryCommand;
use super::fee_allocation::AllocateInquiryFeesCommand;
use crate::Result;

impl SalesInquiryProcess {
    /// 明确已接受费用的关联销售及分摊金额。
    ///
    /// `actor` 为当前销售，`id` 为询价，`req` 为版本、命令键及分摊内容。原子持久化后的安全命令回执。销售来源、状态、费用范围或余额不满足时拒绝。
    pub async fn allocate_fees(
        &self,
        actor: &AuditActor,
        id: &str,
        req: AllocateInquiryFeesCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::AllocateFees(req)).await
    }
    /// 销售开始录入需求时创建询价草稿，不要求既有询价 ID；按当前账号、原幂等键和载荷返回冻结的命令结果。
    /// 权限、责任、版本、状态或业务校验不通过时拒绝，事务失败保留原分类。
    pub async fn create(&self, actor: &AuditActor, req: CreateInquiryCommand) -> Result<InquiryMutationView> {
        self.execute(actor, "", InquiryCommand::Create(req)).await
    }

    /// 销售补录已有询价时，按询价身份保存完整工作副本，按原幂等键同载荷返回冻结结果。
    /// 当前权限、责任、版本、状态或业务规则不符时拒绝；事务错误保持原分类。
    pub async fn save(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Save(req)).await
    }

    /// 销售将询价交给采购时，冻结当前需求并分派采购责任；原幂等键及载荷一致时只返回原命令结果。
    /// 权限、责任、需求版本、状态或业务规则不符时拒绝，事务错误保持原分类。
    pub async fn submit(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SubmitInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Submit(req)).await
    }

    /// 采购发现当前需求不足时按任务请求退回销售补充，并冻结旧采购任务。
    /// 原幂等键同载荷返回冻结结果；权限、任务责任、版本、状态或业务规则不符时拒绝，事务错误保持原分类。
    pub async fn request_supplement(
        &self,
        actor: &AuditActor,
        id: &str,
        req: InquiryTaskReasonCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Supplement(req)).await
    }

    /// 采购在发布前暂存当前任务责任行的工作反馈。按原幂等键和相同载荷重放时只返回冻结结果。
    /// 权限、任务责任、版本、状态或内容不符合规则时拒绝，事务错误保持原分类。
    pub async fn save_feedback(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryFeedbackCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::SaveFeedback(req)).await
    }

    /// 采购完成反馈后，合并各责任结果并发布完整一轮反馈供销售处理；原幂等键和载荷一致时返回原冻结结果。
    /// 权限、任务责任、版本、状态或业务规则不符合时拒绝，事务错误保持原分类。
    pub async fn publish(
        &self,
        actor: &AuditActor,
        id: &str,
        req: PublishInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Publish(req)).await
    }

    /// 销售确认采购反馈时接受请求指向的确切轮次，不以其他轮次替代。按原幂等键和相同载荷返回原冻结结果。
    /// 权限、责任、版本、状态或业务规则不符时拒绝，事务错误保持原分类。
    pub async fn accept(
        &self,
        actor: &AuditActor,
        id: &str,
        req: AcceptInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Accept(req)).await
    }

    /// 销售需要再次询价时，提交重询并为新一轮处理创建责任任务；同一原幂等键和载荷只交付原冻结结果。
    /// 权限、责任、版本、状态或业务规则不符时拒绝，事务错误保持原分类。
    pub async fn reinquire(
        &self,
        actor: &AuditActor,
        id: &str,
        req: ReinquireInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Reinquire(req)).await
    }

    /// 销售终止询价时关闭该询价及全部开放任务；按原幂等键和相同载荷重试只返回原冻结结果。
    /// 权限、责任、版本、状态或业务规则不符时拒绝，事务错误保持原分类。
    pub async fn close(
        &self,
        actor: &AuditActor,
        id: &str,
        req: InquiryReasonCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Close(req)).await
    }

    /// 询价需更换采购责任人时，按当前已识别的采购任务执行受控改派。
    /// 权限、任务责任、版本、状态或业务规则不符时拒绝，事务错误保持原分类；原幂等键和载荷一致时返回原冻结结果。
    pub async fn reassign(
        &self,
        actor: &AuditActor,
        id: &str,
        req: ReassignInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Reassign(req)).await
    }
}
