//! 登记与幂等回放都在原事务重验任务的当前业务资格。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_order_to_cash::sales_inquiry::access::SalesInquiryAccess;
use erp_permissions::{internal, sales_inquiry as sales_inquiry_permissions, supplier_offering};
use persistence_core::Executor;
use serde_json::Value;

use super::imports::{authorize_inquiry, required, version};
use super::{BusinessTransfers, TransferHandler, TransferKind};
use crate::{Error, Result};

impl BusinessTransfers {
    // 私有读取先检查当前功能和真实业务目标，不把历史版本当作当前任务事实。
    //
    // kind、input 为保存的分类和业务引用，actor、executor 为当前身份及读取快照。
    //
    // 当前功能及可由轻量引用证明的业务目标均获授权时成功。
    //
    // 当前身份、功能、任务关系或业务目标资格失效时拒绝。
    pub(super) async fn read_access(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access_with(kind, input, actor, executor).await?;
        match kind {
            TransferKind::InquiryPreview => {
                self.services.inquiry.require_import(actor, required(input, "inquiry_id")?, executor).await?;
            },
            TransferKind::InquiryCreationPreview => {
                self.services
                    .inquiry
                    .creation_header(actor, required(input, "customer_id")?, executor)
                    .await?;
            },
            TransferKind::SupplierQuotePreview | TransferKind::SupplierQuoteArchive => {
                SalesInquiryAccess::new(self.db.clone(), self.authorization.clone())
                    .require_task(
                        actor,
                        &sales_inquiry_permissions::INTERNAL_FEE_BASES,
                        required(input, "inquiry_id")?,
                        required(input, "task_id")?,
                        None,
                        executor,
                    )
                    .await?;
            },
            _ => {},
        }
        Ok(())
    }

    // 在文件归档后的登记事务中执行当前功能及实际目标检查。
    //
    // kind、input 为本次任务类型与业务参数，actor、executor 为当前身份及原事务。
    //
    // 当前资格和所属业务目标均允许时成功。
    //
    // 身份、权限、目标或版本不符时拒绝。
    pub(super) async fn submission_access(
        &self,
        kind: TransferKind,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.access_with(kind, input, actor, executor).await?;
        match kind {
            TransferKind::InquiryPreview | TransferKind::InquiryCreationPreview => {
                authorize_inquiry(&self.services.inquiry, kind, input, actor, executor).await
            },
            TransferKind::SupplyBatch => self.supply_submission(input, actor, executor).await,
            TransferKind::SupplierQuotePreview | TransferKind::SupplierQuoteArchive => {
                self.quote_submission(input, actor, executor).await
            },
            // 导出登记只保存查询输入，数据由执行和交付用例重新授权；门户维持会话写资格。
            _ => Ok(()),
        }
    }

    // 逐真实供给目标重验固定动作，批处理本身仍由原用例执行。
    async fn supply_submission(
        &self,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let permission = match required(input, "mode")? {
            "revise" => supplier_offering::REVISE,
            "availability" => internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE,
            _ => return Err(Error::ValidationError("不支持的批量供给操作".into())),
        };
        let rows = input["rows"]
            .as_array()
            .filter(|rows| !rows.is_empty())
            .ok_or_else(|| Error::ValidationError("请选择供给处理行".into()))?;

        for row in rows {
            self.authorization
                .require(
                    actor,
                    &permission,
                    AccessTarget::record(required(&row["input"], "offering_id")?)?,
                    executor,
                )
                .await?;
        }
        Ok(())
    }

    // 固定采购任务身份和版本在登记快照内保持原询价资格。
    async fn quote_submission(
        &self,
        input: &Value,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let task_version = input["task_version"]
            .as_u64()
            .ok_or_else(|| Error::ValidationError("缺少当前采购任务版本".into()))?;
        let (inquiry, _) = SalesInquiryAccess::new(self.db.clone(), self.authorization.clone())
            .require_task(
                actor,
                &sales_inquiry_permissions::INTERNAL_FEE_BASES,
                required(input, "inquiry_id")?,
                required(input, "task_id")?,
                Some(task_version),
                executor,
            )
            .await?;
        inquiry.ensure_version(version(input)?)?;
        Ok(())
    }
}

