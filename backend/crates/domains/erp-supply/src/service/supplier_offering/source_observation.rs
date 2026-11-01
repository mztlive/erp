//! API 选品保留原始读取时间，不把确认选品时间当作重新核验。

use erp_core::common::time::Instant;

use super::PreparedCreate;
use crate::entity::supplier_offering::OfferingSourceType;
use crate::{Error, Result};

impl PreparedCreate {
    /// 为 API 新供给沿用真实来源观察与已配置的有效期。
    /// # 参数
    /// `observed_at` 为目录成功读取时间，`valid_until` 为该观察的有效期限。
    /// # 返回
    /// 更新待提交事实的接收、核验与期限，不改变来源业务更新时间或报价。
    /// # 错误
    /// 非 API 来源不得使用该入口。
    pub fn retain_api_observation(
        &mut self,
        observed_at: Instant,
        valid_until: Option<Instant>,
    ) -> Result<()> {
        if self.offering.source_type != OfferingSourceType::Api {
            return Err(Error::ValidationError("只有 API 供给可以沿用来源同步核验时间".into()));
        }
        self.availability.received_at = observed_at;
        self.availability.verified_at = Some(observed_at);
        self.availability.valid_until = valid_until;
        Ok(())
    }
}
