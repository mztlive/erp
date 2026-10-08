//! 客户命令所需的主体身份事实消费端口。

use async_trait::async_trait;
use erp_core::ids::PartyId;

use crate::error::{Error, Result};

/// Minimal Party identity snapshot used to hydrate customer views.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartyIdentityFact {
    /// Party stable id.
    pub party_id: String,
    /// Party business number.
    pub party_no: String,
    /// Current legal name when a current revision exists.
    pub legal_name: Option<String>,
    /// Current short name when a current revision exists.
    pub short_name: Option<String>,
}

/// Port customer uses to read Party existence and identity facts.
#[async_trait]
pub trait PartyFactPort: Send + Sync {
    /// 主体不存在时拒绝。
    ///
    /// # 参数
    /// * `party_id` - 主体稳定 ID。
    ///
    /// # 返回
    /// 主体存在时无返回值。
    ///
    /// # 错误
    /// 主体不存在时返回 `NotFound`。适配器读取失败时返回对应错误。
    async fn ensure_exists(&self, party_id: &PartyId) -> Result<()>;

    /// 返回给定主体 ID 的身份事实。
    ///
    /// 缺失的主体省略；调用方把缺口当作静默降级。
    ///
    /// # 参数
    /// * `party_ids` - 待读取的主体 ID。
    ///
    /// # 返回
    /// 返回能读到的身份事实；缺失主体不出现在结果中。
    ///
    /// # 错误
    /// 主体事实读取失败时返回对应错误。
    async fn identities_by_ids(&self, party_ids: &[PartyId]) -> Result<Vec<PartyIdentityFact>>;

    /// 返回当前法定名称、简称或统一社会信用代码匹配 `keyword` 的主体 ID。
    ///
    /// # 参数
    /// * `keyword` - 名称或统一社会信用代码关键词。
    ///
    /// # 返回
    /// 返回命中的主体 ID；无命中时为空集合。
    ///
    /// # 错误
    /// 主体检索失败时返回对应错误。
    async fn matching_ids_by_name(&self, keyword: &str) -> Result<Vec<String>>;
}

/// Fail-closed Party fact port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedPartyFactPort;

#[async_trait]
impl PartyFactPort for FailClosedPartyFactPort {
    async fn ensure_exists(&self, _party_id: &PartyId) -> Result<()> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }

    async fn identities_by_ids(&self, _party_ids: &[PartyId]) -> Result<Vec<PartyIdentityFact>> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }

    async fn matching_ids_by_name(&self, _keyword: &str) -> Result<Vec<String>> {
        Err(Error::Internal("主体端口未接线".to_string()))
    }
}
