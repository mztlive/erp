//! 审批实例授权后读取冻结提交，另列开票当前销售依据和采购最新供应商评估。

use std::slice;

use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SalesOrderSubmissionId};
use erp_sales::dto::sales_order::SubmissionView;
use erp_sales::entity::sales_order::BusinessType;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::mapper::submission_view;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_copy::ApprovalCopyReceipt;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;
use erp_workflow::service::approval::execution::{ApprovalRuntimeService, RuntimeHistoryPage};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::approval_source_sales::{ApprovalSourceSalesOrder, source_sales_orders};
use super::prices::{enrich_inquiry_prices, enrich_line_prices};
use crate::purchase_center::approval_materials::{
    ApprovalExtraCostSummary, ApprovalPurchaseLine, extra_cost_summary, purchase_lines,
};
use crate::purchase_center::approval_supplier::{ApprovalSupplierRating, supplier_rating};
use crate::workbench::{InvoiceSalesBasis, optional_sales_basis};
use crate::{Error, Result};

/// 审批资料及可选的完整销售提交；保留原资料接口字段。
#[derive(Debug, Serialize)]
pub struct ApprovalDocumentMaterials {
    #[serde(flatten)]
    pub materials: ApprovalMaterialsView,
    pub sales_order: Option<SubmissionView>,
    pub purchase_lines: Option<Vec<ApprovalPurchaseLine>>,
    pub extra_cost_summary: Option<ApprovalExtraCostSummary>,
    pub source_sales_orders: Vec<ApprovalSourceSalesOrder>,
    pub current_sales_basis: Option<InvoiceSalesBasis>,
    pub current_sales_basis_error: Option<String>,
    /// 独立的最新登记评估，不属于提交冻结资料；抄送读取不提供。
    pub latest_supplier_rating: Option<ApprovalSupplierRating>,
}

/// 准备销售审批材料时，在实例读取授权后装载精确销售提交，不授予普通销售单读取资格。
///
/// # 参数
/// 数据库、审批运行服务、当前账号与精确审批实例 ID。
///
/// # 返回
/// 原冻结资料及完整提交明细；另列开票当前销售资料和采购最新供应商评估，不覆盖提交内容。
///
/// # 错误
/// 实例未授权、原资料缺失、销售提交缺失或身份及明细不匹配时拒绝。
pub async fn materials<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    instance_id: &str,
) -> Result<ApprovalDocumentMaterials> {
    let (db, runtime, actor, instance_id) =
        (db.clone(), runtime.clone(), actor.clone(), instance_id.to_owned());
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { materials_read(&db, &runtime, &actor, &instance_id, executor).await })
        })
        .await
}

// 处理销售审批材料时，授权、冻结版本和当前核对资料均在同一调用方快照内完成。
async fn materials_read<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalDocumentMaterials> {
    let materials = runtime.materials_with(actor, instance_id, executor).await?;

    let sales_order = if matches!(materials.document_type.as_str(), "sales_order" | "voucher_sales_order") {
        Some(sales_submission(db, &materials, executor).await?)
    } else {
        None
    };
    let (purchase_lines, extra_cost_summary) = if materials.document_type == "purchase_order" {
        (purchase_lines(db, &materials, executor).await?, extra_cost_summary(db, &materials, executor).await?)
    } else {
        (None, None)
    };
    let source_sales_orders = source_sales_orders(db, &materials, executor).await?;
    let (current_sales_basis, current_sales_basis_error) =
        optional_sales_basis(db, &materials, executor).await?;
    let latest_supplier_rating = supplier_rating(db, &materials, executor).await?;

    Ok(ApprovalDocumentMaterials {
        materials,
        sales_order,
        purchase_lines,
        extra_cost_summary,
        source_sales_orders,
        current_sales_basis,
        current_sales_basis_error,
        latest_supplier_rating,
    })
}

