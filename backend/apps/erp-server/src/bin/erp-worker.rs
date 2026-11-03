//! 独立后台任务进程，提供内部探针与调度协议。
use erp_server::bootstrap::run;
use erp_server::runtime::{ProcessRole, Result};

#[tokio::main]
async fn main() -> Result<()> {
    run(ProcessRole::Worker).await
}
