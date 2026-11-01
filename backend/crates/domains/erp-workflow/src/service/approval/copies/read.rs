//! 抄送入口只返回本次冻结资料，不复用审批办理或当前业务依据授权。
use application_core::AuditActor;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::ApprovalProcessInstance;
use chrono::Utc;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde::{Deserialize, Serialize};

use super::require_reader;
use crate::entity::approval_copy::ApprovalCopyReceipt;
use crate::entity::approval_integration::{ApprovalMaterialFile, ApprovalSubjectSnapshot};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::approval_copy::{ApprovalCopyExt, CopyListQuery, CopyPage};
use crate::repository::{ApprovalIntegrationExt, ApprovalSubjectSnapshotRepositoryExt, BpmExt};
use crate::service::approval::execution::runtime_history::{
    RuntimeHistoryPage, history_item_from_execution, history_page_from,
};
use crate::service::approval::execution::runtime_service::{
    ApprovalMaterialView, ApprovalMaterialsView, ApprovalRuntimeService,
};

/// 个人抄送详情。
#[derive(Debug, Serialize)]
pub struct CopyDetail {
    pub receipt: ApprovalCopyReceipt,
    pub instance_status: String,
    pub materials: ApprovalMaterialsView,
    pub history: RuntimeHistoryPage,
}
/// 实例读取资格内的抄送历史。
#[derive(Debug, Serialize)]
pub struct CopyHistoryPage {
    pub items: Vec<ApprovalCopyReceipt>,
    pub next_cursor: Option<String>,
}
/// 明确指定本人记录的批量已读命令。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CopyReadRequest {
    pub ids: Vec<String>,
}
impl CopyReadRequest {
    /// 校验并去重有限的勾选记录。
    /// # 参数
    /// 无。
    /// # 返回
    /// 去重后的 ID。
    /// # 错误
    /// 空集合、空 ID 或超限时拒绝。
    pub fn prepare(mut self) -> Result<Vec<String>> {
        if self.ids.is_empty()
            || self.ids.len() > 100
            || self.ids.iter().any(|id| id.trim().is_empty() || id.len() > 128 || id.trim() != id)
        {
            return Err(Error::ValidationError("请选择 1 至 100 条抄送记录".into()));
        }
        self.ids.sort();
        self.ids.dedup();
        Ok(self.ids)
    }
}

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 按当前账号读取收件箱及未读数。
    /// # 参数
    /// 认证账号和筛选。
    /// # 返回
    /// 服务端分页与完整统计。
    /// # 错误
    /// 非法筛选或无权限时拒绝。
    pub async fn copies(&self, actor: &AuditActor, mut query: CopyListQuery) -> Result<CopyPage> {
        if query.page.is_some_and(|p| p == 0 || p > 100_000)
            || query.query.as_ref().is_some_and(|q| q.len() > 128)
        {
            return Err(Error::ValidationError("抄送查询条件无效".into()));
        }
        query.query = query.query.map(|q| q.trim().to_owned()).filter(|q| !q.is_empty());
        require_reader(&self.auth, actor, &mut NoTransaction).await?;
        let page = self.db.approval_copies().page(actor.id(), &query, &mut NoTransaction).await?;
        require_reader(&self.auth, actor, &mut NoTransaction).await?;
        Ok(page)
    }

    /// 返回精确抄送的提交资料及有界历史。
    /// # 参数
    /// 认证账号、抄送 ID、可选历史游标。
    /// # 返回
    /// 只读详情，不标记已读。
    /// # 错误
    /// 越界、快照不匹配或资料缺失时拒绝。
    pub async fn copy_detail(&self, actor: &AuditActor, id: &str, after: Option<u32>) -> Result<CopyDetail> {
        let (receipt, snapshot, instance) = self.copy_subject(actor, id, &mut NoTransaction).await?;
        let mut display =
            snapshot.display.ok_or_else(|| Error::ConflictError("该审批未保存可预览资料".into()))?;
        if display.source.needs_submitter_name(&snapshot.payload.submitted_by) {
            let account = self.auth.load_account(&snapshot.payload.submitted_by, &mut NoTransaction).await?;
            display.source.resolve_submitter_name(
                &snapshot.payload.submitted_by,
                account.as_ref().map(|a| a.display_name.as_str()),
            );
        }
        display.validate()?;
        let executions = self
            .db
            .bpm_workflow()
            .list_execution_history(&snapshot.approval_process_instance_id, after, 101, &mut NoTransaction)
            .await?;
        let history = history_page_from(executions.iter().map(history_item_from_execution).collect(), 100);
        let materials = ApprovalMaterialsView {
            document_no: snapshot.payload.document_no,
            document_type: snapshot.document_type.as_str().into(),
            document_id: snapshot.business_object_id,
            subject_version: snapshot.subject_version,
            display,
            attachments: snapshot.material_files.iter().map(ApprovalMaterialView::from).collect(),
        };
        self.copy_subject(actor, id, &mut NoTransaction).await?;
        Ok(CopyDetail { receipt, instance_status: instance.status.as_str().into(), materials, history })
    }

    /// 文件读取前后用同一收件关系和冻结允许清单授权。
    /// # 参数
    /// 当前账号、抄送 ID、文件 ID。
    /// # 返回
    /// 内部冻结文件引用。
    /// # 错误
    /// 文件不属于本次提交或当前无权时拒绝。
    pub async fn copy_material_reference(
        &self,
        actor: &AuditActor,
        id: &str,
        file_id: &str,
    ) -> Result<ApprovalMaterialFile> {
        let (_, snapshot, _) = self.copy_subject(actor, id, &mut NoTransaction).await?;
        snapshot
            .material_files
            .into_iter()
            .find(|f| f.file_asset_id.as_ref() == file_id)
            .ok_or_else(copy_not_found)
    }

    /// 已有实例读者查看抄送历史；不赋予额外业务读取权。
    /// # 参数
    /// 当前账号、审批实例、分页游标。
    /// # 返回
    /// 接收人、抄送原因与阅读状态。
    /// # 错误
    /// 实例不可读或游标非法时拒绝。
    pub async fn copy_history(
        &self,
        actor: &AuditActor,
        instance: &str,
        after: Option<&str>,
    ) -> Result<CopyHistoryPage> {
        if after.is_some_and(|id| id.is_empty() || id.len() > 128) {
            return Err(Error::ValidationError("抄送历史游标无效".into()));
        }
        self.instance_detail(actor, instance).await?;
        let mut rows = self.db.approval_copies().history(instance, after, &mut NoTransaction).await?;
        let more = rows.len() > 100;
        rows.truncate(100);
        let next_cursor = if more { rows.last().map(|r| r.base.id.clone()) } else { None };
        self.instance_detail(actor, instance).await?;
        Ok(CopyHistoryPage { items: rows, next_cursor })
    }
    /// 原子标记本人勾选的抄送已读。
    /// # 参数
    /// 当前账号和明确记录集合。
    /// # 返回
    /// 本次改变的条数。
    /// # 错误
    /// 任一记录越界时整批拒绝。
    pub async fn read_copies(&self, actor: &AuditActor, request: CopyReadRequest) -> Result<u64> {
        let ids = request.prepare()?;
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |ex| {
                Box::pin(async move {
                    require_reader(&service.auth, &actor, ex).await?;
                    for id in &ids {
                        service.copy_subject(&actor, id, ex).await?;
                    }
                    Ok(service
                        .db
                        .approval_copies()
                        .mark_read(&ids, actor.id(), Utc::now().timestamp(), ex)
                        .await?)
                })
            })
            .await
    }

    async fn copy_subject(
        &self,
        actor: &AuditActor,
        id: &str,
        ex: &mut dyn Executor,
    ) -> Result<(ApprovalCopyReceipt, ApprovalSubjectSnapshot, ApprovalProcessInstance)> {
        require_reader(&self.auth, actor, ex).await?;
        let receipt =
            self.db.approval_copies().receipt(id, actor.id(), ex).await?.ok_or_else(copy_not_found)?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(&receipt.instance_id, ex)
            .await?
            .ok_or_else(copy_not_found)?;
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.instance_id), ex)
            .await?
            .ok_or_else(copy_not_found)?;
        if !receipt.matches_subject(actor.id(), &snapshot, &instance) {
            return Err(copy_not_found());
        }
        Ok((receipt, snapshot, instance))
    }
}
fn copy_not_found() -> Error {
    Error::NotFound("抄送资料不存在或当前不可读取".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_command_is_bounded_and_deduplicated() {
        assert!(CopyReadRequest { ids: vec![] }.prepare().is_err());
        assert!(CopyReadRequest { ids: vec!["id".into(); 101] }.prepare().is_err());
        assert_eq!(
            CopyReadRequest { ids: vec!["b".into(), "a".into(), "b".into()] }.prepare().unwrap(),
            vec!["a", "b"]
        );
    }
}
