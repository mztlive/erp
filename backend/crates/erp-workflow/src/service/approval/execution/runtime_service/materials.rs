//! 只返回审批提交时冻结的展示与材料元数据；不读取当前业务详情。

use application_core::AuditActor;
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
    /// # 返回
    /// 返回提交时冻结的版本、展示与材料安全元数据。
    /// # 错误
    /// 无权、主体链不一致或旧实例未保存展示快照时拒绝，不补读当前业务数据。
    pub async fn materials(&self, actor: &AuditActor, instance_id: &str) -> Result<ApprovalMaterialsView> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
        let snapshot = subject.snapshot;
        let display =
            snapshot.display.ok_or_else(|| Error::ConflictError("该审批未保存可预览资料".into()))?;
        display.validate()?;
        let mut attachments = Vec::new();
        for file in &snapshot.material_files {
            file.validate()?;
            attachments.push(ApprovalMaterialView::from(file));
        }
        Ok(ApprovalMaterialsView {
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
    /// * `actor` / `instance_id` - 当前读者与审批实例。
    /// * `file_id` - 请求预览的文件资产 ID。
    /// # 返回
    /// 返回冻结元数据；调用方必须校验当前文件的版本、指纹及状态后才读取内容。
    /// # 错误
    /// 未授权、跨实例/版本文件、历史未冻结材料时拒绝。
    pub async fn material_reference(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        file_id: &str,
    ) -> Result<ApprovalMaterialFile> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
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

    use super::*;

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