/// 抄送资料只补齐冻结版本，不加载当前票款或最新销售依据。
#[derive(Debug, Serialize)]
pub struct CopyDocumentDetail {
    pub receipt: ApprovalCopyReceipt,
    pub instance_status: String,
    pub materials: ApprovalDocumentMaterials,
    pub history: RuntimeHistoryPage,
}
/// 组装销售审批材料时，以精确个人收件授权读取完整冻结单据。
///
/// # 参数
/// 数据库、运行服务、当前身份、收件 ID 和历史游标。
///
/// # 返回
/// 本次提交及已冻结关联销售版本；不含当前业务余额。
///
/// # 错误
/// 来源版本不匹配、账号撤权或数据读取失败。
pub async fn copy_detail<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    id: &str,
    after: Option<u32>,
) -> Result<CopyDocumentDetail> {
    let (db, runtime, actor, id) = (db.clone(), runtime.clone(), actor.clone(), id.to_owned());
    db.client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { copy_read(&db, &runtime, &actor, &id, after, executor).await })
        })
        .await
}

// 处理销售审批材料时，抄送的完整业务补齐结束后，验证本次读取的原收件凭证。
async fn copy_read<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    id: &str,
    after: Option<u32>,
    executor: &mut dyn Executor,
) -> Result<CopyDocumentDetail> {
    let (detail, access) = runtime.copy_read(actor, id, after, executor).await?;
    let materials = detail.materials;

    let sales_order = if matches!(materials.document_type.as_str(), "sales_order" | "voucher_sales_order") {
        Some(sales_submission(db, &materials, executor).await?)
    } else {
        None
    };
    let purchase_lines = if materials.document_type == "purchase_order" {
        purchase_lines(db, &materials, executor).await?
    } else {
        None
    };
    let source_sales_orders = source_sales_orders(db, &materials, executor).await?;

    runtime.validate_copy_read(&access, executor).await?;
    Ok(CopyDocumentDetail {
        receipt: detail.receipt,
        instance_status: detail.instance_status,
        history: detail.history,
        materials: ApprovalDocumentMaterials {
            materials,
            sales_order,
            purchase_lines,
            source_sales_orders,
            extra_cost_summary: None,
            current_sales_basis: None,
            current_sales_basis_error: None,
            latest_supplier_rating: None,
        },
    })
}

async fn sales_submission(
    db: &Database,
    materials: &ApprovalMaterialsView,
    executor: &mut dyn Executor,
) -> Result<SubmissionView> {
    let submission = db
        .sales_order_submissions()
        .find_by_order_and_no(&SalesOrderId::new(&materials.document_id), materials.subject_version, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("本次审批对应的销售提交不存在，请联系管理员核对".into()))?;
    let mut lines = db
        .sales_order_submission_lines()
        .list_lines_by_submissions(&[SalesOrderSubmissionId::new(&submission.base.id)], executor)
        .await?;

    validate_submission(
        materials,
        submission.sales_order_id.as_ref(),
        submission.submission_no,
        submission.business_type,
        lines.len(),
    )?;

    lines.sort_by_key(|line| line.line_no);
    let mut view = submission_view(submission, lines);
    enrich_line_prices(db, &mut view.lines, executor).await?;
    enrich_inquiry_prices(db, &mut view.lines, slice::from_ref(&materials.document_id), executor).await?;
    Ok(view)
}

