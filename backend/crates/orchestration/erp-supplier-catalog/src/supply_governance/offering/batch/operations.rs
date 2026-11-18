//! 批量条款与可供更新复用单条供给用例。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_supply::dto::supplier_offering::{
    ReviseSupplierOfferingRequest, UpdateSupplierOfferingAvailabilityRequest,
};
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::Transactional;
use serde::Serialize;
use serde_json::Value;

use super::{BatchOperation, Targeted};
use crate::supply_governance::SupplierOfferingProcess;
use crate::{Error, Result};

// 序列化已完成命令，不携带数据库错误细节。
fn result_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| Error::Internal("序列化供给结果失败".into()))
}

// 预检只回放已完成命令，待写入准备不跨请求缓存。
fn replay<P, R: Serialize>(prepared: CommandPreparation<P, R>) -> Result<Option<Value>> {
    match prepared {
        CommandPreparation::Apply(_) => Ok(None),
        CommandPreparation::Replay(result) => result_value(result).map(Some),
    }
}

#[async_trait]
impl BatchOperation for Targeted<ReviseSupplierOfferingRequest> {
    // 同批一个供给只追加一次修订。返回 `offering_id`。
    fn identity(&self) -> String {
        self.offering_id.clone()
    }
    // 复用原修订命令键。返回可改写的 `command.idempotency_key`。
    fn key(&mut self) -> &mut String {
        &mut self.command.idempotency_key
    }
    // 使用真实修订预检，包括授权、资格和期望修订号。读取使用同一快照事务。
    //
    // `process`：供给流程，提供领域服务与资质端口。
    //
    // 尚未执行时返回 `None`；已有修订结果时返回其 JSON。
    //
    // 修订预检失败时返回对应错误；结果无法序列化时返回 `Internal`。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>> {
        let domain = process.domain();
        let qualification = process.qualification.clone();
        let input = self.clone();
        let actor = actor.clone();
        let prepared = process
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    domain
                        .prepare_revise(
                            &input.offering_id,
                            &input.command,
                            &actor,
                            qualification.as_ref(),
                            executor,
                        )
                        .await
                })
            })
            .await?;
        replay(prepared)
    }
    // 追加条款，保留当前实时可供数量。
    //
    // 返回修订结果的 JSON。
    //
    // 单条修订失败时返回对应错误；结果无法序列化时返回 `Internal`。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value> {
        result_value(process.revise(&self.offering_id, self.command, actor).await?)
    }
}

#[async_trait]
impl BatchOperation for Targeted<UpdateSupplierOfferingAvailabilityRequest> {
    // 同批一个供给只更新一次可供状态。返回 `offering_id`。
    fn identity(&self) -> String {
        self.offering_id.clone()
    }
    // 复用原可供命令键。返回可改写的 `command.idempotency_key`。
    fn key(&mut self) -> &mut String {
        &mut self.command.idempotency_key
    }
    // 可供批量更新必须提供列表读取时的版本。领域预检使用同一快照事务。
    //
    // 尚未执行时返回 `None`；已有可供结果时返回其 JSON。
    //
    // `command.expected_version` 缺失时返回 `ValidationError`；预检失败时返回对应错误；结果无法序列化时返回 `Internal`。
    async fn prepare(&self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Option<Value>> {
        if self.command.expected_version.is_none() {
            return Err(Error::ValidationError("缺少可供状态版本，请刷新供给列表".into()));
        }

        let domain = process.domain();
        let input = self.clone();
        let actor = actor.clone();
        let prepared = process
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    domain
                        .prepare_availability(&input.offering_id, &input.command, &actor, executor)
                        .await
                        .map_err(Error::from)
                })
            })
            .await?;
        replay(prepared)
    }
    // 只更新可供状态与数量，不创建商业条款修订。
    //
    // 返回可供更新结果的 JSON。
    //
    // 单条更新失败时返回对应错误；结果无法序列化时返回 `Internal`。
    async fn execute(self, process: &SupplierOfferingProcess, actor: &AuditActor) -> Result<Value> {
        result_value(process.update_availability(&self.offering_id, self.command, actor).await?)
    }
}
