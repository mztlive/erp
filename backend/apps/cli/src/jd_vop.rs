//! 仅管理已授权凭据，不启动 OAuth 页面或远端刷新。
use application_core::AuditActor;
use cli::args::JdVopCredentialImportArgs;
use cli::jd_vop_input::read_input;
use config::ConfigArgs;
use erp_core::AccountKind;
use erp_supplier_orders::supplier_credentials::{JdVopCredentialImport, JdVopCredentialStatus};

use crate::error::{Error, Result};
use crate::runtime::JdVopCredentialRuntime;

/// 处理 `jd-vop-credential-import`，从本地文件或标准输入接收已取得的京东授权，
/// 按部署绑定校验身份；替换已有凭据时必须给出当前版本。令牌不经命令参数或日志传递。
///
/// 默认只预览，不写凭据、审计或索引。确认后用 `--apply` 写入密文及审计；
/// 刷新结果未知时不能普通覆盖，须等用例允许恢复并提供新授权依据。
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

/// 处理 `jd-vop-credential-status`，按部署配置查询对应凭据的版本、有效期、
/// 刷新阶段和恢复期限，并提示是否需要重新授权。用于判断下一步运维操作，
/// 不解密令牌，也不向京东发起刷新。
pub async fn status(source: &ConfigArgs) -> Result<()> {
    let runtime = JdVopCredentialRuntime::connect(source).await?;
    print_status(&runtime.process.status().await?, "只读状态")
}

// 在预览、写入和查询结束后，只向标准输出打印操作模式与用例生成的脱敏状态。
// 序列化失败时返回固定摘要，不回显字段值或导入内容。
fn print_status(status: &JdVopCredentialStatus, mode: &str) -> Result<()> {
    let output = serde_json::to_string_pretty(status)
        .map_err(|_| Error::Usage("凭据状态序列化失败；内容已隐藏".into()))?;
    println!("模式: {mode}");
    println!("{output}");
    Ok(())
}
