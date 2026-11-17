//! 随独立 worker 启停，发布中的事务必须完成后再退出。
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use erp_resource_grants::ResourceGrantProcess;
use id_generator::next_id;
use tokio::sync::watch;

use crate::wiring::Application;

/// 运行授权消费及有界对账，停止后排空当前事务。
/// # 参数
/// state 为组合根，stop 为进程停止信号。
/// # 返回
/// 收到停止信号后结束循环。
/// # 错误
/// 轮次错误记录后重试，panic 由进程监督器检测。
pub(super) async fn run(state: Application, mut stop: watch::Receiver<bool>) {
    let process = ResourceGrantProcess::new(state.db(), state.authorization());
    let mut cursor = None;
    let mut rounds = 0_u64;
    loop {
        if *stop.borrow() {
            break;
        }
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        if rounds.is_multiple_of(30) {
            match process.reconcile(cursor.as_deref(), now).await {
                Ok(next) => cursor = next,
                Err(error) => tracing::error!(%error, "自动授权对账失败"),
            }
            match process.health(now).await {
                Ok(health) => tracing::info!(?health, "自动授权队列状态"),
                Err(error) => tracing::error!(%error, "自动授权观测失败"),
            }
        }
        if *stop.borrow() {
            break;
        }
        match process.tick(&next_id(), now).await {
            Ok(_) => {},
            Err(error) => tracing::error!(%error, "自动授权处理失败，等待恢复"),
        }
        rounds = rounds.wrapping_add(1);
        tokio::select! {
            _ = stop.changed() => break,
            _ = tokio::time::sleep(Duration::from_secs(2)) => {},
        }
    }
}
