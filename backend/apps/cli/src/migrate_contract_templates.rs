//! 合同模板增量集合迁移，只复用合同领域公开索引入口。

use config::ConfigArgs;
use erp_contract::indexes::ensure_templates;
use persistence_core::{connect, ensure_transaction_support};

use crate::error::Result;

/// 开放合同申请前，先确认目标 MongoDB 支持事务，再调用合同域入口登记模板、主体编号绑定、
/// 年度流水与申请记录的索引（含唯一约束）。本命令不修改历史合同或已有流水。
pub async fn run(source: &ConfigArgs) -> Result<()> {
    let config = source.load().await?;
    let (_, db) = connect(&config.database.uri, &config.database.db_name).await?;
    ensure_transaction_support(&db).await?;
    ensure_templates(&db).await?;
    println!("合同模板集合及索引已登记；2026 年历史流水由首次申请自动承接。");
    Ok(())
}
