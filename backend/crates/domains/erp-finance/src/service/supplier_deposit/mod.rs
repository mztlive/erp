//! 预存款事务步骤；跨域 Process 提供唯一事务，外部 HTTP 在事务外。

mod auto_settlement;
mod coverage;
mod funds_evidence;
mod funds_lease;
mod recharge;
mod release;
mod reserve;
mod resolution;
mod review_recovery;
mod settlement;
mod setup;
mod usage;
use mongodb::Database;
use persistence_core::Executor;
pub use resolution::VerifiedDepositRelease;
use serde::Serialize;
pub use setup::DepositRechargePaymentTarget;
use sha2::{Digest, Sha256};

use crate::{Error, Result};

/// 供应商预存款领域服务，必须在调用方事务内执行多集合资金写入。
pub struct SupplierDepositService {
    db: Database,
}

impl SupplierDepositService {
    /// 在组装供应商预存款用例时绑定本域数据库句柄，实际查询或写入由各用例方法执行。
    /// 返回事务步骤服务。
    ///
    /// 输入约定：
    /// - `db`：当前财务拥有的数据库。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

fn require_transaction(executor: &mut dyn Executor) -> Result<()> {
    if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
        return Err(Error::BusinessLogicError("预存款资金写入必须加入调用方事务".into()));
    }
    Ok(())
}

fn fingerprint(payload: &impl Serialize) -> Result<String> {
    let bytes = serde_json::to_vec(payload).map_err(|_| Error::Internal("预存款载荷无法编码".into()))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn identity(account: &str, operation: &str) -> Result<String> {
    if operation.trim().is_empty() || operation.len() > 256 {
        return Err(Error::ValidationError("预存款操作身份无效".into()));
    }
    fingerprint(&(account, operation))
}

fn replay(expected: &str, actual: &str) -> Result<()> {
    if expected != actual {
        return Err(Error::ConflictError("同一预存款操作身份不能用于不同内容".into()));
    }
    Ok(())
}
