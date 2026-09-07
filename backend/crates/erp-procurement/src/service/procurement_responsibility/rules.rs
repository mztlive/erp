//! 规则选择器校验和采购集合写入，始终加入调用方执行器。

use super::ProcurementResponsibilityService;
use crate::entity::procurement_responsibility::{
    ProcurementResponsibilityRule, ProcurementResponsibilityRuleData,
    ProcurementResponsibilitySelectorReference,
};
use crate::ports::procurement_responsibility::ProcurementResponsibilityFactsPort;
use crate::repository::ProcurementResponsibilityExt;
use crate::{Error, Result};
use persistence_core::Executor;

impl ProcurementResponsibilityService {
    /// 创建已完成校验的规则；唯一冲突保持原采购责任错误文案。
    pub async fn create_rule(
        &self,
        rule: &ProcurementResponsibilityRule,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db
            .procurement_responsibility_rules()
            .create(rule, executor)
            .await?;
        Ok(())
    }

    /// 在原事务内读取、核对版本、更新规则，不写审计或开启事务。
    pub async fn update_rule(
        &self,
        id: &str,
        version: u64,
        data: ProcurementResponsibilityRuleData,
        updated_by: &str,
        executor: &mut dyn Executor,
    ) -> Result<ProcurementResponsibilityRule> {
        let mut rule = self
            .db
            .procurement_responsibility_rules()
            .find_procurement_responsibility_rule(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购责任规则不存在".to_string()))?;
        if rule.base.version != version {
            return Err(Error::ConflictError("采购责任规则版本已变化".to_string()));
        }
        rule.update(data, updated_by).map_err(Error::Logic)?;
        self.db
            .procurement_responsibility_rules()
            .update(&mut rule, executor)
            .await?;
        Ok(rule)
    }

    /// 校验规则选择器引用的目录实体存在。
    ///
    /// # 参数
    /// * `data` - 待维护规则数据
    /// * `executor` - 数据库执行器，可为规则写事务会话
    ///
    /// # 返回
    /// 引用存在或规则不需要目录引用时返回 `Ok(())`。
    ///
    /// # 错误
    /// SKU 或分类不存在时返回校验错误。
    pub async fn validate_selector_reference(
        &self,
        data: &ProcurementResponsibilityRuleData,
        port: &dyn ProcurementResponsibilityFactsPort,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let exists = match data.selector_reference().map_err(Error::Logic)? {
            ProcurementResponsibilitySelectorReference::Sku(id) => port.sku_exists(id, executor).await?,
            ProcurementResponsibilitySelectorReference::Category(id) => {
                port.category_exists(id, executor).await?
            }
            ProcurementResponsibilitySelectorReference::None => return Ok(()),
        };
        if exists {
            return Ok(());
        }
        Err(Error::ValidationError(
            "采购责任规则引用的目录实体不存在或已删除".to_string(),
        ))
    }
}
