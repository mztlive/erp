//! 审批关联商品仅从同次审批冻结允许的销售版本读取。
use std::collections::HashMap;

use erp_core::ids::SalesOrderRevisionId;
use erp_sales::entity::sales_order::{SalesOrderRevision, SalesOrderRevisionLine};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use erp_workflow::repository::prelude::*;
use erp_workflow::{ApprovalIntegrationExt, DocumentType, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::super::WorkbenchReadService;
use super::SearchDocument;
use super::sources::SubjectKey;
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> WorkbenchReadService<A> {
    /// 读取已授权审批的冻结来源版本，追加全部商品名称和规格。
    /// # 参数
    /// `keys` 为已授权精确提交身份，`documents` 为检索资料，`executor` 为当前事务。
    /// # 返回
    /// 就地追加来源单号、客户和完整商品；没有冻结来源时不回填当前单据。
    /// # 错误
    /// 仓储故障或冻结来源身份不一致时拒绝查询。
    pub(super) async fn related_sales_search(
        &self,
        keys: &[SubjectKey],
        documents: &mut HashMap<SubjectKey, SearchDocument>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let objects = snapshot_objects(keys);
        if objects.is_empty() {
            return Ok(());
        }
        let snapshots =
            self.db.approval_subject_snapshots().list_by_business_objects(&objects, executor).await?;
        let allowed = snapshots
            .into_iter()
            .filter_map(|snapshot| {
                let key = (
                    snapshot.document_type.as_str().to_owned(),
                    snapshot.business_object_id,
                    snapshot.subject_version.to_string(),
                );
                keys.contains(&key).then_some((key, snapshot.display?))
            })
            .collect::<Vec<_>>();
        let ids = allowed
            .iter()
            .flat_map(|(_, display)| display.source_sales.iter().map(|source| source.revision_id.clone()))
            .collect::<Vec<_>>();
        if ids.is_empty() {
            return Ok(());
        }
        let revisions = self.db.sales_order_revisions().find_revisions_by_ids(&ids, executor).await?;
        let lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revisions(&ids.iter().map(SalesOrderRevisionId::new).collect::<Vec<_>>(), executor)
            .await?;
        let revisions =
            revisions.iter().map(|revision| (revision.base.id.as_str(), revision)).collect::<HashMap<_, _>>();
        for (key, display) in allowed {
            display.validate()?;
            let doc = documents.entry(key).or_default();
            for source in &display.source_sales {
                let revision = revisions
                    .get(source.revision_id.as_str())
                    .ok_or_else(|| Error::ConflictError("审批来源销售版本缺失，请核对资料".into()))?;
                append_source(doc, source, revision, &lines)?;
            }
        }
        Ok(())
    }
}

fn append_source(
    doc: &mut SearchDocument,
    source: &ApprovalRelatedSalesSnapshot,
    revision: &SalesOrderRevision,
    lines: &[SalesOrderRevisionLine],
) -> Result<()> {
    if revision.sales_order_id.as_ref() != source.document_id
        || revision.revision.revision_no != source.revision_no
    {
        return Err(Error::ConflictError("审批来源销售版本不匹配，请核对资料".into()));
    }
    doc.text("来源销售单", &source.document_no);
    doc.facet("sales_orders", &source.document_id, &source.document_no);
    if let Some(customer) = &source.source.customer {
        doc.text("来源客户", customer);
        doc.facet("counterparties", customer, customer);
    }
    for line in lines.iter().filter(|line| line.sales_order_revision_id.as_ref() == source.revision_id) {
        doc.text("来源商品", &line.item_name_snapshot);
        if let Some(spec) = &line.spec_snapshot {
            doc.text("来源商品规格", spec);
        }
    }
    Ok(())
}

fn snapshot_objects(keys: &[SubjectKey]) -> Vec<(DocumentType, String)> {
    keys.iter()
        .filter_map(|key| {
            DocumentType::ALL.iter().find(|kind| kind.as_str() == key.0).map(|kind| (*kind, key.1.clone()))
        })
        .collect()
}
