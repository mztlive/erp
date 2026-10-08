//! 加载后补合同核对依据；所有读取沿用命令执行器。
use persistence_core::Executor;

use super::SalesOrderService;
use crate::entity::sales_order::contract_terms::ContractTerms;
use crate::entity::sales_order::{
    CommercialStatus, SalesOrder, SalesOrderId, SalesOrderWorkingCopy, SubmissionStatus, WorkingPurpose,
};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 已保存的销售条款和可编辑来源；绑定事务写回副本以防并发修改。
pub struct ContractBindingBasis {
    pub terms: ContractTerms,
    pub label: String,
    pub working_copy: Option<SalesOrderWorkingCopy>,
}

impl SalesOrderService {
    /// 按当前商业状态加载补合同依据，禁止用本次请求的新条款参与核对。
    ///
    /// # 参数
    /// * `order` - 当前事务已授权读取的销售单
    /// * `executor` - 原命令执行器
    /// # 返回
    /// 生效单取当前版本，草稿取有效副本，审批中取最新冻结提交。
    /// # 错误
    /// 来源缺失、归属冲突、作废或审批状态冲突时拒绝。
    pub async fn contract_binding_basis(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<ContractBindingBasis> {
        match order.commercial_status {
            CommercialStatus::Voided => Err(Error::BusinessLogicError("已作废销售单不能补录合同".into())),
            CommercialStatus::Effective => self.contract_revision_basis(order, executor).await,
            CommercialStatus::Draft => {
                let copy = self
                    .db
                    .sales_order_working_copies()
                    .find_active_by_order_and_purpose(
                        &SalesOrderId::new(&order.base.id),
                        WorkingPurpose::FirstSubmission,
                        executor,
                    )
                    .await?;
                if let Some(copy) = copy {
                    if copy.customer_id != order.customer_id
                        || copy.settlement_party_id != order.settlement_party_id
                    {
                        return Err(Error::ConflictError("销售草稿归属不一致，请刷新后重试".into()));
                    }
                    return Ok(ContractBindingBasis {
                        terms: ContractTerms {
                            payment: copy.payment_term_snapshot.clone(),
                            invoice: copy.invoice_requirement_snapshot.clone(),
                        },
                        label: "当前已保存草稿".into(),
                        working_copy: Some(copy),
                    });
                }
                Err(Error::ConflictError("请先保存销售草稿，再补录合同".into()))
            },
            CommercialStatus::PendingReview => self.contract_submission_basis(order, executor).await,
        }
    }

    /// 只采用归属本单的当前生效版本条款；版本缺失或串单时拒绝补录。
    async fn contract_revision_basis(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<ContractBindingBasis> {
        let id = order
            .current_revision_id()
            .ok_or_else(|| Error::ConflictError("销售单缺少当前生效版本，不能补录合同".into()))?;
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(id, executor)
            .await?
            .filter(|row| row.sales_order_id.as_ref() == order.base.id)
            .ok_or_else(|| Error::ConflictError("销售单当前生效版本缺失或归属不一致".into()))?;
        Ok(ContractBindingBasis {
            terms: ContractTerms {
                payment: revision.payment_term_snapshot,
                invoice: revision.invoice_requirement_snapshot,
            },
            label: "当前生效版本".into(),
            working_copy: None,
        })
    }

    /// 审批中只接受仍在审的最新提交，并核对客户与结算主体仍与销售单一致。
    async fn contract_submission_basis(
        &self,
        order: &SalesOrder,
        executor: &mut dyn Executor,
    ) -> Result<ContractBindingBasis> {
        let submission = self
            .db
            .sales_order_submissions()
            .find_latest_by_order(&SalesOrderId::new(&order.base.id), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("销售单缺少已保存条款，请先保存草稿再补录合同".into()))?;
        if submission.customer_id != order.customer_id
            || submission.settlement_party_id != order.settlement_party_id
            || (order.commercial_status == CommercialStatus::PendingReview
                && submission.stable.status != SubmissionStatus::InReview)
        {
            return Err(Error::ConflictError("销售提交归属或审批状态不一致，请刷新后重试".into()));
        }
        Ok(ContractBindingBasis {
            terms: ContractTerms {
                payment: submission.payment_term_snapshot,
                invoice: submission.invoice_requirement_snapshot,
            },
            label: format!("第 {} 次提交", submission.submission_no),
            working_copy: None,
        })
    }
}
