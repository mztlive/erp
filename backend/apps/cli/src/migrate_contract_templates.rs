//! 合同模板增量集合迁移，只复用合同领域公开索引入口。

use config::Config;
use erp_contract::indexes::ensure_templates;
use persistence_core::{connect, ensure_transaction_support};

use crate::error::Result;

/// 登记四个新增集合及其唯一约束，不改历史合同、不回退流水。
/// # 参数
/// * `path` - 目标配置路径。
/// # 返回
/// 迁移成功返回空值。
/// # 错误
/// 配置、事务能力或索引登记失败时返回非零。
pub async fn run(path: &str) -> Result<()> {
    let config = Config::from_file(path).await?;
    let (_, db) = connect(&config.database.uri, &config.database.db_name).await?;
    ensure_transaction_support(&db).await?;
    ensure_templates(&db).await?;
    println!("合同模板集合及索引已登记；2026 年历史流水由首次申请自动承接。");
    Ok(())
}
