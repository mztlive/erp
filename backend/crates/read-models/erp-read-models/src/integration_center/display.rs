//! 已授权集成对象的可读名称投影；未知对象类型不猜测身份。

use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_integration::dto::{ControlledEvidenceRef, DifferenceView, ErrorTaskView};
use erp_integration::entity::integration_ops::CanonicalEvidenceReference;
use persistence_core::NoTransaction;

use super::IntegrationCenterReadService;
use super::error_labels::integration_error_labels;
pub(crate) use super::repository::object_labels;
use crate::Result;

impl IntegrationCenterReadService {
    /// 为已经通过列表或详情授权的错误任务补齐可读展示事实。
    ///
    /// # 参数
    /// * `items` - 当前查询已授权的任务集合。
    ///
    /// # 返回
    /// 原位填入责任人姓名和入站消息名称，未知业务对象类型保持空名称。
    ///
    /// # 错误
    /// 关联事实查询失败时返回仓储错误。
    pub async fn error_task_names(&self, items: &mut [ErrorTaskView]) -> Result<()> {
        let ids = items.iter().filter_map(|item| item.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, &mut NoTransaction).await?;
        let mut labels = integration_error_labels(&self.db, items, &mut NoTransaction).await?;
        for item in items {
            item.owner_user_name = item.owner_user_id.as_ref().and_then(|id| names.get(id).cloned());
            if let Some(labels) = labels.remove(&item.id) {
                item.message_label = labels.message_label;
                item.business_object_label = labels.business_object_label;
            }
        }
        Ok(())
    }

    /// 为已经通过列表或详情授权的差异补齐责任人及类型明确的对象名称。
    ///
    /// # 参数
    /// * `items` - 当前查询已授权的差异集合。
    ///
    /// # 返回
    /// 原位补齐可读名称，缺失关联和未知类型保留空值。
    ///
    /// # 错误
    /// 关联事实查询失败时返回仓储错误。
    pub async fn difference_names(&self, items: &mut [DifferenceView]) -> Result<()> {
        let ids = items.iter().map(|item| item.owner_user_id.clone()).collect::<Vec<_>>();
        let names = self.db.accounts().names_by_ids(&ids, &mut NoTransaction).await?;
        let labels = object_labels(&self.db, items, &mut NoTransaction).await?;
        for item in items {
            item.owner_user_name = names.get(&item.owner_user_id).cloned();
            item.business_object_label =
                labels.get(&(item.business_object_type.clone(), item.business_object_id.clone())).cloned();
        }
        Ok(())
    }
}

pub(super) fn evidence_label(reference: Option<&str>, evidence: &[ControlledEvidenceRef]) -> Option<String> {
    let labels = reference?
        .split(';')
        .map(|part| {
            let reference = CanonicalEvidenceReference::parse_stored(part).ok()?;
            evidence.iter().find_map(|evidence| {
                let candidate = CanonicalEvidenceReference::parse_stored(&evidence.record_id).ok()?;
                (candidate.kind() == reference.kind() && candidate.id() == reference.id())
                    .then(|| evidence.label.trim().to_string())
                    .filter(|label| !label.is_empty())
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (!labels.is_empty()).then(|| labels.join("、"))
}

#[cfg(test)]
mod tests {
    use erp_integration::dto::ControlledEvidenceKind;

    use super::*;

    #[test]
    fn evidence_names_require_exact_type_and_identity() {
        let evidence = vec![ControlledEvidenceRef {
            kind: ControlledEvidenceKind::ExternalCaseResult,
            record_id: "supplier_order_action:result-1".into(),
            label: "供应商结果核验".into(),
        }];
        assert_eq!(
            evidence_label(Some("supplier_order_action:result-1:v2:verified"), &evidence).as_deref(),
            Some("供应商结果核验")
        );
        assert_eq!(
            evidence_label(Some("supplier_order_action://result-1"), &evidence).as_deref(),
            Some("供应商结果核验")
        );
        assert!(evidence_label(Some("another_type:result-1"), &evidence).is_none());
        assert!(evidence_label(Some("supplier_order_action:missing"), &evidence).is_none());
        assert!(evidence_label(Some("supplier_order_action:result-1;unknown:missing"), &evidence).is_none());
    }
}
