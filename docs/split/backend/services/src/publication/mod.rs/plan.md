# `backend/services/src/publication/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/publication/mod.rs` |
| 扫描行数 | 1060 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：该文件包含连接器抽象、Service 容器、稳定发布 CRUD、发布修订与媒体、商城投递结算五个边界清晰的内聚簇。建议保留 mod.rs 作为模块根和统一 re-export 出口，将实现拆成 5 个域内子文件；拆分后最大文件预计约 400–450 行，所有文件均可控制在约 800 行以内。现有纯业务校验 ensure_media_invariant 应按仓库类型内聚约定下沉到已有 dto.rs，而不是留作跨模块 Service helper。
- 拆分建议：
  - **backend/services/src/publication/service.rs**：放置 PublicationService 结构体及 impl PublicationService::new。结构体继续持有 db: Database 和 connector: Arc<dyn MallConnector>；mod.rs 通过 pub use self::service::PublicationService 保持原公共路径。
    - 依赖/注意：service.rs 依赖 super::connector::MallConnector。由于 publication.rs、revision.rs、delivery.rs 将作为同级模块为该类型增加 impl，db 与 connector 字段需声明为 pub(super)，不要改为 pub 或 pub(crate)。connector.rs 不应反向依赖 PublicationService，以免产生循环依赖。
  - **backend/services/src/publication/connector.rs**：放置外部商城连接器契约与默认失败关闭实现：ClassifiedError、PublishAck、MallConnector、UnavailableMallConnector、impl MallConnector for UnavailableMallConnector；同时放置 sample_revision、default_connector_fails_closed_with_classified_error、mock_connector_success_returns_ack 三项连接器测试。
    - 依赖/注意：直接依赖 entities::integration_ops::ErrorClass、entities::ids::SourceSystemId、entities::publication::ProductPublicationRevision 和 std::pin::Pin。mod.rs 必须重新公开导出 ClassifiedError、PublishAck、MallConnector、UnavailableMallConnector，以保持 Handler 使用 services::publication::UnavailableMallConnector 等路径不变。该模块不要引用 service.rs。
  - **backend/services/src/publication/publication.rs**：放置稳定商品发布身份的创建、查询和更新编排，即 impl PublicationService 中的 create_publication、publication_list、publication_detail、update_publication；同时将 ProductPublicationFilter 类型别名移入本文件。
    - 依赖/注意：通过 super::service::PublicationService 引用服务类型，通过 super::dto 引用请求、分页视图和 SortDir。需要 database::CatalogExt、PublicationExt、AccessControlExt、Transactional、NoTransaction。update_publication 的发布更新与审计写入必须继续位于同一事务。原测试 publication_view_flattens_stable_base 更适合迁入已有 backend/services/src/publication/dto.rs 的 tests，因为它验证的是 From<ProductPublication> for ProductPublicationView，而不是本文件的 Service 编排。
  - **backend/services/src/publication/revision.rs**：放置发布修订及媒体编排：impl PublicationService 中的 create_revision、revision_list、revision_media_list、私有 next_revision_no；包含修订实体和媒体实体构造、content_hash 计算、发布状态推进及跨集合事务写入。
    - 依赖/注意：通过 super::service::PublicationService 引用服务，通过 super::dto::publication_content_hash 直接访问现有 DTO helper，避免依赖 mod.rs 的中转 re-export。依赖 CatalogExt、SupplierOfferingExt、PublicationExt、AccessControlExt、Transactional 和 NoTransaction。create_revision 中“修订+媒体创建、发布状态推进、审计”必须保持单一事务。不要让 delivery.rs 调用本文件的私有 next_revision_no，避免形成子模块间耦合。
  - **backend/services/src/publication/delivery.rs**：放置商城投递查询和完整投递生命周期：ProductPublicationDeliveryFilter、delivery_list、deliver_revision，以及私有 idempotent_delivery_result、settle_delivery_success、settle_delivery_failure。该文件集中管理 inbox_message、投递记录、发布生效推进、integration_error_task 和审计写入。
    - 依赖/注意：通过 super::service::PublicationService 访问 db 和 connector，通过 super::connector::{ClassifiedError, PublishAck} 引用连接器结果类型。依赖 PublicationExt、IntegrationOpsExt、AccessControlExt、Transactional 和 NoTransaction。必须严格保留事务 1 写 inbox_message+审计、事务外调用 connector.publish_revision、事务 2 写成功或失败结果的顺序；不要把外部调用移入事务。三个私有结算 helper 应与 deliver_revision 保持在同一文件，避免扩大可见性或形成 delivery 与其他业务子模块的循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
