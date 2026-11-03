//! 支持域经工作流读取已注册业务单据编号。

use std::sync::Arc;

use async_trait::async_trait;
use erp_support::BusinessDocumentPort;
use erp_workflow::DocumentRegistryExt;
use mongodb::Database;
use persistence_core::Executor;

/// 只暴露已注册单据编号、不泄漏工作流类型的 Mongo adapter。
#[derive(Clone)]
pub struct MongoBusinessDocument {
    db: Database,
}

impl MongoBusinessDocument {
    /// 绑定单据注册表所在数据库，构造时不读取。
    ///
    /// # 参数
    /// * `db` - 读取单据注册表的数据库。
    ///
    /// # 返回
    /// 返回未执行 I/O 的 adapter，而不是共享 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 包装为支持域可注入的业务单据 Port。
    ///
    /// # 参数
    /// * `db` - 读取单据注册表的数据库。
    ///
    /// # 返回
    /// 返回实现 `BusinessDocumentPort` 的共享 Port。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn shared(db: Database) -> Arc<dyn BusinessDocumentPort> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl BusinessDocumentPort for MongoBusinessDocument {
    async fn ensure_registered(
        &self,
        document_id: &str,
        executor: &mut dyn Executor,
    ) -> erp_support::Result<()> {
        self.db
            .business_documents()
            .find_by_id(document_id, executor)
            .await
            .map_err(erp_support::Error::from)?
            .ok_or_else(|| erp_support::Error::NotFound("业务单据未注册".to_string()))?;
        Ok(())
    }

    async fn find_registered_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_support::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let documents = self
            .db
            .business_documents()
            .list_active_by_ids(ids, executor)
            .await
            .map_err(erp_support::Error::from)?;
        Ok(documents.into_iter().map(|document| document.base.id).collect())
    }
}
