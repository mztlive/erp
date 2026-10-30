//! HTTP 进程入口；集群使用 external 模式，开发默认保留内嵌任务。
use web_api::runtime::{ProcessRole, Result, run};

#[tokio::main]
async fn main() -> Result<()> {
    run(ProcessRole::from_api_environment()?).await
}
