//! 门户统一申请操作通过精确领域类型分派，协议层不读取业务仓储。

use application_core::AuditActor;
use erp_identity::PortalActor;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::request_access::{RequestKind, RequestTarget};
use super::{PortalReview, PortalTransition, SupplierPortalProcess};
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 按拥有领域修改当前供应商可编辑申请。
    /// # 参数
    /// 精确申请、对应草稿DTO与真实门户身份。
    /// # 返回
    /// 返回持久化领域结果，协议层须作外部允许列表投影。
    /// # 错误
    /// 不存在、字段非法、范围或版本失效时拒绝。
    pub async fn application_update(&self, id: &str, input: Value, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, actor).await? {
            RequestKind::OfferingCreate | RequestKind::OfferingUpdate => {
                value(self.application_save(Some(id), parse(input)?, actor).await?)
            },
            RequestKind::NewProduct => value(self.new_product_save(Some(id), parse(input)?, actor).await?),
            RequestKind::Cooperation => value(self.cooperation_save(Some(id), parse(input)?, actor).await?),
        }
    }

    /// 冻结统一入口申请并建立对应采购确认任务。
    /// # 参数
    /// 精确申请、原版本命令与门户身份。
    /// # 返回
    /// 返回实际提交结果。
    /// # 错误
    /// 领域类型、归属、输入或处理资格不符时拒绝。
    pub async fn submit(&self, id: &str, input: &PortalTransition, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, actor).await? {
            RequestKind::OfferingCreate | RequestKind::OfferingUpdate => {
                value(self.application_submit(id, input.clone(), actor).await?)
            },
            RequestKind::NewProduct => value(self.new_product_submit(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_submit(id, input.clone(), actor).await?),
        }
    }

    /// 撤回统一入口的当前待确认申请及任务。
    /// # 参数
    /// 精确申请、版本命令及门户身份。
    /// # 返回
    /// 返回保留冻结历史的撤回结果。
    /// # 错误
    /// 越界、已完成或并发变更时拒绝。
    pub async fn withdraw(&self, id: &str, input: &PortalTransition, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, actor).await? {
            RequestKind::OfferingCreate | RequestKind::OfferingUpdate => {
                value(self.application_withdraw(id, input.clone(), actor).await?)
            },
            RequestKind::NewProduct => value(self.new_product_withdraw(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_withdraw(id, input.clone(), actor).await?),
        }
    }

    /// 由当前具体内部采购人作领域对应的原子决定。
    /// # 参数
    /// 申请、任务版本、明确映射和内部身份。
    /// # 返回
    /// 返回正式结果或退回意见。
    /// # 错误
    /// 任一对象或处理资格失效时拒绝。
    pub async fn review(&self, id: &str, input: &PortalReview, actor: &AuditActor) -> Result<Value> {
        match self.review_kind(id, &input.work_item_id, actor).await? {
            RequestKind::OfferingCreate | RequestKind::OfferingUpdate => {
                value(self.application_review(id, input.clone(), actor).await?)
            },
            RequestKind::NewProduct => value(self.new_product_review(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_review(id, input.clone(), actor).await?),
        }
    }

    /// 分派预检只交付类型；真实任务完整资格与类型读取使用同一快照。
    async fn review_kind(&self, id: &str, task_id: &str, actor: &AuditActor) -> Result<RequestKind> {
        let (this, id, task_id, actor) = (self.clone(), id.to_owned(), task_id.to_owned(), actor.clone());
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { this.review_kind_stage(&id, &task_id, &actor, executor).await })
            })
            .await
    }

    async fn review_kind_stage(
        &self,
        id: &str,
        task_id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<RequestKind> {
        self.require_review_result(id, task_id, actor, executor).await?;
        self.load_request_kind(id, None, executor).await
    }

    async fn request_kind_for(&self, id: &str, actor: &PortalActor) -> Result<RequestKind> {
        self.session_validate(actor, &mut NoTransaction).await?;
        self.load_request_kind(id, Some(actor.supplier_id.as_str()), &mut NoTransaction).await
    }

    async fn load_request_kind(
        &self,
        id: &str,
        supplier_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<RequestKind> {
        RequestTarget::load(&self.db, id, executor)
            .await?
            .and_then(|target| target.kind_for_supplier(supplier_id))
            .ok_or_else(|| Error::NotFound("申请不存在或无权查看".into()))
    }
}

fn parse<T: DeserializeOwned>(input: Value) -> Result<T> {
    serde_json::from_value(input)
        .map_err(|_| Error::ValidationError("申请内容不符合要求，请核对表单中填写的资料后重新提交".into()))
}
fn value<T: Serialize>(result: T) -> Result<Value> {
    serde_json::to_value(result).map_err(|error| Error::Internal(error.to_string()))
}

#[cfg(test)]
mod authorization_tests {
    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[tokio::test]
    async fn internal_dispatch_requires_complete_task_qualification_before_request_reads() {
        let process = super::super::testing::unavailable_process().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let result =
            process.review_kind_stage("request", "task", &actor, &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }
}
