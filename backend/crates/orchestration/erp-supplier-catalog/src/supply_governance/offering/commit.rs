//! 跨域写入根的实际provider：领域命令持久化后写同一审计。
use async_trait::async_trait;
use erp_audit::AuditLog;
use erp_audit::execution::persist_log;
use erp_supply::dto::supplier_offering::{
    ReviseSupplierOfferingResult, UpdateSupplierOfferingAvailabilityResult,
};
use erp_supply::service::supplier_offering::{
    PreparedAvailability, PreparedCreate, PreparedRevision, SupplierOfferingService,
};
use mongodb::Database;
use persistence_core::Executor;
#[cfg(test)]
use persistence_core::ReadExecutor;

use crate::Result;
#[async_trait]
trait CommitPort: Send {
    type Output: Send;
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Output>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}
// 供给命令已准备好领域写入时，在调用方事务先保存供给事实，再写成功审计；本步骤不单独开事务。
async fn commit<P: CommitPort>(port: &mut P, executor: &mut dyn Executor) -> Result<P::Output> {
    let result = port.domain(executor).await?;
    port.audit(executor).await?;
    Ok(result)
}
struct MongoCreated<'a> {
    db: &'a Database,
    prepared: &'a PreparedCreate,
    audit: &'a AuditLog,
}
#[async_trait]
impl CommitPort for MongoCreated<'_> {
    type Output = ();
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Output> {
        SupplierOfferingService::new(self.db.clone())
            .persist_created(self.prepared, executor)
            .await
            .map_err(Into::into)
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_log(self.db, self.audit, executor).await.map_err(Into::into)
    }
}
// 提交新建供给的领域事实与原审计，复用传入的同一事务执行器。
//
// `prepared`：已准备的创建载荷；`audit`：同一执行器中写入的审计日志；`executor`：调用方事务执行器。
//
// 领域事实和审计都写入后完成。
//
// 领域持久化或审计写入失败时返回该错误并停止后续写入。
pub(super) async fn created(
    db: &Database,
    prepared: &PreparedCreate,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    commit(&mut MongoCreated { db, prepared, audit }, executor).await
}
struct MongoRevised<'a> {
    db: &'a Database,
    prepared: &'a mut PreparedRevision,
    audit: &'a AuditLog,
}
#[async_trait]
impl CommitPort for MongoRevised<'_> {
    type Output = ReviseSupplierOfferingResult;
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Output> {
        SupplierOfferingService::new(self.db.clone())
            .persist_revised(self.prepared, executor)
            .await
            .map_err(Into::into)
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_log(self.db, self.audit, executor).await.map_err(Into::into)
    }
}
// 提交供给修订的领域事实与原审计，复用传入的同一事务执行器。
//
// `prepared`：已准备的修订载荷，持久化时就地更新；`audit`：同一执行器中写入的审计日志；`executor`：调用方事务执行器。
//
// 领域修订结果。审计在该结果产生之后写入。
//
// 领域持久化或审计写入失败时返回该错误并停止后续写入。
pub(super) async fn revised(
    db: &Database,
    prepared: &mut PreparedRevision,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<ReviseSupplierOfferingResult> {
    commit(&mut MongoRevised { db, prepared, audit }, executor).await
}
struct MongoAvailability<'a> {
    db: &'a Database,
    prepared: &'a mut PreparedAvailability,
    audit: &'a AuditLog,
}
#[async_trait]
impl CommitPort for MongoAvailability<'_> {
    type Output = UpdateSupplierOfferingAvailabilityResult;
    async fn domain(&mut self, executor: &mut dyn Executor) -> Result<Self::Output> {
        SupplierOfferingService::new(self.db.clone())
            .persist_availability(self.prepared, executor)
            .await
            .map_err(Into::into)
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        persist_log(self.db, self.audit, executor).await.map_err(Into::into)
    }
}
// 提交可供状态的领域事实与原审计，复用传入的同一事务执行器。
//
// `prepared`：已准备的可供更新载荷，持久化时就地更新；`audit`：同一执行器中写入的审计日志；`executor`：调用方事务执行器。
//
// 可供更新结果。审计在该结果产生之后写入。
//
// 领域持久化或审计写入失败时返回该错误并停止后续写入。
pub(super) async fn availability(
    db: &Database,
    prepared: &mut PreparedAvailability,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<UpdateSupplierOfferingAvailabilityResult> {
    commit(&mut MongoAvailability { db, prepared, audit }, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    type Marker = ReadExecutor;

    struct Recorder {
        pointer: usize,
        calls: Vec<&'static str>,
        fail: Option<usize>,
    }
    impl Recorder {
        fn record(&mut self, name: &'static str, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let i = self.calls.len();
            self.calls.push(name);
            if self.fail == Some(i) {
                return Err(Error::ConflictError(format!("commit {i}")));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl CommitPort for Recorder {
        type Output = u64;
        async fn domain(&mut self, executor: &mut dyn Executor) -> Result<u64> {
            self.record("domain", executor)?;
            Ok(73)
        }
        async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", executor)
        }
    }
    #[tokio::test]
    async fn domain_and_audit_preserve_executor_result_and_each_error() {
        for fail in [None, Some(0), Some(1)] {
            let mut executor = Marker::default();
            let mut port = Recorder { pointer: &mut executor as *mut Marker as usize, calls: vec![], fail };
            let result = commit(&mut port, &mut executor).await;
            if let Some(i) = fail {
                assert!(matches!(result,Err(Error::ConflictError(ref e)) if e==&format!("commit {i}")));
                assert_eq!(port.calls, ["domain", "audit"][..=i]);
            } else {
                assert_eq!(result.unwrap(), 73);
                assert_eq!(port.calls, ["domain", "audit"]);
            }
        }
    }
}
