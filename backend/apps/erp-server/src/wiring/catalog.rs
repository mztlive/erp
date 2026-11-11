//! 商品审计、文件资产与待提交附件 adapter。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_catalog::{CatalogAuditPort, FileAssetFact, FileAssetFactsPort, PreparedCatalogAudit};
use erp_core::common::time::Instant;
use erp_core::ids::FileAssetId;
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::audit_conversion::{PreparedAudit, map_audit_error};

/// 把商品审计事实写入 `erp-audit` 的 Mongo adapter。
#[derive(Clone)]
pub struct MongoCatalogAudit {
    db: Database,
}

impl MongoCatalogAudit {
    /// 绑定审计日志所在数据库，构造时不写库。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为商品域可注入的共享审计 Port。
    ///
    /// # 参数
    /// * `db` - 持久化审计日志的数据库。
    ///
    /// # 返回
    /// 返回共享的商品审计 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn CatalogAuditPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl CatalogAuditPort for MongoCatalogAudit {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> erp_catalog::Result<PreparedCatalogAudit> {
        let log = actor.resource_log(action, resource_type, resource_id).map_err(map_audit_to_catalog)?;
        Ok(PreparedCatalogAudit::from_log(&log))
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> erp_catalog::Result<PreparedCatalogAudit> {
        let log = actor
            .resource_log_with_message(action, resource_type, resource_id, message)
            .map_err(map_audit_to_catalog)?;
        Ok(PreparedCatalogAudit::from_log(&log))
    }

    async fn persist(
        &self,
        audit: &PreparedCatalogAudit,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<()> {
        let log = audit.to_log().map_err(map_audit_to_catalog)?;
        persist_log(&self.db, &log, executor).await.map_err(map_audit_to_catalog)?;
        Ok(())
    }
}

/// 为商品媒体与标识读取文件资产是否存在的 Mongo adapter。
#[derive(Clone)]
pub struct MongoCatalogFileAssets {
    db: Database,
}

impl MongoCatalogFileAssets {
    /// 绑定文件资产集合所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 文件资产集合所在数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为商品域可注入的文件资产事实 Port。
    ///
    /// # 参数
    /// * `db` - 文件资产集合所在数据库。
    ///
    /// # 返回
    /// 返回共享的文件资产事实 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn FileAssetFactsPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl FileAssetFactsPort for MongoCatalogFileAssets {
    async fn find_by_id(
        &self,
        asset_id: &FileAssetId,
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<Option<FileAssetFact>> {
        Ok(self
            .db
            .file_assets()
            .find_by_id(asset_id.as_ref(), executor)
            .await
            .map_err(erp_catalog::Error::from)?
            .map(|asset| FileAssetFact {
                usable_image: asset.is_usable_at(Instant::now())
                    && matches!(
                        asset.content_type.as_str(),
                        "image/jpeg" | "image/png" | "image/webp" | "image/gif"
                    ),
                id: asset.base.id,
            }))
    }

    async fn missing_ids(
        &self,
        asset_ids: &[FileAssetId],
        executor: &mut dyn Executor,
    ) -> erp_catalog::Result<Vec<FileAssetId>> {
        self.db
            .file_assets()
            .missing_file_asset_ids(asset_ids, executor)
            .await
            .map_err(erp_catalog::Error::from)
    }
}

map_audit_error!(fn map_audit_to_catalog -> erp_catalog);

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use entity_core::BaseModel;
    use erp_audit::prepare_business_log;
    use erp_core::AccountKind;

    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
    }

    #[test]
    fn prepared_audit_round_trip_freezes_name_and_original_event_metadata() {
        let mut log = actor()
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap()
            .resource_log("product.update", "product", "object-1".into())
            .unwrap()
            .with_event_sequence(7)
            .unwrap();
        log.base = BaseModel {
            id: "audit-original".into(),
            version: 7,
            created_at: 11,
            updated_at: 19,
            deleted_at: 23,
        };
        log.structured_event.as_mut().unwrap().occurred_at = log.base.created_at;
        let log = prepare_business_log(&log).unwrap();
        let prepared = PreparedCatalogAudit::from_log(&log);
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence.get(), 7);
        let renamed_actor = actor()
            .with_actor_name_snapshot(Some("新名称".into()))
            .unwrap()
            .with_request_id(Some("request-next".into()))
            .unwrap();
        assert_eq!(renamed_actor.actor_name_snapshot(), Some("新名称"));
        assert_eq!(renamed_actor.request_id(), Some("request-next"));
        let restored = prepared.to_log().unwrap();
        assert_eq!(restored, log);
        assert!(restored.message.as_deref().unwrap().contains("周晓彤"));
        assert!(restored.message.as_deref().unwrap().contains("商品"));
    }

    #[test]
    fn prepared_audit_preserves_unknown_name_and_discards_private_body() {
        let log = actor().resource_log("product.update", "product", "object-1".into()).unwrap();
        let mut prepared = PreparedCatalogAudit::from_log(&log);
        prepared.message = Some("password bank-account ciphertext private-request".into());
        let restored = prepared.to_log().unwrap();
        let event = restored.structured_event.as_ref().unwrap();
        assert_eq!(event.actor_name_snapshot, None);
        assert_eq!(event.request_id, None);
        assert_eq!(event.event_sequence, NonZeroU32::MIN);
        assert_eq!(event.actor_account, "login");
        let serialized = serde_json::to_string(&restored).unwrap();
        assert!(!serialized.contains("password"));
        assert!(!serialized.contains("bank-account"));
        assert!(!serialized.contains("ciphertext"));
        assert!(!serialized.contains("private-request"));
        assert!(!serialized.contains("actor_name_snapshot"));
        assert!(!serialized.contains("request_id"));
        assert!(restored.message.as_deref().unwrap().contains("login"));
    }

    #[test]
    fn prepared_audit_rejects_invalid_request_id() {
        let log = actor().resource_log("product.update", "product", "object-1".into()).unwrap();
        for request_id in ["unsafe\nrequest".into(), "x".repeat(129)] {
            let prepared = PreparedCatalogAudit::from_log(&log).with_request_id(Some(request_id));
            assert!(prepared.to_log().unwrap_err().to_string().contains("请求编号"));
        }
    }

    #[test]
    fn prepared_audit_rejects_invalid_name_snapshot() {
        let log = actor().resource_log("product.update", "product", "object-1".into()).unwrap();
        let prepared =
            PreparedCatalogAudit::from_log(&log).with_actor_name_snapshot(Some("unsafe\nname".into()));
        assert!(prepared.to_log().is_err());
    }
}
