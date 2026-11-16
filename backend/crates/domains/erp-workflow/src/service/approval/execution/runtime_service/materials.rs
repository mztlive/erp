//! 只返回审批提交时冻结的展示与材料元数据；不读取当前业务详情。

use application_core::AuditActor;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::{ApprovalRuntimeService, hidden_not_found};
use crate::entity::approval_integration::ApprovalMaterialFile;
use crate::entity::approval_integration::display_snapshot::ApprovalDisplaySnapshot;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;

/// 对外公开的审批材料引用；版本和内容指纹仅供内部访问重验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalMaterialView {
    pub file_asset_id: String,
    pub file_name: String,
    pub content_type: String,
    pub byte_size: u64,
}

impl From<&ApprovalMaterialFile> for ApprovalMaterialView {
    fn from(file: &ApprovalMaterialFile) -> Self {
        Self {
            file_asset_id: file.file_asset_id.to_string(),
            file_name: file.file_name.clone(),
            content_type: file.content_type.clone(),
            byte_size: file.byte_size,
        }
    }
}

/// 当前账号可读取的同一提交版本资料，不含当前草稿或存储定位信息。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApprovalMaterialsView {
    pub document_no: String,
    pub document_type: String,
    pub document_id: String,
    pub subject_version: u32,
    pub display: ApprovalDisplaySnapshot,
    pub attachments: Vec<ApprovalMaterialView>,
}

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 在精确实例读取授权后返回冻结展示及允许材料目录。
    ///
    /// # 参数
    /// * `actor` - 当前认证账号。
    /// * `instance_id` - 审批实例 ID。
    ///
    /// # 返回
    /// 返回提交时冻结的版本、展示与材料安全元数据。
    ///
    /// # 错误
    /// 无权、主体链不一致、展示或材料校验失败，或旧实例未保存展示快照时拒绝，不补读当前业务数据。
    pub async fn materials(&self, actor: &AuditActor, instance_id: &str) -> Result<ApprovalMaterialsView> {
        let service = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.materials_with(&actor, &instance_id, executor).await })
            })
            .await
    }

    /// 在调用方的原快照中授权并读取冻结审批资料。
    /// # 参数
    /// actor、instance_id 为真实账号及精确实例，executor 为完整读取用例的活动快照事务。
    /// # 返回
    /// 返回当前实例授权下的冻结资料，供调用方沿原事务继续补齐结果。
    /// # 错误
    /// 非快照执行器、实例无权、冻结主体不一致或读取失败时拒绝。
    pub async fn materials_with(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalMaterialsView> {
        if !executor
            .boundary()
            .is_some_and(|boundary| boundary.is_snapshot() && boundary.is_current(executor))
        {
            return Err(Error::ValidationError("审批资料读取必须使用快照事务".into()));
        }
        self.ensure_ordinary_runtime_read_stage(actor, instance_id, executor).await?;
        let subject = self.load_runtime_read_subject_stage(instance_id, executor).await?;
        let snapshot = subject.snapshot;
        let mut display =
            snapshot.display.ok_or_else(|| Error::ConflictError("该审批未保存可预览资料".into()))?;
        if display.source.needs_submitter_name(&snapshot.payload.submitted_by) {
            let account = self.identity.directory().account(&snapshot.payload.submitted_by, executor).await?;
            display.source.resolve_submitter_name(
                &snapshot.payload.submitted_by,
                account.as_ref().map(|account| account.display_name.as_str()),
            );
        }
        display.validate()?;
        let mut attachments = Vec::new();
        for file in &snapshot.material_files {
            file.validate()?;
            attachments.push(ApprovalMaterialView::from(file));
        }
        Ok(ApprovalMaterialsView {
            document_no: snapshot.payload.document_no,
            document_type: snapshot.document_type.as_str().into(),
            document_id: snapshot.business_object_id,
            subject_version: snapshot.subject_version,
            display,
            attachments,
        })
    }

    /// 验证精确实例访问及冻结允许清单，返回仅供服务器比对的原始文件引用。
    ///
    /// # 参数
    /// * `actor` - 当前读者。
    /// * `instance_id` - 审批实例 ID。
    /// * `file_id` - 请求预览的文件资产 ID。
    ///
    /// # 返回
    /// 返回冻结元数据；调用方必须校验当前文件的版本、指纹及状态后才读取内容。
    ///
    /// # 错误
    /// 未授权、文件不在冻结允许清单或找不到该文件时隐藏为不存在；仓储失败时返回对应错误。
    pub async fn material_reference(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        file_id: &str,
    ) -> Result<ApprovalMaterialFile> {
        let service = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_owned();
        let file_id = file_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.material_reference_stage(&actor, &instance_id, &file_id, executor).await
                })
            })
            .await
    }

    /// 文件允许清单检查与实例读取资格使用同一事务。
    async fn material_reference_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        file_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalMaterialFile> {
        self.ensure_ordinary_runtime_read_stage(actor, instance_id, executor).await?;
        let subject = self.load_runtime_read_subject_stage(instance_id, executor).await?;
        if !subject.snapshot.allows_runtime_material(
            instance_id,
            subject.document_type,
            subject.instance.subject.subject_id(),
            subject.instance.subject_version,
            file_id,
        ) {
            return Err(hidden_not_found());
        }
        subject
            .snapshot
            .material_files
            .into_iter()
            .find(|file| file.file_asset_id.as_ref() == file_id)
            .ok_or_else(hidden_not_found)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::FileAssetId;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::FailClosedWorkflowAuthorizationPort;
    use crate::service::approval::definition::test_support::unavailable_service;

    #[tokio::test]
    async fn shared_materials_read_requires_snapshot_and_authorizes_before_loading_subject() {
        let (definition, actor) = unavailable_service().await;
        let runtime = ApprovalRuntimeService::new(
            definition.db,
            definition.identity,
            definition.authorization,
            FailClosedWorkflowAuthorizationPort,
        );
        assert!(matches!(
            runtime.materials_with(&actor, "instance", &mut NoTransaction).await,
            Err(Error::ValidationError(reason)) if reason == "审批资料读取必须使用快照事务"
        ));
        assert!(matches!(
            runtime.materials_with(&actor, "instance", &mut SnapshotExecutor::default()).await,
            Err(Error::Internal(reason)) if reason == "测试授权设施不可用"
        ));
    }

    #[test]
    fn public_material_projection_omits_internal_fingerprint_and_version() {
        let file = ApprovalMaterialFile {
            file_asset_id: FileAssetId::new("proof"),
            file_name: "proof.pdf".into(),
            content_type: "application/pdf".into(),
            byte_size: 1,
            asset_version: 2,
            content_hmac: "a".repeat(64),
        };
        let value = serde_json::to_value(ApprovalMaterialView::from(&file)).unwrap();
        assert_eq!(value["file_asset_id"], "proof");
        assert!(value.get("content_hmac").is_none());
        assert!(value.get("asset_version").is_none());
    }
}
