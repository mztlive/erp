//! 独立后台任务进程，仅提供内部健康探针。
use web_api::runtime::{ProcessRole, Result, run};

#[tokio::main]
async fn main() -> Result<()> {
    run(ProcessRole::Worker).await
}
