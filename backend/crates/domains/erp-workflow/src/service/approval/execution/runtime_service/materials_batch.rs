//! 批量读取工作台所需的冻结审批资料摘要。

use application_core::AuditActor;
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};

use super::{ApprovalMaterialsView, ApprovalRuntimeService};
use crate::entity::approval_integration::display_snapshot::ApprovalBriefSource;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;

const MAX_REFERENCES: usize = 50;
const CONCURRENT_READS: usize = 8;

/// 本页任务绑定的原审批引用，按请求数组顺序返回结果。
#[derive(Deserialize)]
pub struct ApprovalMaterialsBatchQuery {
    references: Vec<ApprovalMaterialsReference>,
}

impl ApprovalMaterialsBatchQuery {
    /// 限制单次请求的读取数量，允许空引用列表。
    fn validate(&self) -> Result<()> {
        if self.references.len() > MAX_REFERENCES {
            return Err(Error::ValidationError(format!("单次最多读取 {MAX_REFERENCES} 份审批资料")));
        }
        Ok(())
    }
}

/// 精确到同一次提交的资料引用。
#[derive(Clone, Deserialize)]
struct ApprovalMaterialsReference {
    instance_id: String,
    document_type: String,
    document_id: String,
    subject_version: String,
}

/// 一条审批资料摘要的读取结果；拒绝与不存在统一为不可查看。
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ApprovalMaterialsBatchResult {
    Available { materials: Box<ApprovalMaterialSummary> },
    Unavailable,
    ReferenceMismatch,
    Failed,
}

/// 工作台列表需要的冻结业务资料，不含附件和完整业务单据。
#[derive(Serialize)]
pub struct ApprovalMaterialSummary {
    document_no: String,
    document_type: String,
    document_id: String,
    subject_version: u32,
    counterparty_label: Option<String>,
    impact_summary: Option<String>,
    source: ApprovalBriefSource,
    source_sales: Vec<ApprovalMaterialSourceSummary>,
}

/// 列表展示的冻结来源销售单身份和单号。
#[derive(Serialize)]
struct ApprovalMaterialSourceSummary {
    document_id: String,
    document_no: String,
}

impl ApprovalMaterialsBatchResult {
    /// 校验授权返回的资料是否与任务绑定的精确提交一致，再生成摘要。
    fn from_read(reference: &ApprovalMaterialsReference, result: Result<ApprovalMaterialsView>) -> Self {
        let materials = match result {
            Ok(materials) => materials,
            Err(Error::Forbidden(_) | Error::NotFound(_) | Error::ValidationError(_)) => {
                return Self::Unavailable;
            },
            Err(error) => {
                tracing::warn!(error_class = ?error.class(), "读取工作台审批资料失败");
                return Self::Failed;
            },
        };
        if materials.document_type != reference.document_type
            || materials.document_id != reference.document_id
            || materials.subject_version.to_string() != reference.subject_version
        {
            return Self::ReferenceMismatch;
        }

        let display = materials.display;
        let source_sales = display
            .source_sales
            .into_iter()
            .map(|sales| ApprovalMaterialSourceSummary {
                document_id: sales.document_id,
                document_no: sales.document_no,
            })
            .collect();
        Self::Available {
            materials: Box::new(ApprovalMaterialSummary {
                document_no: materials.document_no,
                document_type: materials.document_type,
                document_id: materials.document_id,
                subject_version: materials.subject_version,
                counterparty_label: display.counterparty_label,
                impact_summary: display.impact_summary,
                source: display.source,
                source_sales,
            }),
        }
    }
}

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 并发读取一页审批资料，每个实例复用单条资料的授权与快照读取。
    /// # 参数
    /// 当前认证账号及带原审批实例、单据类型、单据 ID 和提交版本的引用。
    /// # 返回
    /// 按输入顺序返回摘要、不可查看或读取失败；各条读取失败互不影响。
    /// # 错误
    /// 单批引用超过上限时拒绝整个请求。
    pub async fn batch_materials(
        &self,
        actor: &AuditActor,
        query: ApprovalMaterialsBatchQuery,
    ) -> Result<Vec<ApprovalMaterialsBatchResult>> {
        query.validate()?;
        let items = stream::iter(query.references)
            .map(|reference| async move {
                let materials = self.materials(actor, &reference.instance_id).await;
                ApprovalMaterialsBatchResult::from_read(&reference, materials)
            })
            .buffered(CONCURRENT_READS)
            .collect()
            .await;
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::approval_integration::display_snapshot::ApprovalDisplaySnapshot;

    fn reference() -> ApprovalMaterialsReference {
        ApprovalMaterialsReference {
            instance_id: "rejected-instance".into(),
            document_type: "sales_order".into(),
            document_id: "original-order".into(),
            subject_version: "3".into(),
        }
    }

    fn materials() -> ApprovalMaterialsView {
        ApprovalMaterialsView {
            document_no: "SO-001".into(),
            document_type: "sales_order".into(),
            document_id: "original-order".into(),
            subject_version: 3,
            display: ApprovalDisplaySnapshot::new("original-order".into()),
            attachments: vec![],
        }
    }

    #[test]
    fn only_matching_authorized_submissions_produce_a_summary() {
        let reference = reference();
        let available = ApprovalMaterialsBatchResult::from_read(&reference, Ok(materials()));
        let available = serde_json::to_value(available).unwrap();
        assert_eq!(available["status"], "available");
        assert_eq!(available["materials"]["document_no"], "SO-001");
        assert_eq!(available["materials"]["subject_version"], 3);
        assert!(available["materials"].get("attachments").is_none());

        let mut newer = materials();
        newer.subject_version = 4;
        let mut different = materials();
        different.document_id = "another-order".into();
        let mut wrong_type = materials();
        wrong_type.document_type = "purchase_order".into();
        for materials in [newer, different, wrong_type] {
            let mismatch = ApprovalMaterialsBatchResult::from_read(&reference, Ok(materials));
            assert_eq!(
                serde_json::to_value(mismatch).unwrap(),
                serde_json::json!({"status":"reference_mismatch"})
            );
        }
        for result in [Err(Error::NotFound("不存在".into())), Err(Error::Forbidden("无权访问".into()))]
        {
            let unavailable = ApprovalMaterialsBatchResult::from_read(&reference, result);
            assert_eq!(
                serde_json::to_value(unavailable).unwrap(),
                serde_json::json!({"status":"unavailable"})
            );
        }

        let failed =
            ApprovalMaterialsBatchResult::from_read(&reference, Err(Error::Internal("内部错误".into())));
        assert_eq!(serde_json::to_value(failed).unwrap(), serde_json::json!({"status":"failed"}));
    }

    #[test]
    fn material_batch_accepts_a_full_page_and_rejects_larger_requests() {
        for count in [0, MAX_REFERENCES] {
            assert!(ApprovalMaterialsBatchQuery { references: vec![reference(); count] }.validate().is_ok());
        }
        assert!(matches!(
            ApprovalMaterialsBatchQuery { references: vec![reference(); MAX_REFERENCES + 1] }.validate(),
            Err(Error::ValidationError(_))
        ));
    }
}
