//! 域 D02 `document_registry` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as DocumentRegistryExt>::BUSINESS_DOCUMENTS` 等值。

use entities::document_registry::{BusinessDocument, DocumentParticipant, DocumentRelation, WorkflowAction};
use mongodb::Database;

use super::super::document_registry::{
    BusinessDocumentFilter, DocumentRegistryRepository, WorkflowActionFilter,
};
use crate::Repository;

/// 域 D02 仓储访问器。
pub trait DocumentRegistryExt {
    /// `business_document` 集合名。
    const BUSINESS_DOCUMENTS: &'static str = "business_documents";
    /// `document_relation` 集合名。
    const DOCUMENT_RELATIONS: &'static str = "document_relations";
    /// `document_participant` 集合名。
    const DOCUMENT_PARTICIPANTS: &'static str = "document_participants";
    /// `workflow_action` 集合名。
    const WORKFLOW_ACTIONS: &'static str = "workflow_actions";

    /// 单据注册列表筛选条件类型（定义见 `repository::document_registry`）。
    type BusinessDocumentFilter;

    /// 工作流动作列表筛选条件类型（定义见 `repository::document_registry`）。
    type WorkflowActionFilter;

    /// 获取 `business_document` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::document_registry::BusinessDocument>`。
    fn business_documents(&self) -> Repository<'_, BusinessDocument>;

    /// 获取 `document_relation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::document_registry::DocumentRelation>`。
    fn document_relations(&self) -> Repository<'_, DocumentRelation>;

    /// 获取 `document_participant` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::document_registry::DocumentParticipant>`。
    fn document_participants(&self) -> Repository<'_, DocumentParticipant>;

    /// 获取 `workflow_action` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::document_registry::WorkflowAction>`。
    fn workflow_actions(&self) -> Repository<'_, WorkflowAction>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `DocumentRegistryRepository` 实例。
    fn document_registry(&self) -> DocumentRegistryRepository<'_>;
}

impl DocumentRegistryExt for Database {
    type BusinessDocumentFilter = BusinessDocumentFilter;
    type WorkflowActionFilter = WorkflowActionFilter;

    fn business_documents(&self) -> Repository<'_, BusinessDocument> {
        Repository::new(self, Self::BUSINESS_DOCUMENTS)
    }

    fn document_relations(&self) -> Repository<'_, DocumentRelation> {
        Repository::new(self, Self::DOCUMENT_RELATIONS)
    }

    fn document_participants(&self) -> Repository<'_, DocumentParticipant> {
        Repository::new(self, Self::DOCUMENT_PARTICIPANTS)
    }

    fn workflow_actions(&self) -> Repository<'_, WorkflowAction> {
        Repository::new(self, Self::WORKFLOW_ACTIONS)
    }

    fn document_registry(&self) -> DocumentRegistryRepository<'_> {
        DocumentRegistryRepository::new(self)
    }
}
