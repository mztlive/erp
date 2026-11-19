//! 按当前业务事实查询读取授权的生成状态。
use erp_finance::repository::extensions::ReceivableExt;
use erp_identity::Result;
use erp_identity::entity::resource_access::SourceKey;
use erp_identity::ports::ResourceGrantFacts;
use persistence_core::Executor;

use crate::ResourceGrantProcess;
use crate::facts::Facts;

impl ResourceGrantProcess {
    /// 用当前读取事务查看账号的开票初次审批和后续转派授权，任一已核实来源待生成才返回 `true`，供业务区分权限尚未就绪与无授权依据。
    /// 无正式证据或来源已永久终止不提示等待；事实错配或读取失败直接返回错误，不能代替完整的权限检查。
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
    /// 按服务端确定的来源查找正式事件，并用同一执行器重算目标，与当前来源版本和目标对比，区分不存在、待生成、已就绪和已终止。
    /// 没有正式事件返回 `Absent`；账号失效、关联损坏或读取失败不得当作待生成，也不能以就绪结果替代访问授权。
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

        let update = Facts { db: self.db.clone() }.compute_source(&event, executor).await?;
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
