//! 合同合法单据参与 Port：只查询本次目标与账号参与修订。

use std::sync::Arc;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 合同域消费的合法参与事实；adapter 读取工作流参与记录。
#[async_trait]
pub trait ContractParticipantPort: Send + Sync {
    /// 读取本次合同目标中的历史参与身份，不枚举账号的其他单据。
    ///
    /// # 参数
    /// * `user_id` - 当前账号
    /// * `ids` - 当前授权所检查的合同身份
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回目标集合中去重后的参与单据 ID；空目标返回空集合。
    ///
    /// # 错误
    /// 未装配或读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 历史参与必须来自有效业务参与事实，不得由签约经办或业绩快照推导。
    async fn document_ids_in(
        &self,
        user_id: &str,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 点查账号历史参与集合的真实修订号。
    /// # 参数
    /// user_id 为当前账号，executor 为原授权执行器。
    /// # 返回
    /// 追加参与记录时原子推进的修订号；尚无修订记录时为零。
    /// # 错误
    /// 未接线、修订损坏或读取失败时拒绝。
    async fn revision(&self, user_id: &str, executor: &mut dyn Executor) -> Result<i64>;
}

/// 未接线时失败关闭，不得把参与集合解释为空成功。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedContractParticipantPort;

impl FailClosedContractParticipantPort {
    /// 返回未接线的共享端口。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可注入合同 Service 的失败关闭端口。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 读取动作补充历史参与前必须注入真实 adapter。
    pub fn shared() -> Arc<dyn ContractParticipantPort> {
        Arc::new(Self)
    }
}

#[async_trait]
impl ContractParticipantPort for FailClosedContractParticipantPort {
    async fn document_ids_in(
        &self,
        _user_id: &str,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("合同参与端口未接线".into()))
    }

    async fn revision(&self, _: &str, _: &mut dyn Executor) -> Result<i64> {
        Err(Error::Internal("合同参与端口未接线".into()))
    }
}

/// 空参与集合，仅供不涉及历史参与的单元测试。
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptyContractParticipants;

#[async_trait]
impl ContractParticipantPort for EmptyContractParticipants {
    async fn document_ids_in(
        &self,
        _user_id: &str,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn revision(&self, _: &str, _: &mut dyn Executor) -> Result<i64> {
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::NoTransaction;
    use tokio::runtime::Builder;

    use super::*;

    #[test]
    fn fail_closed_participant_port_does_not_become_empty_success() {
        let runtime = Builder::new_current_thread().build().expect("runtime");
        runtime.block_on(async {
            let err = FailClosedContractParticipantPort
                .document_ids_in("user-1", &["contract-1".into()], &mut NoTransaction)
                .await
                .expect_err("must fail closed");
            match err {
                Error::Internal(message) => assert_eq!(message, "合同参与端口未接线"),
                other => panic!("期望 Internal，得到 {other:?}"),
            }
        });
    }
}
