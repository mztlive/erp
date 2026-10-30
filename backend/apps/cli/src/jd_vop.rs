//! 仅管理已授权凭据，不启动 OAuth 页面或远端刷新。
use application_core::AuditActor;
use cli::args::JdVopCredentialImportArgs;
use cli::jd_vop_input::read_input;
use config::ConfigArgs;
use erp_core::AccountKind;
use erp_processes::connectors::jd_vop::credential::{JdVopCredentialImport, JdVopCredentialStatus};

use crate::error::{Error, Result};
use crate::runtime::JdVopCredentialRuntime;

/// 预览或明确写入已有授权；令牌只读取本地输入，不进入参数和日志。
/// # 参数
/// `source` 为部署配置，`args` 为文件、版本和明确操作模式。
/// # 返回
/// 输出脱敏状态；默认模式不写入凭据、审计或索引。
/// # 错误
/// 读取、格式、绑定、版本、未知刷新边界或持久化失败时返回脱敏错误。
pub async fn import(source: &ConfigArgs, args: JdVopCredentialImportArgs) -> Result<()> {
    let file = args.file;
    let input = tokio::task::spawn_blocking(move || read_input::<JdVopCredentialImport>(&file))
        .await
        .map_err(|_| Error::CredentialInputTask)??;
    let runtime = JdVopCredentialRuntime::connect(source).await?;
    let evidence = if args.reauthorized { args.evidence_reference } else { None };
    let status = if args.apply {
        let actor = AuditActor::new(
            "system:jd-vop-credential-cli".into(),
            "system:jd-vop-credential-cli".into(),
            AccountKind::Admin,
        );
        runtime.process.import(input, args.expected_version, evidence, &actor).await?
    } else {
        runtime.process.preview(&input, args.expected_version, evidence.as_deref()).await?
    };
    print_status(&status, if args.apply { "已写入" } else { "预览，未写入" })
}

/// 查询配置绑定凭据的脱敏状态，不读取令牌明文或发出任何刷新请求。
/// # 参数
/// `source` 为统一配置来源，凭据身份仅由部署配置决定。
/// # 返回
/// 输出版本、有效期、刷新阶段与期限及是否需要重新授权。
/// # 错误
/// 配置、绑定或数据库查询失败时返回脱敏错误。
pub async fn status(source: &ConfigArgs) -> Result<()> {
    let runtime = JdVopCredentialRuntime::connect(source).await?;
    print_status(&runtime.process.status().await?, "只读状态")
}

/// 输出由凭据管理用例统一脱敏的状态，禁止输出输入 DTO。
/// # 参数
/// `status` 为安全状态 DTO，`mode` 为固定操作模式摘要。
/// # 返回
/// 完成标准输出写入。
/// # 错误
/// 序列化失败时只返回固定错误，不传播可能包含字段值的正文。
fn print_status(status: &JdVopCredentialStatus, mode: &str) -> Result<()> {
    let output = serde_json::to_string_pretty(status)
        .map_err(|_| Error::Usage("凭据状态序列化失败；内容已隐藏".into()))?;
    println!("模式: {mode}");
    println!("{output}");
    Ok(())
}
