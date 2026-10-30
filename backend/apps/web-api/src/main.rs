//! HTTP 进程入口，不启动后台任务。
use web_api::runtime::{ProcessRole, Result, run};

#[tokio::main]
async fn main() -> Result<()> {
    run(ProcessRole::Api).await
}
