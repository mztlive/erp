//! 显式人员范围迁移命令，默认只输出报告。
use config::ConfigArgs;
use erp_identity::indexes::ensure_person_scopes;
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::scope_configuration;

use crate::args::MigrateScopeArgs;
use crate::error::{Error, Result};

/// 输出报告，仅 --apply 写入。
/// # 参数
/// 配置来源和明确人员。
/// # 返回
/// 无阻断时成功。
/// # 错误
/// 配置、数据库或迁移阻断返回非零。
pub async fn run(source: &ConfigArgs, args: MigrateScopeArgs) -> Result<()> {
    if args.apply && args.expected_policy_version.is_none() {
        return Err(Error::Usage("--apply 必须提供预览报告的 --expected-policy-version".into()));
    }
    if !args.apply && args.expected_policy_version.is_some() {
        return Err(Error::Usage("预览不得携带应用版本，写入必须显式 --apply".into()));
    }
    let config = source.load().await?;
    let (_, db) = persistence_core::connect(&config.database.uri, &config.database.db_name).await?;
    if args.apply {
        ensure_person_scopes(&db).await?;
    }
    let rbac = shared_rbac_service(db.clone());
    let report = scope_configuration(db, rbac)
        .migrate_person_scopes(args.user_id, args.expected_policy_version)
        .await?;
    println!("{}", serde_json::to_string_pretty(&report).map_err(|e| Error::Usage(e.to_string()))?);
    if !report.blockers.is_empty() {
        return Err(Error::Usage("存在阻断项，禁止切换；请显式设置人员范围后重新预览".into()));
    }
    Ok(())
}
