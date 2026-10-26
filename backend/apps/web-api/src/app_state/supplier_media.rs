//! 供应商来源素材与选品的生产装配；配置错误不得退化为无图成功。

use std::sync::Arc;

use erp_processes::Result;
use erp_processes::source_media::SourceMediaPreparer;
use erp_processes::supplier_catalog_adoption::SupplierCatalogAdoptionProcess;

use super::AppState;

impl AppState {
    /// 从当前部署配置创建受控来源图片准备器。
    /// # 参数
    /// 无；复用应用对象存储和受控配置快照。
    /// # 返回
    /// 已启用协议对应的准备器；未启用协议时返回空。
    /// # 错误
    /// 素材主机或内容校验配置无效时拒绝，不吞掉错误。
    pub fn supplier_source_media_preparer(&self) -> Result<Option<Arc<SourceMediaPreparer>>> {
        let config = self.config_snapshot();
        let Some(settings) = config.dangaoshushu.as_ref().filter(|settings| settings.enabled) else {
            return Ok(None);
        };
        SourceMediaPreparer::new(
            self.storage().clone(),
            config.app.secret.as_bytes().to_vec(),
            settings.media_allowed_hosts.clone(),
        )
        .map(|preparer| Some(Arc::new(preparer)))
    }

    /// 装配真实商品、供给和来源素材共用的选品流程。
    /// # 参数
    /// 无；复用应用数据库、权限与素材准备器。
    /// # 返回
    /// 可执行预览、核对与逐项确认的选品流程。
    /// # 错误
    /// 已启用的素材配置非法时拒绝。
    pub fn supplier_catalog_adoption_process(&self) -> Result<SupplierCatalogAdoptionProcess> {
        let process = SupplierCatalogAdoptionProcess::new(self.db(), self.rbac());
        Ok(match self.supplier_source_media_preparer()? {
            Some(preparer) => process.with_media_preparer(preparer),
            None => process,
        })
    }
}
