//! 门户统一申请操作通过精确领域类型分派，协议层不读取业务仓储。

use application_core::AuditActor;
use erp_catalog::portal::CatalogPortalExt;
use erp_identity::PortalActor;
use erp_supplier::portal::CooperationRepository;
use erp_supply::portal::PortalSupplyExt;
use persistence_core::NoTransaction;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::{PortalReview, PortalTransition, SupplierPortalProcess};
use crate::{Error, Result};

enum RequestKind {
    Offering,
    NewProduct,
    Cooperation,
}

impl SupplierPortalProcess {
    /// 按拥有领域修改当前供应商可编辑申请。
    /// # 参数
    /// 精确申请、对应草稿DTO与真实门户身份。
    /// # 返回
    /// 返回持久化领域结果，协议层须作外部允许列表投影。
    /// # 错误
    /// 不存在、字段非法、范围或版本失效时拒绝。
    pub async fn application_update(&self, id: &str, input: Value, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, Some(actor)).await? {
            RequestKind::Offering => value(self.application_save(Some(id), parse(input)?, actor).await?),
            RequestKind::NewProduct => value(self.new_product_save(Some(id), parse(input)?, actor).await?),
            RequestKind::Cooperation => value(self.cooperation_save(Some(id), parse(input)?, actor).await?),
        }
    }

    /// 冻结统一入口申请并建立对应采购确认任务。
    /// # 参数
    /// 精确申请、原版本命令与门户身份。
    /// # 返回
    /// 返回实际提交结果。
    /// # 错误
    /// 领域类型、归属、输入或处理资格不符时拒绝。
    pub async fn submit(&self, id: &str, input: &PortalTransition, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, Some(actor)).await? {
            RequestKind::Offering => value(self.application_submit(id, input.clone(), actor).await?),
            RequestKind::NewProduct => value(self.new_product_submit(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_submit(id, input.clone(), actor).await?),
        }
    }

    /// 撤回统一入口的当前待确认申请及任务。
    /// # 参数
    /// 精确申请、版本命令及门户身份。
    /// # 返回
    /// 返回保留冻结历史的撤回结果。
    /// # 错误
    /// 越界、已完成或并发变更时拒绝。
    pub async fn withdraw(&self, id: &str, input: &PortalTransition, actor: &PortalActor) -> Result<Value> {
        match self.request_kind_for(id, Some(actor)).await? {
            RequestKind::Offering => value(self.application_withdraw(id, input.clone(), actor).await?),
            RequestKind::NewProduct => value(self.new_product_withdraw(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_withdraw(id, input.clone(), actor).await?),
        }
    }

    /// 由当前具体内部采购人作领域对应的原子决定。
    /// # 参数
    /// 申请、任务版本、明确映射和内部身份。
    /// # 返回
    /// 返回正式结果或退回意见。
    /// # 错误
    /// 任一对象或处理资格失效时拒绝。
    pub async fn review(&self, id: &str, input: &PortalReview, actor: &AuditActor) -> Result<Value> {
        match self.request_kind_for(id, None).await? {
            RequestKind::Offering => value(self.application_review(id, input.clone(), actor).await?),
            RequestKind::NewProduct => value(self.new_product_review(id, input.clone(), actor).await?),
            RequestKind::Cooperation => value(self.cooperation_review(id, input.clone(), actor).await?),
        }
    }

    async fn request_kind_for(&self, id: &str, actor: Option<&PortalActor>) -> Result<RequestKind> {
        if let Some(actor) = actor {
            self.session_validate(actor, &mut NoTransaction).await?;
        }
        let found =
            if let Some(app) = self.db.portal_applications().find_by_id(id, &mut NoTransaction).await? {
                Some((RequestKind::Offering, app.supplier_id))
            } else if let Some(app) = self.db.new_product_drafts().find_by_id(id, &mut NoTransaction).await? {
                Some((RequestKind::NewProduct, app.supplier_id))
            } else {
                CooperationRepository::new(&self.db)
                    .find_any(id, &mut NoTransaction)
                    .await?
                    .map(|app| (RequestKind::Cooperation, app.supplier_id))
            };
        match found {
            Some((kind, supplier)) if actor.is_none_or(|a| a.supplier_id == supplier) => Ok(kind),
            _ => Err(Error::NotFound("申请不存在或无权查看".into())),
        }
    }
}

fn parse<T: DeserializeOwned>(input: Value) -> Result<T> {
    serde_json::from_value(input).map_err(|error| Error::ValidationError(format!("申请字段无效: {error}")))
}
fn value<T: Serialize>(result: T) -> Result<Value> {
    serde_json::to_value(result).map_err(|error| Error::Internal(error.to_string()))
}
