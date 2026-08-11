# `backend/services/src/projection/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/projection/mod.rs` |
| 扫描行数 | 1013 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议拆分。当前 1013 行文件混合了服务容器、商城连接器、四类查询、投影及版本形成、外部下发结算和连接器测试。按照 services 域目录的模块根模式，让 mod.rs 仅保留领域文档、子模块声明和公开 re-export，并新增 service.rs、connector.rs、query.rs、revision.rs、delivery.rs。拆分后最大文件预计约 350 行，所有文件均可稳定控制在 800 行以内；公开导入路径和业务事务边界可以保持不变。主要实施注意点是 ProjectionService 字段需使用 pub(super)、私有 helper 要与调用簇共同迁移，以及 connector 不应反向依赖 service。
- 拆分建议：
  - **backend/services/src/projection/service.rs**：定义并实现服务容器：ProjectionService，以及 ProjectionService::new。结构体保存 Database 和 Arc<dyn MallConnector>；mod.rs 通过 pub use self::service::ProjectionService 保持原公开路径。
    - 依赖/注意：使用 super::connector::MallConnector。由于 query.rs、revision.rs、delivery.rs 是 service.rs 的兄弟模块，db 与 connector 字段应设为 pub(super)，不能继续使用仅 service.rs 内可见的私有字段，也不应设为 pub。
  - **backend/services/src/projection/connector.rs**：放置商城连接器边界及默认实现：ClassifiedError、DeliverAck、MallConnector、UnavailableMallConnector、impl MallConnector for UnavailableMallConnector；同时迁入 sample_revision、default_connector_fails_closed_with_classified_error、mock_connector_success_returns_ack 及测试内 MockConnector。
    - 依赖/注意：该文件只依赖 entities 的 ErrorClass、SalesOrderProjectionRevision、SourceSystemId 和标准库 Future/Pin，不应反向引用 ProjectionService。mod.rs 必须 re-export ClassifiedError、DeliverAck、MallConnector、UnavailableMallConnector，以保持 services::projection::* 的既有公共 API。
  - **backend/services/src/projection/query.rs**：集中所有只读查询编排：SalesOrderProjectionFilter、SalesOrderProjectionDeliveryFilter、ProjectionService::projection_list、ProjectionService::projection_detail、ProjectionService::revision_list、ProjectionService::delivery_list。
    - 依赖/注意：通过 super::service::ProjectionService 扩展 inherent impl，通过 super::dto 导入 PageView、各列表参数与 View、SortDir。只需要访问 ProjectionService 的 pub(super) db 字段；不依赖 connector 或 revision/delivery 私有 helper，因此不会形成循环依赖。
  - **backend/services/src/projection/revision.rs**：集中投影建立和版本推进工作流：ProjectionService::create_projection、ProjectionService::create_revision、私有方法 next_revision_no、私有方法 load_current_sales_revision，以及私有自由函数 voucher_expiry、to_projection_card_form。
    - 依赖/注意：通过 super::service::ProjectionService 扩展 impl；通过 super::dto::projection_content_hash 直接调用现有 DTO 内部指纹函数。next_revision_no 和 load_current_sales_revision 仅被本文件方法调用，应继续保持私有；voucher_expiry 与 to_projection_card_form 也不需要 pub(super)。该文件会创建 PendingSend 下发记录，但该动作属于投影版本原子形成流程，不应为追求类型归类而移到 delivery.rs。
  - **backend/services/src/projection/delivery.rs**：集中投影外部下发及事务结算：ProjectionService::deliver_revision、私有方法 idempotent_delivery_result、settle_delivery_success、settle_delivery_failure。保留下发前消息事务、事务外 MallConnector 调用，以及成功或失败后的第二段事务。
    - 依赖/注意：通过 super::service::ProjectionService 访问 pub(super) db 和 connector，通过 super::connector::{ClassifiedError, DeliverAck} 使用连接器结果类型，通过 super::dto 使用 DeliverProjectionRevisionRequest 与 ProjectionDeliveryResultView。三个 helper 均只服务 deliver_revision，应留在同一文件并保持私有，避免跨兄弟模块使用 pub(super)。外部 connector 调用必须继续位于两个数据库事务之间，拆分时不能改变事务边界。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
