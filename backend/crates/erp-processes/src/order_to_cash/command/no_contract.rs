//! 无合同销售草稿解析及建单凭证原子关联。

use application_core::AuditActor;
use erp_core::ids::{BusinessDocumentId, ContractId, CustomerAccountId, DocumentAttachmentId, PartyId};
use erp_identity::SharedRbacService;
use erp_party::PartyExt;
use erp_read_models::workbench::authorize_material_transfer;
use erp_sales::dto::sales_order::{SalesOrderDraftRequest, SalesOrderEditableDraftRequest};
use erp_sales::entity::sales_order::SalesOrder;
use erp_support::repository::prelude::*;
use erp_support::{
    AttachmentUsage, DocumentAttachment, DocumentAttachmentData, FileAssetExt, SecurityScanStatus,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::authorization::SalesCommandAccess;
use crate::adapters::workflow::workflow_auth;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
    /// 按原命令顺序解析编辑内容并接纳首次合同关联。
    ///
    /// # 参数
    /// * `access` / `order` - 当前命令授权及已授权稳定对象
    /// * `source` / `editable` - 所选合同、客户及可编辑草稿
    /// * `actor` / `require_editable` - 当前用户及提交前的状态检查要求
    /// # 返回
    /// 返回已准备的稳定对象与完整草稿，不执行写入。
    /// # 错误
    /// 关系、编辑状态或合同上下文不一致时保留原首错順序。
    pub(super) async fn prepare_sales_edit(
        &self,
        access: &SalesCommandAccess,
        mut order: SalesOrder,
        source: (Option<ContractId>, Option<CustomerAccountId>),
        editable: SalesOrderEditableDraftRequest,
        actor: &AuditActor,
        require_editable: bool,
    ) -> Result<(SalesOrder, SalesOrderDraftRequest)> {
        let (contract_id, customer_id) = source;
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(access, &contract_id, &customer_id, editable, &mut NoTransaction)
            .await?;
        if require_editable {
            order
                .ensure_first_submission_working_copy_editable()
                .map_err(|error| Error::ConflictError(error.to_string()))?;
        }
        order
            .apply_command_contract_context(&contract_id, &customer_id, &settlement_party_id, actor.id())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        Ok((order, draft))
    }

    /// 解析所选合同，或按客户主体现行资料冻结无合同销售草稿。
    ///
    /// # 参数
    /// * `access` / `executor` - 调用方授权与执行器
    /// * `contract_id` / `customer_id` - 所选业务关系
    /// * `editable` - 可编辑行和约定条款
    /// # 返回
    /// 返回客户、结算主体及服务端完整草稿。
    /// # 错误
    /// 客户缺失、停用、版本缺失或非法条款时拒绝。
    pub(super) async fn resolve_sales_command_draft(
        &self,
        access: &SalesCommandAccess,
        contract_id: &Option<ContractId>,
        customer_id: &Option<CustomerAccountId>,
        editable: SalesOrderEditableDraftRequest,
        executor: &mut dyn Executor,
    ) -> Result<(CustomerAccountId, PartyId, SalesOrderDraftRequest)> {
        if let Some(contract_id) = contract_id {
            return self.resolve_contract_sales_draft(access, contract_id, editable, executor).await;
        }
        editable.validate()?;
        if editable.requested_contract_revision_id.is_some() {
            return Err(Error::ValidationError("无合同销售单不得指定合同版本".into()));
        }
        let id =
            customer_id.as_ref().ok_or_else(|| Error::ValidationError("无合同销售单必须选择客户".into()))?;
        let customer = access.load_customer(id.as_ref(), executor).await?;
        if !customer.is_active() {
            return Err(Error::BusinessLogicError("客户已停用，禁止创建新销售单".into()));
        }
        let party = self
            .db
            .parties()
            .find_by_id(customer.party_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("客户主体不存在".into()))?;
        let revision_id = party
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::ConflictError("客户主体缺少有效资料版本".into()))?;
        let revision = self
            .db
            .party_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .filter(|row| row.party_id == customer.party_id)
            .ok_or_else(|| Error::ConflictError("客户主体资料版本不存在或归属不一致".into()))?;
        let mut draft = editable.into_no_contract_draft(revision.legal_name)?;
        self.sales().resolve_draft_reference_prices(&mut draft.lines, &self.catalog(), executor).await?;
        Ok((CustomerAccountId::new(customer.base.id), customer.party_id, draft))
    }

    /// 在建单原事务中验证并关联首次凭证，供详情与审批材料授权读取。
    ///
    /// # 参数
    /// * `db` / `rbac` / `executor` - 原建单授权及事务
    /// * `order` / `actor` - 待创建稳定单据及当前用户
    /// # 返回
    /// 原子创建全部凭证与单据的关联。
    /// # 错误
    /// 资产缺失、非 PDF/图片、归属未授权或关联保存失败时拒绝。
    pub(super) async fn persist_creation_evidence(
        db: &Database,
        rbac: &SharedRbacService,
        order: &SalesOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let files = db.file_assets().find_by_ids(&order.evidence_file_asset_ids, executor).await?;
        if files.len() != order.evidence_file_asset_ids.len() {
            return Err(Error::ValidationError("销售建单凭证不存在或已删除".into()));
        }
        authorize_material_transfer(&workflow_auth(db.clone(), rbac.clone()), actor, &files, executor)
            .await?;
        for file in files {
            SalesOrder::validate_creation_evidence_file(
                &file.content_type,
                &file.file_name,
                file.destroyed_at.is_some()
                    || matches!(
                        file.security_scan_status,
                        SecurityScanStatus::Rejected | SecurityScanStatus::Quarantined
                    ),
                file.byte_size,
            )?;
            let attachment = DocumentAttachment::new(
                DocumentAttachmentId::new(next_id()),
                DocumentAttachmentData {
                    document_id: BusinessDocumentId::new(order.base.id.clone()),
                    file_asset_id: file.base.id.into(),
                    usage: AttachmentUsage::Attachment,
                    created_by: actor.id().to_string(),
                },
            )?;
            db.document_attachments().create(&attachment, executor).await?;
        }
        Ok(())
    }
}
