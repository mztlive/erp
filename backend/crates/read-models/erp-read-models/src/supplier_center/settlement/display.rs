//! 对已有授权或命令结果补名称，不重查列表、不改变分页或授权。

use erp_supply::dto::supplier_settlement::{
    SettlementDifferenceDecisionResult, SettlementDifferenceEvidenceResult, SettlementPageView,
    SupplierSettlementDifferenceView, SupplierSettlementItemView, SupplierSettlementStatementListView,
};

use super::display_dto::{
    SettlementEvidenceDisplayView, SettlementItemDisplayView, SettlementStatementDisplayDetail,
    SettlementStatementDisplayList,
};
use super::display_mapping::readable_name;
use super::dto::SupplierSettlementStatementDetailView;
use super::{SettlementStatementResult, SupplierSettlementReadService};
use crate::Result;

impl SupplierSettlementReadService {
    /// 为已执行的正式结果补齐结算单名称。
    ///
    /// # 参数
    /// * `result` - 已通过原授权和命令规则的结果。
    /// # 返回
    /// 返回同一正式结果及可读结算单名称。
    /// # 错误
    /// 名称缺失或附加读取失败时保持正式结果，名称为空。
    pub async fn statement_result<T: SettlementStatementResult>(&self, result: T) -> Result<T::Display> {
        let facts = self.command_display_facts(&[result.statement()], Vec::new()).await;
        let statement = facts.statement(result.statement().clone());
        Ok(result.with_statement(statement))
    }

    /// 为当前授权列表页批量补齐名称，保留全部范围版本和统计。
    ///
    /// # 参数
    /// * `page` - 原授权流程返回的列表页。
    /// # 返回
    /// 返回同一分页和统计口径的名称视图。
    /// # 错误
    /// 名称读取失败时返回对应仓储错误。
    pub async fn statement_list_view(
        &self,
        page: SupplierSettlementStatementListView,
    ) -> Result<SettlementStatementDisplayList> {
        let statements = page.items.iter().collect::<Vec<_>>();
        let facts = self.statement_display_facts(&statements, Vec::new()).await?;
        Ok(SupplierSettlementStatementListView {
            items: page.items.into_iter().map(|statement| facts.statement(statement)).collect(),
            total: page.total,
            page: page.page,
            page_size: page.page_size,
            stats: page.stats,
            processing_state: page.processing_state,
            scope_version: page.scope_version,
            policy_version: page.policy_version,
            organization_version: page.organization_version,
            as_of: page.as_of,
            empty_reason: page.empty_reason,
            scope_summary: page.scope_summary,
            ownership_basis: page.ownership_basis,
        })
    }

    /// 为当前结算明细页批量补齐订单业务单号和商品名称。
    ///
    /// # 参数
    /// * `page` - 原结算明细查询结果。
    /// # 返回
    /// 返回同一分页口径的可读视图。
    /// # 错误
    /// 关联事实读取失败时返回仓储错误。
    pub async fn item_page_view(
        &self,
        page: SettlementPageView<SupplierSettlementItemView>,
    ) -> Result<SettlementPageView<SettlementItemDisplayView>> {
        Ok(SettlementPageView {
            items: self.item_display_views(page.items).await?,
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    /// 为当前差异页批量补齐补证记录人姓名。
    ///
    /// # 参数
    /// * `page` - 原结算差异查询结果。
    /// # 返回
    /// 返回同一差异集合和分页口径。
    /// # 错误
    /// 姓名读取失败时返回仓储错误。
    pub async fn difference_page_view(
        &self,
        page: SettlementPageView<SupplierSettlementDifferenceView>,
    ) -> Result<SettlementPageView<SupplierSettlementDifferenceView<SettlementEvidenceDisplayView>>> {
        let users = evidence_users(&page.items);
        let facts = self.statement_display_facts(&[], users).await?;
        Ok(SettlementPageView {
            items: page.items.into_iter().map(|difference| facts.difference(difference)).collect(),
            total: page.total,
            page: page.page,
            page_size: page.page_size,
        })
    }

    /// 为已登记的正式补证结果补齐记录人姓名与材料标签。
    ///
    /// # 参数
    /// * `result` - 正式补证命令结果。
    /// # 返回
    /// 返回原结果及补证名称。
    /// # 错误
    /// 名称缺失或附加读取失败时保持正式结果，名称为空。
    pub async fn evidence_result_view(
        &self,
        result: SettlementDifferenceEvidenceResult,
    ) -> Result<SettlementDifferenceEvidenceResult<SettlementEvidenceDisplayView>> {
        let facts = self.command_display_facts(&[], vec![result.evidence.provided_by.clone()]).await;
        Ok(SettlementDifferenceEvidenceResult {
            result_status: result.result_status,
            message: result.message,
            request_id: result.request_id,
            statement_id: result.statement_id,
            difference_id: result.difference_id,
            evidence: facts.evidence(result.evidence),
        })
    }

    /// 为已登记的正式差异决定补齐补证名称。
    ///
    /// # 参数
    /// * `result` - 正式差异决定结果。
    /// # 返回
    /// 返回同一决定和差异事实的名称视图。
    /// # 错误
    /// 名称缺失或附加读取失败时保持正式结果，名称为空。
    pub async fn difference_result_view(
        &self,
        result: SettlementDifferenceDecisionResult,
    ) -> Result<
        SettlementDifferenceDecisionResult<SupplierSettlementDifferenceView<SettlementEvidenceDisplayView>>,
    > {
        let users = evidence_users(std::slice::from_ref(&result.difference));
        let facts = self.command_display_facts(&[], users).await;
        Ok(SettlementDifferenceDecisionResult {
            result_status: result.result_status,
            message: result.message,
            operation_id: result.operation_id,
            statement_id: result.statement_id,
            statement_lock_version: result.statement_lock_version,
            difference: facts.difference(result.difference),
        })
    }

    /// 为已授权详情批量补齐名称并保留动作投影。
    ///
    /// # 参数
    /// * `detail` - 原结算、差异及当前正式任务详情。
    /// # 返回
    /// 返回同一详情的名称视图。
    /// # 错误
    /// 关联事实读取失败时返回仓储错误。
    pub(super) async fn detail_display_view(
        &self,
        mut detail: SupplierSettlementStatementDetailView,
    ) -> Result<SettlementStatementDisplayDetail> {
        let mut users = evidence_users(&detail.differences);
        if let Some(user) = detail.review_work_item.as_ref().and_then(|item| item.owner_user_id.clone()) {
            users.push(user);
        }
        let facts = self.statement_display_facts(&[&detail.statement], users).await?;
        if let Some(work_item) = &mut detail.review_work_item {
            work_item.owner_user_name =
                work_item.owner_user_id.as_deref().and_then(|id| readable_name(&facts.user_names, id));
        }
        Ok(SupplierSettlementStatementDetailView {
            statement: facts.statement(detail.statement),
            items: self.item_display_views(detail.items).await?,
            differences: detail
                .differences
                .into_iter()
                .map(|difference| facts.difference(difference))
                .collect(),
            stats: detail.stats,
            processing_state: detail.processing_state,
            review_work_item: detail.review_work_item,
            review_processing_state: detail.review_processing_state,
            review_action_blockers: detail.review_action_blockers,
            allowed_actions: detail.allowed_actions,
            action_blockers: detail.action_blockers,
        })
    }
}

fn evidence_users(differences: &[SupplierSettlementDifferenceView]) -> Vec<String> {
    differences
        .iter()
        .flat_map(|difference| difference.evidence.iter().map(|evidence| evidence.provided_by.clone()))
        .collect()
}