// 公共流程只向原事务开放登记检查；接入缺失不能降为预检结果。
//
// handler 为已装配业务适配器，kind、input 为固定任务，actor、executor 为当前身份及登记事务。
//
// 业务在原事务授权通过时成功。
//
// 非快照执行器或业务授权失败时拒绝。
pub(super) async fn authorize_registration(
    handler: &dyn TransferHandler,
    kind: TransferKind,
    input: &Value,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::ValidationError("任务登记授权必须使用快照事务".into()));
    }
    handler.authorize_submission(kind, input, actor, executor).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;
    use persistence_core::{NoTransaction, ReadExecutor};
    use serde_json::json;

    use super::*;
    use crate::data_transfer::{TransferContext, TransferResult};

    #[derive(Default)]
    struct Handler {
        calls: Mutex<Vec<(TransferKind, Value, String)>>,
        denied: bool,
    }

    #[async_trait]
    impl TransferHandler for Handler {
        async fn authorize_read(
            &self,
            _: TransferKind,
            _: &Value,
            _: &AuditActor,
            _: &mut dyn Executor,
        ) -> Result<()> {
            unreachable!("登记复验不读取私有文件")
        }
        async fn authorize_submission(
            &self,
            kind: TransferKind,
            input: &Value,
            actor: &AuditActor,
            executor: &mut dyn Executor,
        ) -> Result<()> {
            assert!(executor.boundary().is_some_and(|boundary| boundary.is_snapshot()));
            self.calls.lock().unwrap().push((kind, input.clone(), actor.id().into()));
            if self.denied { Err(Error::Forbidden("当前业务权限已撤销".into())) } else { Ok(()) }
        }

        async fn prepare(&self, _: TransferKind, _: Value, _: &AuditActor) -> Result<Value> {
            unreachable!("登记复验不重新准备文件")
        }
        async fn execute(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<TransferResult> {
            unreachable!("登记复验不执行后台业务")
        }
        async fn authorize_result(
            &self,
            _: TransferKind,
            _: &Value,
            _: &Value,
            _: &AuditActor,
        ) -> Result<()> {
            unreachable!("登记复验不交付文件")
        }
        async fn context(
            &self,
            _: TransferKind,
            _: &Value,
            _: Option<&Value>,
            _: &AuditActor,
        ) -> Result<TransferContext> {
            unreachable!("登记复验不恢复业务上下文")
        }
        async fn recover(&self, _: TransferKind, _: &Value, _: &AuditActor) -> Result<Value> {
            unreachable!("登记复验不恢复业务回执")
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("sales-1".into(), "sales".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn registration_requires_snapshot_before_consulting_business() {
        let handler = Handler::default();
        let input = json!({"inquiry_id":"inquiry-1","expected_version":3});
        for executor in [&mut NoTransaction as &mut dyn Executor, &mut ReadExecutor::default()] {
            let error =
                authorize_registration(&handler, TransferKind::InquiryPreview, &input, &actor(), executor)
                    .await
                    .unwrap_err();
            assert!(
                matches!(error, Error::ValidationError(message) if message == "任务登记授权必须使用快照事务")
            );
        }
        assert!(handler.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn registration_checks_original_target_in_the_active_transaction() {
        let handler = Handler::default();
        let input = json!({"customer_id":"customer-1"});
        let mut executor = SnapshotExecutor::default();

        authorize_registration(
            &handler,
            TransferKind::InquiryCreationPreview,
            &input,
            &actor(),
            &mut executor,
        )
        .await
        .unwrap();

        assert_eq!(
            *handler.calls.lock().unwrap(),
            vec![(TransferKind::InquiryCreationPreview, input, "sales-1".into(),)]
        );
    }

    #[tokio::test]
    async fn registration_propagates_revocation_even_after_input_was_prepared() {
        let handler = Handler { denied: true, ..Default::default() };
        let error = authorize_registration(
            &handler,
            TransferKind::InquiryPreview,
            &json!({"inquiry_id":"inquiry-1","expected_version":3}),
            &actor(),
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Forbidden(message) if message == "当前业务权限已撤销"));
        assert_eq!(handler.calls.lock().unwrap().len(), 1);
    }
}
