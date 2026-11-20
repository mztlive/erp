//! 暴露已装配跨领域事实的授权管理服务。
use erp_identity::service::resource_access::ResourceGrantService;

use crate::ResourceGrantProcess;

impl ResourceGrantProcess {
    /// 返回已绑定业务事实的授权管理服务，供授予、撤销、查询来源或发起重算；各操作自行校验管理员权限。
    pub fn management(&self) -> ResourceGrantService {
        self.grants.clone()
    }
}
