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
    /// # 参数
    /// `actor` 为当前销售，`id` 为询价，`req` 为版本、命令键及分摊内容。
    /// # 返回
    /// 原子持久化后的安全命令回执。
    /// # 错误
    /// 销售来源、状态、费用范围或余额不满足时拒绝。
    pub async fn allocate_fees(
        &self,
        actor: &AuditActor,
        id: &str,
        req: AllocateInquiryFeesCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::AllocateFees(req)).await
    }
    /// 创建询价草稿。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn create(&self, actor: &AuditActor, req: CreateInquiryCommand) -> Result<InquiryMutationView> {
        self.execute(actor, "", InquiryCommand::Create(req)).await
    }

    /// 保存完整询价工作副本。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn save(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Save(req)).await
    }

    /// 冻结需求并分派当前采购责任。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn submit(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SubmitInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Submit(req)).await
    }

    /// 当前采购责任人接手询价。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn claim(
        &self,
        actor: &AuditActor,
        id: &str,
        req: InquiryTaskCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Claim(req)).await
    }

    /// 退回销售补充并冻结旧采购任务。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn request_supplement(
        &self,
        actor: &AuditActor,
        id: &str,
        req: InquiryTaskReasonCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Supplement(req)).await
    }

    /// 保存当前责任行的采购工作草稿。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn save_feedback(
        &self,
        actor: &AuditActor,
        id: &str,
        req: SaveInquiryFeedbackCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::SaveFeedback(req)).await
    }

    /// 合并各责任结果并发布完整反馈。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn publish(
        &self,
        actor: &AuditActor,
        id: &str,
        req: PublishInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Publish(req)).await
    }

    /// 销售接受确切反馈轮次。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn accept(
        &self,
        actor: &AuditActor,
        id: &str,
        req: AcceptInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Accept(req)).await
    }

    /// 销售提交重询并创建新责任任务。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn reinquire(
        &self,
        actor: &AuditActor,
        id: &str,
        req: ReinquireInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Reinquire(req)).await
    }

    /// 关闭询价及全部开放任务。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn close(
        &self,
        actor: &AuditActor,
        id: &str,
        req: InquiryReasonCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Close(req)).await
    }

    /// 受控改派当前采购任务。
    ///
    /// # 参数
    /// `actor` 为当前账号，`req` 携带幂等键及业务内容；更新同时传入询价身份。
    /// # 返回
    /// 返回冻结的安全命令回执；同键同载荷返回原结果。
    /// # 错误
    /// 权限、责任、版本、状态或业务规则失效时拒绝；事务错误保持原分类。
    pub async fn reassign(
        &self,
        actor: &AuditActor,
        id: &str,
        req: ReassignInquiryCommand,
    ) -> Result<InquiryMutationView> {
        self.execute(actor, id, InquiryCommand::Reassign(req)).await
    }
}
