//! 按当前业务事实查询读取授权的生成状态。
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::Result;
use erp_identity::entity::resource_access::SourceKey;
use persistence_core::Executor;

use crate::ResourceGrantProcess;
use crate::facts::Facts;

impl ResourceGrantProcess {
    /// 查询开票审批及后续转派的独立授权来源是否仍在生成。
    /// # 参数
    /// request_id 为申请 ID，recipient 为读取账号，executor 为当前读取事务。
    /// # 返回
    /// 任一已核实来源尚未生成时返回 true；永久终止或无证据不会提示待处理。
    /// # 错误
    /// 事实关联或读取失败时拒绝。
    pub async fn invoice_pending(
        &self,
        request_id: &str,
        recipient: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let initial = SourceKey {
            rule: "invoice_approved".into(),
            business_type: "sales_invoice_request".into(),
            business_id: request_id.into(),
            recipient: recipient.into(),
        };
        if self.source_readiness(&initial, executor).await? == GrantReadiness::NotReady {
            return Ok(true);
        }
        let Some(request) = self.db.sales_invoice_requests().find_by_id(request_id, executor).await? else {
            return Ok(false);
        };
        let Some(task_id) = request.work_item_id else {
            return Ok(false);
        };
        let reassigned = SourceKey {
            rule: "invoice_task_reassigned".into(),
            business_type: "work_item".into(),
            business_id: task_id,
            recipient: recipient.into(),
        };
        Ok(self.source_readiness(&reassigned, executor).await? == GrantReadiness::NotReady)
    }
}

/// 仅表示读取授权生成状态，不授予功能权限或办理资格。
#[derive(Debug, PartialEq, Eq)]
pub enum GrantReadiness {
    Absent,
    NotReady,
    Ready,
    Terminated,
}
impl ResourceGrantProcess {
    /// 对业务确定的来源读取正式事件，并以当前可信事实核验授权就绪。
    /// # 参数
    /// source 为服务端来源身份；executor 为当前请求事务。
    /// # 返回
    /// 没有正式事件返回 Absent；其他状态按当前来源、事实和授权结果判定。
    /// # 错误
    /// 关联损坏、账号失效或读取失败不作为待处理返回。
    pub async fn source_readiness(
        &self,
        source: &SourceKey,
        executor: &mut dyn Executor,
    ) -> Result<GrantReadiness> {
        let Some(expected) = self.queue.source_event(source, executor).await? else {
            return Ok(GrantReadiness::Absent);
        };
        let Some(event) = self.queue.event(&expected, executor).await? else {
            return Ok(GrantReadiness::Absent);
        };
        let update = Facts::compute(&self.db, &event, executor).await?;
        let source = self.queue.source(&event.source, executor).await?;
        Ok(match source {
            Some(s) if s.terminated => GrantReadiness::Terminated,
            Some(s)
                if s.fact_version == update.fact_version
                    && s.rule_version == update.rule_version
                    && s.targets == update.targets =>
            {
                GrantReadiness::Ready
            },
            _ => GrantReadiness::NotReady,
        })
    }
}