// 处理销售审批材料的版本事实时，完整预览必须与已授权资料的单据、提交版本、业务类型及冻结总行数一致。
fn validate_submission(
    materials: &ApprovalMaterialsView,
    order_id: &str,
    version: u32,
    business_type: BusinessType,
    line_count: usize,
) -> Result<()> {
    let document_type = match business_type {
        BusinessType::GoodsService => "sales_order",
        BusinessType::Voucher => "voucher_sales_order",
    };
    let source = &materials.display.source;
    let expected_count = source.lines.len().checked_add(
        usize::try_from(source.more_count)
            .map_err(|_| Error::ConflictError("销售提交行数超出范围".into()))?,
    );
    if materials.document_id != order_id
        || materials.display.root_document_id != order_id
        || materials.subject_version != version
        || materials.document_type != document_type
        || expected_count != Some(line_count)
    {
        return Err(Error::ConflictError("销售提交与本次审批资料不一致，请联系管理员核对".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_identity::testing::unavailable_authorization;
    use erp_identity::{RbacService, Result as IdentityResult};
    use erp_workflow::entity::approval_integration::display_snapshot::{
        ApprovalBriefLine, ApprovalDisplaySnapshot,
    };
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[derive(Clone)]
    struct UnusedAuthorization;

    impl WorkflowAuthorizationPort for UnusedAuthorization {}

    struct UnusedAudit;

    #[async_trait]
    impl IdentityAuditPort for UnusedAudit {
        fn resource_log(
            &self,
            _: AuditActor,
            _: &str,
            _: &str,
            _: String,
        ) -> IdentityResult<PreparedResourceAudit> {
            panic!("读取拒绝不得准备审计")
        }

        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> IdentityResult<()> {
            panic!("读取拒绝不得写入审计")
        }
    }

    #[tokio::test]
    async fn complete_material_and_copy_reads_reject_before_loading_business_data() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("approval_complete_material_read");
        let identity = Arc::new(RbacService::new(db.clone(), Arc::new(UnusedAudit)));
        let runtime = ApprovalRuntimeService::new(
            db.clone(),
            identity,
            unavailable_authorization(),
            UnusedAuthorization,
        );
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);

        for result in [
            materials_read(&db, &runtime, &actor, "instance", &mut SnapshotExecutor::default())
                .await
                .map(|_| ()),
            copy_read(&db, &runtime, &actor, "receipt", None, &mut SnapshotExecutor::default())
                .await
                .map(|_| ()),
        ] {
            assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
        }

        assert!(matches!(
            materials_read(&db, &runtime, &actor, "instance", &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
        assert!(matches!(
            copy_read(&db, &runtime, &actor, "receipt", None, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(_))
        ));
    }

    fn frozen_materials() -> ApprovalMaterialsView {
        let mut display = ApprovalDisplaySnapshot::new("sales-1".into());
        display.source.lines = (0..3)
            .map(|index| ApprovalBriefLine {
                title: format!("商品{index}"), quantity: None, due_label: None
            })
            .collect();
        display.source.more_count = 16;
        ApprovalMaterialsView {
            document_no: "XS-001".into(),
            document_type: "sales_order".into(),
            document_id: "sales-1".into(),
            subject_version: 2,
            display,
            attachments: vec![],
        }
    }

    #[test]
    fn complete_submission_requires_all_nineteen_lines() {
        let materials = frozen_materials();
        assert!(validate_submission(&materials, "sales-1", 2, BusinessType::GoodsService, 19).is_ok());
        assert!(validate_submission(&materials, "sales-1", 2, BusinessType::GoodsService, 3).is_err());
        assert!(validate_submission(&materials, "sales-1", 2, BusinessType::GoodsService, 0).is_err());
    }

    #[test]
    fn another_order_version_or_business_type_cannot_replace_submission() {
        let materials = frozen_materials();
        assert!(validate_submission(&materials, "sales-2", 2, BusinessType::GoodsService, 19).is_err());
        assert!(validate_submission(&materials, "sales-1", 3, BusinessType::GoodsService, 19).is_err());
        assert!(validate_submission(&materials, "sales-1", 2, BusinessType::Voucher, 19).is_err());

        let mut voucher = materials;
        voucher.document_type = "voucher_sales_order".into();
        assert!(validate_submission(&voucher, "sales-1", 2, BusinessType::Voucher, 19).is_ok());

        voucher.display.root_document_id = "sales-2".into();
        assert!(validate_submission(&voucher, "sales-1", 2, BusinessType::Voucher, 19).is_err());
    }
}
