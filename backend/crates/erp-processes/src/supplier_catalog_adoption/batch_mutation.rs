//! 草稿作业采用可接管的操作标记和逐项短事务，避免整批长事务。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{BatchAdoptionExt, CatalogAdoptionBatch, has_inflight_batch_items};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::SupplierCatalogAdoptionProcess;
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 为保存或核对登记可接管的处理标记，并作废旧预览。
    /// # 参数
    /// 连接、批次、期望版本及当前认证用户。
    /// # 返回
    /// 已保存批次及本次处理标记。
    /// # 错误
    /// 越权、旧版本、正在入库或其他草稿操作尚未结束时拒绝。
    pub(super) async fn begin_batch_draft(
        &self,
        connection: &str,
        id: &str,
        expected: u64,
        actor: &AuditActor,
    ) -> Result<(CatalogAdoptionBatch, String)> {
        self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        let rows = self.all_batch_items(id, &mut NoTransaction).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        let (process, connection, id, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = process.load_batch(&connection, &id, &actor, executor).await?;
                    batch.ensure_version(expected)?;
                    let now = Instant::now().unix_secs();
                    batch.ensure_draft_available(now)?;
                    if has_inflight_batch_items(&process.db, &id, executor).await? {
                        return Err(Error::ConflictError("入库正在执行，请先查看结果".into()));
                    }
                    let token = next_id();
                    batch.draft_operation = Some(token.clone());
                    batch.draft_started_at = Some(now);
                    batch.invalidate_preview();
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok((batch, token))
                })
            })
            .await
    }

    /// 在逐项事务中重验本次草稿操作仍持有写入资格。
    /// # 参数
    /// 连接、批次、本次标记、认证用户及调用方执行器。
    /// # 返回
    /// 当前已授权批次。
    /// # 错误
    /// 范围不足或操作已经被接管时拒绝。
    pub(super) async fn check_batch_draft(
        &self,
        connection: &str,
        id: &str,
        token: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionBatch> {
        let batch = self.load_batch(connection, id, actor, executor).await?;
        batch.ensure_draft_operation(token)?;
        Ok(batch)
    }

    /// 清除本次操作标记，不解除后来接管者的处理状态。
    /// # 参数
    /// 连接、批次、本次标记及当前认证用户。
    /// # 返回
    /// 本次标记已清除或已由其他操作接管。
    /// # 错误
    /// 授权失效或存储失败时返回错误，允许超时后重新读取并接管。
    pub(super) async fn release_batch_draft(
        &self,
        connection: &str,
        id: &str,
        token: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), token.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = process.load_batch(&connection, &id, &actor, executor).await?;
                    if batch.draft_operation.as_deref() == Some(token.as_str()) {
                        batch.draft_operation = None;
                        batch.draft_started_at = None;
                        process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    }
                    Ok(())
                })
            })
            .await
    }
}
