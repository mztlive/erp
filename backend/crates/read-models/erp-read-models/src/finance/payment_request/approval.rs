//! 提交前展示正式发布路线，已经绑定的申请只读取其冻结定义。

use bpm::ProcessKind;
use bpm::ids::ApprovalProcessDefinitionId;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_workflow::BpmExt;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::{BusinessDocument, DocumentType};
use erp_workflow::service::document_registry::find_registered_document;
use persistence_core::Executor;

use super::PurchasePaymentRequestReadService;
use crate::finance::dto::{DocumentApprovalDefinitionView, DocumentApprovalNodeView};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    pub(super) async fn request_approval_binding(
        &self,
        request: &PurchasePaymentRequest,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalDefinitionBinding>> {
        let document = find_registered_document(&self.db, &request.base.id, executor).await?;
        registered_binding(&request.base.id, request.status, request.submission_version, document)
    }

    pub(super) async fn approval_definition(
        &self,
        binding: Option<&ApprovalDefinitionBinding>,
        executor: &mut dyn Executor,
    ) -> Result<Option<DocumentApprovalDefinitionView>> {
        let (id, version) = if let Some(binding) = binding {
            (binding.approval_process_definition_id.clone(), binding.approval_definition_version)
        } else {
            let Some(definition) = self
                .db
                .bpm_workflow()
                .find_published_by_process_kind(ProcessKind::PurchasePaymentRequest, executor)
                .await?
            else {
                return Ok(None);
            };
            (ApprovalProcessDefinitionId::new(definition.base.id), definition.definition_version)
        };
        let mut graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&id, executor)
            .await?
            .filter(|graph| graph.definition.definition_version == version)
            .ok_or_else(|| Error::ConflictError("付款申请审批定义与冻结版本不一致".into()))?;
        graph.nodes.sort_by_key(|node| node.display_order);
        Ok(Some(
            DocumentApprovalDefinitionView::new(id.to_string(), graph.definition.name)
                .with_version(version)
                .with_nodes(graph.nodes.iter().map(DocumentApprovalNodeView::from_node).collect()),
        ))
    }
}

fn registered_binding(
    id: &str,
    status: PurchasePaymentRequestStatus,
    submission_version: u32,
    document: Option<BusinessDocument>,
) -> Result<Option<ApprovalDefinitionBinding>> {
    let Some(document) = document else {
        if status == PurchasePaymentRequestStatus::Draft && submission_version == 0 {
            return Ok(None);
        }
        return Err(Error::ConflictError("已提交付款申请缺少审批登记".into()));
    };
    if document.base.id != id || document.document_type != DocumentType::PurchasePaymentRequest {
        return Err(Error::ConflictError("付款申请与审批登记身份不一致".into()));
    }
    if submission_version != 0 && document.approval_binding.is_none() {
        return Err(Error::ConflictError("已提交付款申请缺少冻结审批定义".into()));
    }
    Ok(document.approval_binding)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_workflow::service::document_registry::new_registered_document;

    use super::*;

    #[test]
    fn initial_draft_is_readable_before_workflow_registration_but_submitted_requests_fail_closed() {
        assert_eq!(
            registered_binding("request", PurchasePaymentRequestStatus::Draft, 0, None).unwrap(),
            None
        );
        for status in [
            PurchasePaymentRequestStatus::Draft,
            PurchasePaymentRequestStatus::InApproval,
            PurchasePaymentRequestStatus::Rejected,
            PurchasePaymentRequestStatus::Withdrawn,
            PurchasePaymentRequestStatus::Approved,
            PurchasePaymentRequestStatus::PartiallyPaid,
            PurchasePaymentRequestStatus::Paid,
            PurchasePaymentRequestStatus::Closed,
        ] {
            assert!(registered_binding("request", status, 1, None).is_err());
            if status != PurchasePaymentRequestStatus::Draft {
                assert!(registered_binding("request", status, 0, None).is_err());
            }
        }
    }

    #[test]
    fn existing_submission_requires_its_exact_registered_identity_and_frozen_definition() {
        let mut document =
            new_registered_document("request", DocumentType::PurchasePaymentRequest, "FK-1").unwrap();
        assert!(
            registered_binding(
                "request",
                PurchasePaymentRequestStatus::InApproval,
                1,
                Some(document.clone())
            )
            .is_err()
        );
        let binding = ApprovalDefinitionBinding::new(
            ApprovalProcessDefinitionId::new("definition"),
            2,
            Instant::from_unix_secs(10),
        )
        .unwrap();
        document.approval_binding = Some(binding.clone());
        assert_eq!(
            registered_binding(
                "request",
                PurchasePaymentRequestStatus::InApproval,
                1,
                Some(document.clone())
            )
            .unwrap(),
            Some(binding)
        );
        assert!(
            registered_binding("other", PurchasePaymentRequestStatus::Draft, 0, Some(document.clone()))
                .is_err()
        );
        document.document_type = DocumentType::PurchaseOrder;
        assert!(
            registered_binding("request", PurchasePaymentRequestStatus::Draft, 0, Some(document)).is_err()
        );
    }
}
