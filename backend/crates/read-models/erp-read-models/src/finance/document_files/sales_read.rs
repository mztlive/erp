//! 销售附件沿原销售详情查询读取，并在完整文件补齐后检查交付资格。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_permissions::sales_order;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::repository::sales_order::scope::SalesReadScope;
use persistence_core::Executor;

use super::{BusinessFinanceFiles, FinancialFileView};
use crate::sales_center::access::{SalesAccess, SalesReadContext};
use crate::{Error, Result};

/// 固定销售详情读取的三个阶段；不提供调用方替换政策的能力。
#[async_trait]
pub(super) trait SalesFilesRead: Sync {
    type Access: Send + Sync;

    /// 在原执行器生成固定销售详情查询。
    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Access>;
    /// 按原凭证过滤根销售单，并读取其真实财务文件。
    async fn load(
        &self,
        id: &str,
        access: &Self::Access,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialFileView>>;
    /// 在全部文件补齐后校验同一凭证。
    async fn validate(&self, access: &Self::Access, executor: &mut dyn Executor) -> Result<()>;
}

#[async_trait]
impl SalesFilesRead for BusinessFinanceFiles {
    type Access = (SalesReadContext, SalesReadScope);

    async fn query(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Access> {
        SalesAccess::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, &sales_order::DETAIL, executor)
            .await
    }

    async fn load(
        &self,
        id: &str,
        access: &Self::Access,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinancialFileView>> {
        self.db
            .sales_orders()
            .find_authorized(id, &access.1, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在或无权查看".into()))?;
        self.sales_files(id, executor).await
    }

    async fn validate(&self, access: &Self::Access, executor: &mut dyn Executor) -> Result<()> {
        access.0.validate(executor).await
    }
}

/// 授权、来源与文件补齐、交付检查必须完整成功，才允许返回文件清单。
pub(super) async fn read(
    port: &impl SalesFilesRead,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<FinancialFileView>> {
    let access = port.query(actor, executor).await?;
    let files = port.load(id, &access, executor).await?;
    port.validate(&access, executor).await?;
    Ok(files)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::AccountKind;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    struct Files {
        denied: bool,
        expires: bool,
        calls: Mutex<Vec<(&'static str, usize)>>,
    }

    impl Files {
        fn record(&self, operation: &'static str, executor: &mut dyn Executor) {
            assert!(executor.boundary().unwrap().is_snapshot());
            self.calls.lock().unwrap().push((operation, executor as *mut dyn Executor as *mut () as usize));
        }
    }

    #[async_trait]
    impl SalesFilesRead for Files {
        type Access = &'static str;

        async fn query(&self, _: &AuditActor, executor: &mut dyn Executor) -> Result<Self::Access> {
            self.record("query", executor);
            if self.denied {
                return Err(Error::Forbidden("无销售单详情资格".into()));
            }
            Ok("original-query")
        }

        async fn load(
            &self,
            id: &str,
            access: &Self::Access,
            executor: &mut dyn Executor,
        ) -> Result<Vec<FinancialFileView>> {
            assert_eq!(id, "sales");
            assert_eq!(*access, "original-query");
            self.record("files", executor);
            Ok(vec![FinancialFileView {
                document_id: "invoice".into(),
                document_no: "INV-1".into(),
                file_asset_id: "file".into(),
                file_name: "发票.pdf".into(),
                content_type: "application/pdf".into(),
                byte_size: 10,
            }])
        }

        async fn validate(&self, access: &Self::Access, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(*access, "original-query");
            self.record("validate", executor);
            if self.expires {
                return Err(Error::ConflictError("DATA_SCOPE_CHANGED：授权关系已到期".into()));
            }
            Ok(())
        }
    }

    /// 清单完整补齐后仍在原执行器完成交付检查。
    #[tokio::test]
    async fn file_list_is_delivered_after_original_query_validation() {
        let port = Files { denied: false, expires: false, calls: Mutex::new(vec![]) };
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        let mut executor = SnapshotExecutor::default();
        let pointer = &mut executor as *mut SnapshotExecutor as usize;
        let files = read(&port, &actor, "sales", &mut executor).await.unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].file_asset_id, "file");
        assert_eq!(
            *port.calls.lock().unwrap(),
            [("query", pointer), ("files", pointer), ("validate", pointer)]
        );
    }

    /// 无资格不读取文件；读取中到期不能交付已补齐的清单。
    #[tokio::test]
    async fn denial_stops_file_reads_and_expiry_discards_the_loaded_files() {
        let actor = AuditActor::new("sales".into(), "sales".into(), AccountKind::Admin);
        for (denied, expires) in [(true, false), (false, true)] {
            let port = Files { denied, expires, calls: Mutex::new(vec![]) };
            let result = read(&port, &actor, "sales", &mut SnapshotExecutor::default()).await;
            if denied {
                assert!(matches!(result, Err(Error::Forbidden(_))));
            } else {
                assert!(
                    matches!(result, Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED"))
                );
            }
            let steps = port.calls.lock().unwrap().iter().map(|(step, _)| *step).collect::<Vec<_>>();
            assert_eq!(steps, if denied { vec!["query"] } else { vec!["query", "files", "validate"] });
        }
    }
}
