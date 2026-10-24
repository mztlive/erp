//! 逐行保存费用占用，同一调用内复用已保存询价，保留每行版本递增与首错顺序。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderSubmissionLine};
use erp_sales::entity::sales_inquiry::SalesInquiry;
use mongodb::Database;
use persistence_core::Executor;

use super::persistence::{ExpensePersistence, MongoExpensePersistence, confirm_payee};
use super::reservation;
use crate::Result;

/// 核验收款依据后登记采购费用来源占用。
///
/// # 参数
/// `db`、`executor` 为原建单事务；其余参数为已验证采购与费用行。
/// # 返回
/// 来源为空时不写入；存在来源时逐行保存跨轮次占用。
/// # 错误
/// 来源失效、销售归属、费用条件、收款依据或额度不符时拒绝；保存失败立即停止。
pub(crate) async fn reserve_purchase(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    reserve_lines(&MongoExpensePersistence::new(db), order, lines, actor.id(), executor).await
}

async fn reserve_lines(
    persistence: &impl ExpensePersistence,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut inquiries = HashMap::<String, SalesInquiry>::new();
    for line in lines {
        let Some(detail) = line.expense.as_ref() else { continue };
        let Some(source) = detail.inquiry_source.as_ref() else { continue };
        let source = source.normalized()?;
        let mut inquiry = match inquiries.remove(&source.inquiry_id) {
            Some(inquiry) => inquiry,
            None => persistence.inquiry(&source.inquiry_id, executor).await?,
        };
        inquiry.fee_for_terms(
            source.feedback_round,
            &source.fee_id,
            line.gross_amount,
            line.input_tax_rate,
        )?;
        confirm_payee(persistence, &inquiry, &source, order, actor_id, executor).await?;
        let reserved = reservation(
            &source,
            order.sales_order_id.as_ref(),
            &order.base.id,
            &detail.expense_line_id,
            line.gross_amount,
        );
        inquiry.reserve_fee(reserved, Instant::now())?;
        // CAS 成功后 save 同步内存版本；不得将逐行持久化合并成末尾一次保存。
        persistence.save(&mut inquiry, executor).await?;
        inquiries.insert(source.inquiry_id, inquiry);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use entity_core::BaseModel;
    use erp_core::ids::{
        PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
        SalesOrderRevisionId, SupplierAccountId,
    };
    use erp_core::money::Amount;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, InquiryExpenseSource, PurchaseExpenseCategory, PurchaseExpenseDetail,
        PurchaseLineType, PurchaseOrderData, PurchaseOrderSubmissionLineData, PurchaseType,
    };
    use erp_sales::entity::sales_inquiry::{
        InquiryAcceptance, InquiryCreateData, InquiryFee, InquiryFeeAllocation, InquiryFeedbackData,
        InquiryFeedbackSnapshot, InquiryRequirementData, InquiryStatus,
    };
    use erp_supply::inquiry::{InquiryFeeBasis, InquiryFeeBasisData};
    use mongodb::ClientSession;

    use super::*;
    use crate::Error;

    struct TestExecutor {
        identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut ClientSession> {
            assert_eq!(self.identity, 42);
            None
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Step {
        Load(String),
        Bases(String),
        Append,
        Save(String, u64),
    }
    struct Store {
        inquiries: HashMap<String, SalesInquiry>,
        bases: Vec<InquiryFeeBasis>,
        calls: Vec<(Step, usize)>,
        fail_save: bool,
    }
    struct RecordedPersistence(Mutex<Store>);
    impl RecordedPersistence {
        fn new(ids: &[&str], with_basis: bool) -> Self {
            Self(Mutex::new(Store {
                inquiries: ids.iter().map(|id| (id.to_string(), inquiry(id))).collect(),
                bases: if with_basis { ids.iter().map(|id| basis(id)).collect() } else { vec![] },
                calls: vec![],
                fail_save: false,
            }))
        }
        fn record(store: &mut Store, step: Step, executor: &mut dyn Executor) {
            executor.session();
            store.calls.push((step, executor as *mut dyn Executor as *mut () as usize));
        }
    }
    #[async_trait]
    impl ExpensePersistence for RecordedPersistence {
        async fn inquiry(&self, id: &str, executor: &mut dyn Executor) -> Result<SalesInquiry> {
            let mut store = self.0.lock().unwrap();
            Self::record(&mut store, Step::Load(id.into()), executor);
            store.inquiries.get(id).cloned().ok_or_else(|| Error::NotFound(id.into()))
        }
        async fn save(&self, inquiry: &mut SalesInquiry, executor: &mut dyn Executor) -> Result<()> {
            let mut store = self.0.lock().unwrap();
            Self::record(&mut store, Step::Save(inquiry.base.id.clone(), inquiry.base.version), executor);
            if store.fail_save {
                return Err(Error::ConflictError("保存失败".into()));
            }
            assert_eq!(store.inquiries[&inquiry.base.id].base.version, inquiry.base.version);
            inquiry.base.version += 1;
            store.inquiries.insert(inquiry.base.id.clone(), inquiry.clone());
            Ok(())
        }
        async fn fee_bases(
            &self,
            id: &str,
            version: u32,
            round: u32,
            executor: &mut dyn Executor,
        ) -> Result<Vec<InquiryFeeBasis>> {
            let mut store = self.0.lock().unwrap();
            Self::record(&mut store, Step::Bases(id.into()), executor);
            assert_eq!((version, round), (1, 1));
            Ok(store.bases.iter().filter(|basis| basis.data.inquiry_id == id).cloned().collect())
        }
        async fn append_basis(&self, basis: &InquiryFeeBasis, executor: &mut dyn Executor) -> Result<()> {
            let mut store = self.0.lock().unwrap();
            Self::record(&mut store, Step::Append, executor);
            store.bases.push(basis.clone());
            Ok(())
        }
    }

    fn inquiry(id: &str) -> SalesInquiry {
        let mut inquiry = SalesInquiry::create(
            id.into(),
            format!("IQ-{id}"),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(100),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData::default(),
            },
        )
        .unwrap();
        inquiry.base = BaseModel { id: id.into(), ..BaseModel::fake() };
        inquiry.status = InquiryStatus::Confirmed;
        inquiry.requirement_version = 1;
        inquiry.feedback_round = 1;
        inquiry.feedbacks.push(InquiryFeedbackSnapshot {
            round: 1,
            requirement_version: 1,
            published_by: "buyer".into(),
            published_at: Instant::from_unix_secs(10),
            data: InquiryFeedbackData {
                lines: vec![],
                fees: vec![InquiryFee {
                    fee_id: "fee".into(),
                    name: "运费".into(),
                    amount_gross: "100".parse().unwrap(),
                    tax_rate: "0".parse().unwrap(),
                    line_ids: vec![],
                    valid_until: Instant::from_unix_secs(0),
                    public_note: String::new(),
                }],
            },
        });
        inquiry.acceptances.push(InquiryAcceptance {
            requirement_version: 1,
            feedback_round: 1,
            accepted_by: "sales".into(),
            accepted_at: Instant::from_unix_secs(20),
        });
        inquiry.fee_allocations.push(InquiryFeeAllocation {
            allocation_id: "allocation".into(),
            sales_order_id: "sales".into(),
            feedback_round: 1,
            fee_id: "fee".into(),
            amount: "100".parse().unwrap(),
        });
        inquiry
    }

    fn order() -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new("purchase"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: String::new(),
                sales_order_id: SalesOrderId::new("sales"),
                sales_order_revision_id: SalesOrderRevisionId::new("sales-revision"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                purchase_type: PurchaseType::Expense,
                payment_term_code: "net30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::NotApplicable,
                owner_user_id: "buyer".into(),
                target_warehouse_id: None,
            },
            "buyer",
            |code| {
                Ok(PaymentTermFact {
                    canonical_code: code.into(),
                    prepay_gate: false,
                    prepay_minimum_ratio: None,
                    days_after_delivery: Some(30),
                    calendar_due: None,
                })
            },
        )
        .unwrap()
    }
    fn basis(id: &str) -> InquiryFeeBasis {
        InquiryFeeBasis::new(
            format!("basis-{id}"),
            InquiryFeeBasisData {
                inquiry_id: id.into(),
                requirement_version: 1,
                feedback_round: 1,
                fee_id: "fee".into(),
                payee_supplier_id: SupplierAccountId::new("supplier"),
                payment_term_code: "net30".into(),
                internal_note: String::new(),
                attachment_ids: vec![],
                recorded_by: "buyer".into(),
            },
        )
        .unwrap()
    }
    fn line(id: &str, inquiry_id: &str, value: &str) -> PurchaseOrderSubmissionLine {
        let mut expense =
            PurchaseExpenseDetail::new(id.into(), PurchaseExpenseCategory::Logistics, "运费".into(), None)
                .unwrap();
        expense.inquiry_source = Some(InquiryExpenseSource {
            inquiry_id: inquiry_id.into(),
            feedback_round: 1,
            fee_id: "fee".into(),
            allocation_id: "allocation".into(),
        });
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("submission"),
                line_no: 1,
                line_type: PurchaseLineType::LogisticsFee,
                procurement_confirmation_line_id: None,
                sku_id: None,
                sku_revision_id: None,
                supplier_offering_source: None,
                delivery_allocations: vec![],
                product_name_snapshot: None,
                specification_snapshot: None,
                quantity: None,
                base_unit_code: None,
                unit_cost_gross: None,
                gross_amount: value.parse().unwrap(),
                net_amount: value.parse().unwrap(),
                tax_amount: Amount::zero(),
                input_tax_rate: Some("0".parse().unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: None,
                sales_order_revision_line_id: None,
                sales_order_submission_line_id: None,
                allocated_quantity: None,
                expense: Some(expense),
            },
        )
        .unwrap()
    }

    #[tokio::test]
    async fn reuses_saved_inquiries_but_keeps_each_cas_save_and_single_basis_read() {
        let persistence = RecordedPersistence::new(&["first", "second"], true);
        let mut executor = TestExecutor { identity: 42 };
        let expected_executor = &mut executor as *mut TestExecutor as usize;
        reserve_lines(
            &persistence,
            &order(),
            &[line("a", "first", "30"), line("b", "second", "20"), line("c", "first", "40")],
            "buyer",
            &mut executor,
        )
        .await
        .unwrap();
        let store = persistence.0.lock().unwrap();
        assert_eq!(
            store.calls.iter().map(|(step, _)| step).collect::<Vec<_>>(),
            [
                &Step::Load("first".into()),
                &Step::Bases("first".into()),
                &Step::Save("first".into(), 1),
                &Step::Load("second".into()),
                &Step::Bases("second".into()),
                &Step::Save("second".into(), 1),
                &Step::Bases("first".into()),
                &Step::Save("first".into(), 2),
            ]
        );
        assert!(store.calls.iter().all(|(_, executor)| *executor == expected_executor));
        let first = &store.inquiries["first"];
        assert_eq!(first.base.version, 3);
        assert_eq!(first.fee_ledger.len(), 2);
        assert_eq!(first.fee_ledger[0].amount, "30".parse().unwrap());
        assert_eq!(first.fee_ledger[1].amount, "40".parse().unwrap());
    }

    #[tokio::test]
    async fn cached_inquiry_keeps_cumulative_balance_and_first_error_order() {
        let persistence = RecordedPersistence::new(&["first", "later"], true);
        let result = reserve_lines(
            &persistence,
            &order(),
            &[line("a", "first", "60"), line("b", "first", "60"), line("c", "later", "1")],
            "buyer",
            &mut TestExecutor { identity: 42 },
        )
        .await;
        assert!(
            matches!(result, Err(Error::BusinessLogicError(message)) if message == "费用来源或销售分摊可用余额不足")
        );
        let store = persistence.0.lock().unwrap();
        assert_eq!(store.inquiries["first"].fee_ledger.len(), 1);
        assert_eq!(store.inquiries["first"].base.version, 2);
        assert_eq!(store.calls.len(), 4);
        assert_eq!(store.calls[3].0, Step::Bases("first".into()));
    }

    #[tokio::test]
    async fn failed_save_stops_before_loading_the_next_line_and_matching_basis_is_not_reread() {
        let persistence = RecordedPersistence::new(&["first", "later"], true);
        persistence.0.lock().unwrap().fail_save = true;
        let result = reserve_lines(
            &persistence,
            &order(),
            &[line("a", "first", "30"), line("b", "later", "20")],
            "buyer",
            &mut TestExecutor { identity: 42 },
        )
        .await;
        assert!(matches!(result, Err(Error::ConflictError(message)) if message == "保存失败"));
        let store = persistence.0.lock().unwrap();
        assert_eq!(store.calls.len(), 3);
        assert_eq!(store.calls[2].0, Step::Save("first".into(), 1));
        assert!(store.inquiries["first"].fee_ledger.is_empty());
    }

    #[tokio::test]
    async fn missing_basis_is_written_then_reread_before_reservation_save() {
        let persistence = RecordedPersistence::new(&["first"], false);
        reserve_lines(
            &persistence,
            &order(),
            &[line("a", "first", "30")],
            "buyer",
            &mut TestExecutor { identity: 42 },
        )
        .await
        .unwrap();
        let store = persistence.0.lock().unwrap();
        assert_eq!(store.calls.len(), 5);
        assert_eq!(store.calls[2].0, Step::Append);
        assert_eq!(store.calls[3].0, Step::Bases("first".into()));
        assert_eq!(store.calls[4].0, Step::Save("first".into(), 1));
        assert_eq!(store.bases[0].data.recorded_by, "buyer");
        assert_eq!(store.inquiries["first"].fee_ledger[0].source_line_id, "a");
    }

    #[tokio::test]
    async fn duplicate_source_replay_keeps_per_line_versions_without_double_reservation() {
        let persistence = RecordedPersistence::new(&["first"], true);
        let line = line("a", "first", "30");
        reserve_lines(
            &persistence,
            &order(),
            &[line.clone(), line],
            "buyer",
            &mut TestExecutor { identity: 42 },
        )
        .await
        .unwrap();
        let store = persistence.0.lock().unwrap();
        assert_eq!(store.inquiries["first"].fee_ledger.len(), 1);
        assert_eq!(store.inquiries["first"].base.version, 3);
        assert_eq!(store.calls.len(), 5);
        assert_eq!(store.calls[4].0, Step::Save("first".into(), 2));
    }
}
