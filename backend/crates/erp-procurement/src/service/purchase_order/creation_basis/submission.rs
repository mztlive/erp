//! 按事务内供应商事实构造采购草稿快照。
use crate::entity::purchase_order::{
    BasisScope, PurchaseOrderSubmission, PurchaseOrderSubmissionData, SupplierSnapshot,
};
use crate::ports::creation_basis::CreationBasisSupplierPort;
use crate::{Error, Result};
use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId};
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;
/// 构造采购草稿提交头。
///
/// # 参数
/// * `port` - 供应商事务事实与唯一付款解析提供方
/// * `order_id` - 新采购单主键
/// * `scope` - 精确拆分范围
/// * `supplier_name` - 供应商名称快照
/// * `totals` - 表头金额三元组
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回采购草稿提交头。
///
/// # 错误
/// 供应商或商务版本缺失、快照字段非法时返回错误。
///
/// # 关键业务约束
/// 供应商修订和付款条件在创建事务内重新读取并冻结。
pub async fn build_draft_submission(
    port: &dyn CreationBasisSupplierPort,
    order_id: &PurchaseOrderId,
    scope: &BasisScope,
    supplier_name: &str,
    totals: (Amount, Amount, Amount),
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderSubmission> {
    let supplier = port
        .supplier_role(&scope.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision_id = supplier
        .current_commercial_profile_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
    let payment_term = port.payment_term(&scope.payment_term_code)?;
    PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order_id.clone(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: scope.supplier_id.clone(),
            purchase_type: scope.purchase_type,
            fulfillment_responsibility: scope.fulfillment_responsibility,
            supplier_revision_id: revision_id,
            supplier_snapshot: SupplierSnapshot::new(supplier_name.to_string())?,
            payment_term_snapshot: crate::entity::purchase_order::PaymentTermSnapshot::new(
                payment_term.canonical_code,
                payment_term.prepay_gate,
                None,
                None,
                |code| port.payment_snapshot(code),
            )?,
            gross_amount: totals.0,
            net_amount: totals.1,
            tax_amount: totals.2,
        },
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::facts::{PaymentTermFact, SupplierRoleFact};
    use crate::entity::purchase_order::{FulfillmentResponsibility, PurchaseType};
    use async_trait::async_trait;
    use erp_core::ids::{SupplierAccountId, SupplierCommercialProfileRevisionId};
    use std::sync::Mutex;

    struct RecordingExecutor {
        marker: u64,
    }
    impl Executor for RecordingExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            self.marker += 1;
            None
        }
    }
    enum SupplierOutcome {
        Missing,
        NoRevision,
        Ready,
        Failure,
    }
    struct SupplierPort {
        expected_executor: usize,
        outcome: SupplierOutcome,
        calls: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl CreationBasisSupplierPort for SupplierPort {
        async fn supplier_role(
            &self,
            id: &SupplierAccountId,
            executor: &mut dyn Executor,
        ) -> crate::Result<Option<SupplierRoleFact>> {
            assert_eq!(
                executor as *mut dyn Executor as *mut () as usize,
                self.expected_executor
            );
            self.calls.lock().unwrap().push(format!("supplier:{id}"));
            match self.outcome {
                SupplierOutcome::Failure => Err(Error::Internal("supplier read failed".to_string())),
                SupplierOutcome::Missing => Ok(None),
                SupplierOutcome::NoRevision => Ok(Some(SupplierRoleFact {
                    current_commercial_profile_revision_id: None,
                })),
                SupplierOutcome::Ready => Ok(Some(SupplierRoleFact {
                    current_commercial_profile_revision_id: Some(SupplierCommercialProfileRevisionId::new(
                        "profile-1",
                    )),
                })),
            }
        }
        fn payment_term(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
            self.calls.lock().unwrap().push(format!("payment:{code}"));
            if code == "INVALID" {
                return Err(erp_core::Error::from("invalid supplier payment term"));
            }
            Ok(PaymentTermFact {
                canonical_code: "POSTPAY_NET30".to_string(),
                prepay_gate: false,
                days_after_delivery: Some(30),
            })
        }
        fn payment_snapshot(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
            self.payment_term(code)
        }
    }
    fn scope(code: &str) -> BasisScope {
        BasisScope {
            supplier_id: SupplierAccountId::new("supplier-1"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: code.to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
        }
    }
    async fn invoke(outcome: SupplierOutcome, code: &str) -> (Result<PurchaseOrderSubmission>, Vec<String>) {
        let mut executor = RecordingExecutor { marker: 37 };
        let port = SupplierPort {
            expected_executor: (&mut executor as *mut RecordingExecutor) as usize,
            outcome,
            calls: Mutex::new(Vec::new()),
        };
        let zero = super::super::zero_amount();
        let result = build_draft_submission(
            &port,
            &PurchaseOrderId::new("po-1"),
            &scope(code),
            "供应商名称",
            (zero, zero, zero),
            &mut executor,
        )
        .await;
        assert_eq!(executor.marker, 37);
        (result, port.calls.into_inner().unwrap())
    }
    /// 同一非零大小执行器重读供应商后，才解析付款并冻结草稿快照。
    #[tokio::test]
    async fn draft_supplier_port_preserves_executor_and_read_then_parse_order() {
        let (result, calls) = invoke(SupplierOutcome::Ready, "NET-30").await;
        let submission = result.unwrap();
        assert_eq!(
            calls,
            vec!["supplier:supplier-1", "payment:NET-30", "payment:POSTPAY_NET30"]
        );
        assert_eq!(submission.supplier_revision_id.as_ref(), "profile-1");
        assert_eq!(submission.supplier_snapshot.supplier_name, "供应商名称");
        assert_eq!(
            submission.payment_term_snapshot.payment_term_code,
            "POSTPAY_NET30"
        );
    }
    /// 供应商缺失在任何付款解析或草稿构造前返回原首错。
    #[tokio::test]
    async fn missing_supplier_stops_before_payment_parse() {
        let (result, calls) = invoke(SupplierOutcome::Missing, "INVALID").await;
        assert!(matches!(result,Err(Error::NotFound(message)) if message=="供应商不存在"));
        assert_eq!(calls, vec!["supplier:supplier-1"]);
    }
    /// 当前商务指针缺失保持原业务错误，不能被非法付款条件覆盖。
    #[tokio::test]
    async fn missing_commercial_pointer_stops_before_payment_parse() {
        let (result, calls) = invoke(SupplierOutcome::NoRevision, "INVALID").await;
        assert!(
            matches!(result,Err(Error::BusinessLogicError(message)) if message=="供应商缺少商务结算版本")
        );
        assert_eq!(calls, vec!["supplier:supplier-1"]);
    }
    /// 提供方读取失败直接传播，后续解析与构造均不执行。
    #[tokio::test]
    async fn supplier_failure_stops_before_payment_parse() {
        let (result, calls) = invoke(SupplierOutcome::Failure, "NET-30").await;
        assert!(matches!(result,Err(Error::Internal(message)) if message=="supplier read failed"));
        assert_eq!(calls, vec!["supplier:supplier-1"]);
    }
    /// 付款解析错误发生在供应商存在与商务指针校验之后。
    #[tokio::test]
    async fn payment_failure_follows_supplier_validation() {
        let (result, calls) = invoke(SupplierOutcome::Ready, "INVALID").await;
        assert!(matches!(result, Err(Error::Logic(_))));
        assert_eq!(calls, vec!["supplier:supplier-1", "payment:INVALID"]);
    }
}
