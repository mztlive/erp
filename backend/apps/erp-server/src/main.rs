//! HTTP 进程入口，不启动常驻消费者。
use erp_server::bootstrap::run;
use erp_server::runtime::{ProcessRole, Result};

#[tokio::main]
async fn main() -> Result<()> {
    run(ProcessRole::Api).await
}
