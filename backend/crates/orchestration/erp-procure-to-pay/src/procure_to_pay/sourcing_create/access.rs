//! 全部采购候选的发起人对象授权先于库存仓发或采购正式编号分配。

use async_trait::async_trait;
use erp_procurement::entity::purchase_order::{SourcingDraftPlan, SourcingPlan};
use erp_sales::entity::sales_order::SalesOrder;
use mongodb::Database;
use persistence_core::Executor;

use super::CreateFromSourcingApplyInput;
use crate::Result;
use crate::procure_to_pay::PurchaseOrderPorts;
use crate::procure_to_pay::creation_basis::create::prepare_basis_order;

#[async_trait]
trait CandidateAccess: Sync {
    type Candidate: Sync;
    async fn authorize(&self, candidate: &Self::Candidate, executor: &mut dyn Executor) -> Result<()>;
}

async fn authorize_all<A: CandidateAccess>(
    access: &A,
    candidates: &[A::Candidate],
    executor: &mut dyn Executor,
) -> Result<()> {
    for candidate in candidates {
        access.authorize(candidate, executor).await?;
    }
    Ok(())
}

struct PlannedAccess<'a, 'input> {
    db: &'a Database,
    ports: &'a PurchaseOrderPorts,
    input: &'a CreateFromSourcingApplyInput<'input>,
    order: &'a SalesOrder,
}

#[async_trait]
impl CandidateAccess for PlannedAccess<'_, '_> {
    type Candidate = SourcingDraftPlan;

    async fn authorize(&self, draft: &SourcingDraftPlan, executor: &mut dyn Executor) -> Result<()> {
        let candidate = prepare_basis_order(
            self.db,
            self.order,
            &draft.group,
            &self.input.req.work_item_id,
            self.input.actor,
            draft.target_warehouse_id.clone(),
            executor,
        )
        .await?;

        self.ports
            .access(self.db.clone())
            .ensure_create_and_submit(self.input.actor, &candidate, executor)
            .await?;
        Ok(())
    }
}

// 在原事务内先验证完整拆单批次的采购候选，不分配任何业务单号。
//
// db、ports、input 为原命令设施及身份；order、plan 为 guard 后计划；executor 为原事务。全部采购候选的创建和提交范围均成立时成功。
//
// 任一候选构造、主属组织或发起人对象授权失败时终止批次。
pub(super) async fn authorize_planned_orders(
    db: &Database,
    ports: &PurchaseOrderPorts,
    input: &CreateFromSourcingApplyInput<'_>,
    order: &SalesOrder,
    plan: &SourcingPlan,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorize_all(&PlannedAccess { db, ports, input, order }, plan.purchase_plans(), executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    struct Access {
        reject: Option<u8>,
        calls: Mutex<Vec<(u8, usize)>>,
    }

    #[async_trait]
    impl CandidateAccess for Access {
        type Candidate = u8;
        async fn authorize(&self, candidate: &u8, executor: &mut dyn Executor) -> Result<()> {
            self.calls.lock().unwrap().push((*candidate, executor as *mut dyn Executor as *mut () as usize));
            if self.reject == Some(*candidate) {
                return Err(Error::Forbidden("采购候选超出范围".into()));
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn complete_candidate_batch_uses_the_original_executor() {
        let access = Access { reject: None, calls: Mutex::new(Vec::new()) };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;

        authorize_all(&access, &[1, 2, 3], &mut executor).await.unwrap();
        assert_eq!(*access.calls.lock().unwrap(), vec![(1, expected), (2, expected), (3, expected)]);
    }

    #[tokio::test]
    async fn denied_candidate_stops_the_batch_and_empty_batch_needs_no_candidate_permission() {
        let access = Access { reject: Some(2), calls: Mutex::new(Vec::new()) };
        let mut executor = SnapshotExecutor::default();

        assert!(matches!(authorize_all(&access, &[1, 2, 3], &mut executor).await, Err(Error::Forbidden(_))));
        assert_eq!(access.calls.lock().unwrap().iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![1, 2]);

        access.calls.lock().unwrap().clear();
        authorize_all(&access, &[], &mut executor).await.unwrap();
        assert!(access.calls.lock().unwrap().is_empty());
    }
}
