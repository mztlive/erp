# S00 共享基元与单据基础设施

## 1. 元信息

- 分支：`feat/erp-s00-document-infra`
- 业务期：`p1`（一期共享基元）
- 依赖阶段：无（`depends_on: []`）；后续业务 worktree 依赖本阶段稳定主键、`work_item_type` 注册表与 DTO 契约
- 本阶段是否要求单独编译：`must_compile=false`
  - 中间阶段：仅实现 `owns_modules` 目录内 entities / repository / services / handler
  - 允许因缺少 `lib.rs` / `mod.rs` / routes / `DatabaseExt` / indexes 注册而无法通过 workspace 编译
  - 共享汇合文件**禁止**本阶段并行修改，一律写入 `docs/dev-plan/S00-PATCHNOTES.md` 注册清单，由最终集成汇合阶段应用并执行 `cargo fmt/check/clippy/test --workspace`

## 2. 目标与业务范围

本阶段只交付**跨域共享基元与单据基础设施**契约，使后续域 worktree 可无冲突引用稳定主键与待办类型。业务范围必须能在下列文档找到依据；禁止发明未写表或同义单据。

| # | 能力 | 文档依据 |
| --- | --- | --- |
| 1 | 来源系统 `source_system` 与外部身份映射 `external_identity_map` / `external_identity_target` | `erp-data-model.md` §4.1、§5.1、§6.1；`erp-mall-data-mapping.md` 映射表；W17 来源谱系 |
| 2 | 跨域单据注册 `business_document`、关系 `document_relation`、历史参与人 `document_participant` | `erp-data-model.md` §5.1、§6.1 `business_document`/`document_relation`；§4.6 / `customer_assignment` 对 `document_participant` 写入规则；W19 历史参与权 |
| 3 | 固定状态机动作流水 `workflow_action`（追加式，非可配置审批引擎） | `erp-data-model.md` §6.1 `workflow_action`、§4.6、§7；`erp-phase-1.md` §5.2「可配置审批流不建设」、§10 固定状态机 |
| 4 | 正式待办 `work_item` 领域服务基元：创建/领取/完成/关闭/转交/暂挂；固定 `work_item_type` 注册表 | `erp-data-model.md` §6.1 `work_item`；W02 §1–§8；`erp-phase-1.md` §4.6.2、§5.1 |
| 5 | 批量选择快照 `bulk_selection_snapshot` / `bulk_selection_item` | `erp-data-model.md` §6.1；W08 导出「当前筛选全部」等引用 |
| 6 | 后台任务 `background_job` / `background_job_item`（统一进度注册，不替代领域强类型任务表） | `erp-data-model.md` §6.1；W18 导入应用进度；W17 同步任务进度可挂接；W25 导出引用 |
| 7 | 业务审计事件 `audit_event`（与既有 IAM `audit_log` 并存，职责不同） | `erp-data-model.md` §4.5、§5.1；W19 §5.2 审计事件字段 |
| 8 | 文件资产 `file_asset` 与业务附件关联 `document_attachment` | `erp-data-model.md` §4.5、§6.1 `file_asset`；§5.1 `document_attachment`；W18 保留策略；W14/W21 媒体引用 |
| 9 | 数据范围 `data_scope` 实体（与既有 Casbin 角色权限协同，**不重写 IAM**） | `erp-data-model.md` §5.1；§4.6 配置化权限；W19 数据范围视图 `scopeType/scopeTargets` |

前端契约消费面：

- **W02** `unified-task-queue`：`WorkItemActionCommand` / 队列查询 / 领取条件更新语义
- **W19** `access-audit`：`data_scope` 列表与 `audit_event` 查询扩展
- **W18** `import-opening` / **W17** `mall-sync`：`background_job` 进度与结果下载契约；W18 确认复用 W02 完成动作

## 3. 明确不在范围

防止 scope creep。下列能力本阶段**禁止**实现或伪装实现：

1. **可配置审批流引擎**：禁止动态节点/路由配置；仅固定状态机 + 追加 `workflow_action`（`erp-phase-1.md` §5.2、§10；`erp-data-model.md` §4.6）。
2. **一期事件机制与对外消息总线**：禁止 `inbox_message` / webhook / outbox / 消息中间件（`erp-phase-1.md` §3、§5.2；集成治理表属二期普通表组，非本阶段 owns）。
3. **完整财务总账与法定报表**（`erp-phase-1.md` §5.2）。
4. **禁止发明未写入 data-model 的业务单据或同义 `work_item_type`**：`work_item_type` 仅允许 data-model §6.1 已列固定码；页面/API/后台任务不得临时创造同义代码。
5. **不重写 IAM**：不改 Casbin 适配、`role`/`permission`/`user_role` 既有链路；`data_scope` 仅新增实体与协同查询，不替换 `RbacService`。
6. **不替代领域强类型表**：`business_document` 不是万能单据表；`background_job` 不替代商城同步/导入批次等强类型任务表，只登记统一进度与安全边界。
7. **不在本阶段实现各业务 handler 的领域 decision**（采购确认、票款复核、映射确认、导入确认等）；仅提供 `work_item` 基元与「完成钩子」接口，供后续域 service 在同一事务调用。
8. **不建设租户表/租户外键**（`erp-mall-data-mapping.md`：当前两期单公司；来源隔离用 `source_system`）。
9. **本阶段不修改共享汇合文件**：`services/src/lib.rs`、`entities/src/lib.rs`、`database/src/repository/mod.rs`、`extensions.rs`、`indexes.rs`、`apps/web-api` 的 routes / handler `mod.rs` / `app_state` / `main`。
10. **不建设「标记完成」独立伪接口**：正式完成必须由领域事务调用基元；HTTP 仅暴露 W02 统一动作命令中的 CLAIM/DEFER/TRANSFER/CLOSE 与任务内非终结动作骨架。

## 4. 代码落点与目录布局（强制统一风格）

对齐 `backend/AGENTS.md` + rust-coding-standards：

- 分层固定：HTTP Handler → Service → Repository → MongoDB
- Handler：协议适配 / `#[permission_macros::permission]` / `ApiResponse`；禁止直连 DB
- Service：事务与跨聚合编排；按用例拆文件；查询方法用名词、写操作用动词
- Repository：只暴露数据事实；`command` / `query` / `transaction` 拆分；事务方法 `_with_session`
- Entities：含 `BaseModel`；不变式与校验下沉类型；方法 ≤30 行有效代码；**每个公共/私有方法均须完整文档注释**
- DTO 定义在 `services`，Handler 复用；禁止 handler 同构重复类型
- `mod.rs` 仅：模块声明、结构体、构造、re-export；禁止堆主业务实现

### 4.1 建议树

```text
backend/entities/src/
  source_system/{mod.rs, data.rs, status.rs, validation.rs}
  external_identity/{mod.rs, map.rs, target.rs, id.rs, status.rs, validation.rs}
  business_document/{mod.rs, data.rs, validation.rs}
  document_relation/{mod.rs, data.rs, relation_type.rs}
  document_participant/{mod.rs, data.rs, validation.rs}
  workflow_action/{mod.rs, data.rs, action_type.rs}
  work_item/{mod.rs, data.rs, status.rs, types.rs, validation.rs}
  bulk_selection/{mod.rs, snapshot.rs, item.rs, status.rs}
  background_job/{mod.rs, job.rs, item.rs, status.rs, validation.rs}
  audit_event/{mod.rs, data.rs, validation.rs}
  file_asset/{mod.rs, data.rs, retention.rs, security.rs, validation.rs}
  data_scope/{mod.rs, data.rs, scope_type.rs, validation.rs}
  document_attachment/{mod.rs, data.rs, validation.rs}

backend/database/src/repository/
  source_system/{mod.rs, command.rs, query.rs, transaction.rs}
  external_identity/{mod.rs, command.rs, query.rs, transaction.rs}
  business_document/{mod.rs, command.rs, query.rs, transaction.rs}
  document_relation/{mod.rs, command.rs, query.rs, transaction.rs}
  document_participant/{mod.rs, command.rs, query.rs, transaction.rs}
  workflow_action/{mod.rs, command.rs, query.rs, transaction.rs}
  work_item/{mod.rs, command.rs, query.rs, transaction.rs}
  bulk_selection/{mod.rs, command.rs, query.rs, transaction.rs}
  background_job/{mod.rs, command.rs, query.rs, transaction.rs}
  audit_event/{mod.rs, command.rs, query.rs}
  file_asset/{mod.rs, command.rs, query.rs, transaction.rs}
  document_attachment/{mod.rs, command.rs, query.rs, transaction.rs}
  data_scope/{mod.rs, command.rs, query.rs, transaction.rs}

backend/services/src/
  document_infra/
    mod.rs                 # DocumentInfraService 结构 + 构造
    dto.rs                 # 或 dto/{mod,source,identity,document,workflow,scope,audit}.rs
    source_system.rs       # 来源系统创建/启停/查询
    external_identity.rs   # 解析/登记 map、追加 target、冲突态
    business_document.rs   # 与强类型表配对注册（仅供域服务调用的 API）
    document_relation.rs
    document_participant.rs
    workflow_action.rs     # 追加动作流水
    data_scope.rs          # 范围 CRUD + 有效范围解释辅助
    audit_event.rs         # 写入与查询编排
    query.rs               # 跨表只读投影
  work_item/
    mod.rs
    dto.rs                 # WorkItemActionCommand 对齐 W02 §8.2
    create.rs              # 域内创建有效待办（唯一性：对象+类型）
    claim.rs               # 条件更新原子领取
    action.rs              # DEFER / TRANSFER / CLOSE / 非终结 WORK_ITEM_ACTION 骨架
    complete.rs            # complete_with_session：供领域事务调用
    query.rs               # 队列列表、详情、计数
    registry.rs            # 固定 work_item_type → handler_key/completion_action 注册表
  background_job/
    mod.rs
    dto.rs
    create.rs
    progress.rs            # 进度与 item 结果写入
    cancel.rs
    query.rs
    download.rs            # 结果下载鉴权编排
  file_asset/
    mod.rs
    dto.rs
    create.rs              # 元数据登记 + 安全扫描状态
    attach.rs              # document_attachment 关联
    query.rs
    download.rs            # 按对象/角色/data_scope 重验

backend/apps/web-api/src/core/handler/admin/
  document_infra/{mod.rs, source_system.rs, external_identity.rs,
                  business_document.rs, data_scope.rs, audit_event.rs}
  work_item/{mod.rs, query.rs, actions.rs}
  background_job/{mod.rs, query.rs, command.rs}
  file_asset/{mod.rs, command.rs, query.rs, attachment.rs}
```

### 4.2 owns_modules（本阶段唯一可写代码路径）

见规划 JSON 的 `owns_modules` 列表；实现时不得越界修改其它域目录。

### 4.3 汇合文件仅集成阶段改

在 `docs/dev-plan/S00-PATCHNOTES.md` 声明应追加项（本阶段只写清单，不改文件）：

| 汇合点 | 应追加内容 |
| --- | --- |
| `entities/src/lib.rs` | `mod` + `pub use` 各新实体模块 |
| `database/src/repository/mod.rs` | `mod` 各 repository 子目录 |
| `database/src/repository/extensions.rs` | `DatabaseExt` 访问器（集合名见 §5） |
| `database/src/indexes.rs` | `ensure_indexes` 调用本阶段索引函数 |
| `services/src/lib.rs` | `pub mod document_infra; work_item; background_job; file_asset` |
| `handler/admin/mod.rs` | `pub mod document_infra; work_item; background_job; file_asset` |
| `routes/admin.rs` | nest 路由 + `with_permission` |
| `app_state` / `main` | 如需挂载 Service 实例 |

样板参考：`entities/src/role.rs`、`database/src/repository/role.rs`、`services/src/iam/`、`handler/admin/role.rs`。

## 5. 数据模型与索引

集合名约定：与 `erp-data-model.md` 表名一致（snake_case 单数）。实体一律 `#[serde(flatten)] base: BaseModel`（`id`/`version`/`created_at`/`updated_at`/`deleted_at`）。写操作软删/更新走 **id + version 乐观锁**。

### 5.1 表与关键字段（禁止超文档发明列）

#### `source_system`（§6.1）

- 字段：`code`、`system_type`（`ERP`/`MALL`/`SUPPLIER`）、`name`、`status`（启用/停用）
- 索引：`code` 唯一；`system_type + status` 查询

#### `external_identity_map` / `external_identity_target`（§6.1）

- map：`source_system_id`、`object_type`、`external_id`（原值）、`external_id_key`（规范化 UTF-8 二进制比较键）、`mapping_status`、`mapped_at`/`mapped_by`
- target：`external_identity_map_id`、`internal_object_type`/`internal_object_id`、`relation_role`（`PRIMARY`/`COMPONENT`/`MERGED_INTO`/`REVISION_SOURCE`）、`valid_from`/`valid_to`、`status`、`approved_at`/`approved_by`
- 索引：`(source_system_id, object_type, external_id_key)` 唯一；target `(map_id, internal_object_type, internal_object_id, relation_role, valid_from)` 唯一；反向 `(internal_object_type, internal_object_id, status)`；待确认/冲突状态索引
- 规则：根业务身份同一时点仅一个有效 `PRIMARY`；禁止覆盖旧 target 丢历史；外部 ID 不做大小写折叠（卡券销售单号仅去协议禁止的首尾空白）

#### `business_document`（§6.1）

- 字段：`document_type`、`document_no`、`formalized_at`
- 索引：`(document_type, document_no)` 唯一；`document_no` 全局搜索
- 规则：与强类型业务表一对一；**禁止**脱离强类型表单独创建空单据（注册 API 仅供领域 create 事务内调用）

#### `document_relation`（§6.1）

- 字段：`from_document_id`、`to_document_id`、`relation_type`（`CHANGES`/`RETURNS`/`REFUNDS`/`REVERSES`/`RED_OF`/`DERIVED_FROM`）
- 索引：`(from, to, relation_type)` 唯一；`to_document_id + relation_type` 反向

#### `document_participant`（§5.1、§4.6、`customer_assignment` 规则）

- 用途：单据历史参与人及查看依据；新单据写入当时负责人/协作销售；负责人变更不删除历史参与权
- 建模至少包含：关联 `document_id`（或稳定业务对象身份）、`user_id`、参与角色（与归属角色语义一致，如 `OWNER`/`COLLABORATOR` 及文档允许的历史职责）、有效依据/时间审计字段
- 索引：按 `document_id`、按 `user_id` 查询历史参与权
- **禁止**仅凭当前 `data_scope` 反推历史参与权（W19）

#### `workflow_action`（§6.1）

- 字段：`document_id`、`action_type`、`from_status`/`to_status`、`actor_id`/`actor_role`、`comment`、记录时间
- 索引：`document_id + recorded_at`；`actor_id + recorded_at`
- 语义：追加式流水；不驱动可配置流引擎

#### `work_item`（§6.1 + W02）

- 固定类型（两期至少，禁止同义码）：  
  `PROCUREMENT_CONFIRMATION`、`LOW_MARGIN_MANAGER_CONFIRMATION`、`PURCHASE_ORDER_REVIEW`、`CARD_FUNDS_REVIEW`、`CARD_FUNDS_DELTA_REVIEW`、`CARD_SALES_MANAGER_APPROVAL`、`CARD_SALES_OPERATION_APPROVAL`、`OWNERSHIP_MIGRATION_SALES_CONFIRMATION`、`OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION`、`INVENTORY_ADJUSTMENT_REVIEW`、`FINANCE_CORRECTION_REVIEW`、`SUPPLIER_SETTLEMENT_REVIEW`、`IMPORT_BUSINESS_CONFIRMATION`、`INTEGRATION_RESULT_UNKNOWN`、`BUSINESS_EXCEPTION`
- 一期导入确认固定注册：  
  `IMPORT_BUSINESS_CONFIRMATION` → `business_object_type=LEGACY_IMPORT_BATCH`、`handler_key=import_business_confirmation`、目标 W18、`completion_action=COMPLETE_IMPORT_BUSINESS_CONFIRMATION`；decision 仅 `CONFIRM_SCOPE`|`RETURN_FOR_FIX`
- 字段：`work_item_type`、`business_object_type`/`business_object_id`、`subject_version`、`status`（`UNCLAIMED`/`IN_PROGRESS`/`COMPLETED`/`CLOSED`）、`owner_role`/`owner_user_id`、`priority`/`due_at`、`reason_code`/`impact_summary`、`completion_action`、`completed_at`/`completed_by`
- 索引：同一业务对象+任务类型同时最多一个有效任务；`owner_role + owner_user_id + status + due_at` 队列索引
- 领取：条件更新 `status=UNCLAIMED → IN_PROGRESS` 写领取人；影响 0 行=已被领
- 关闭：仅重复/误派/有替代任务；审批/确认/结果未知/未完成补偿禁止人工关闭
- 完成/关闭本身不改业务事实

#### `bulk_selection_snapshot` / `bulk_selection_item`（§6.1）

- snapshot：`selection_type`、`data_cutoff_at`、`item_count`、`created_by`/`created_at`/`expires_at`、`status`
- item：`selection_snapshot_id`、`object_type`/`object_id`、`expected_version`/`expected_hash`、`result_status`/`result_code`
- 索引：`(selection_snapshot_id, object_type, object_id)` 唯一
- 规则：确认后目标集合与水位不可改；执行逐项重验权限/范围/版本；不绕过正式审批

#### `background_job` / `background_job_item`（§6.1）

- job：`job_no`/`job_type`、`domain_job_type`/`domain_job_id`、`selection_snapshot_id`、`requested_by`/`request_id`、`input_file_asset_id`/`result_file_asset_id`、`status`、计数器、`started_at`/`finished_at`/`last_progress_at`、`result_expires_at`、`error_summary`
- item：`background_job_id`/`item_no`、对象身份、期望版本、导入行列定位、结果码与结果对象
- 索引：`job_no` 唯一、`request_id` 唯一、`(background_job_id, item_no)` 唯一
- 保留：导出 7 天；成功导入白名单长期；失败诊断 30 天；成功/失败资产不得共用 `file_asset_id`

#### `file_asset`（§6.1）

- 字段：`storage_object_key`、`file_name`/`content_type`/`byte_size`、`content_hmac`（keyed，禁裸摘要）、`security_scan_status`、`sensitivity_class`/`retention_class`、`expires_at`/`destroyed_at`、`created_by`/`created_at`
- 规则：扫描通过且允许保留类才可关联业务对象；下载按对象+角色+data_scope 重验；密钥/签名 URL 不进业务日志

#### `document_attachment`（§5.1）

- 用途：业务对象与 `file_asset` 的受控关联（用途、排序、归属 document/object）
- 字段至少：`file_asset_id`、业务对象类型/ID（或 `document_id`）、用途码、排序、创建审计
- 索引：按业务对象查附件；按 `file_asset_id` 反查

#### `audit_event`（§4.5、W19 §5.2）

- 字段语义：操作者、动作、对象、请求追踪号、时间、变更字段名；敏感字段只记「已变更」摘要，不记完整旧/新值
- 查询投影对齐 W19：`auditEventId`、`recordedAt`、`actorId`/`actorLabel`、`actorRole`、`actionType`、`objectType`/`objectId`/`objectLabel`、`requestId`/`traceId`、`result`、`changedFieldNames`、`safeDigest`
- **与**现有 `audit_log`（账号/RBAC 操作日志）**并存**：`audit_event` 服务业务安全审计与变更留痕

#### `data_scope`（§5.1、W19）

- 用途：配置化数据范围；与 Casbin 模块动作权限协同
- 字段对齐 W19：`subjectType`（ROLE|USER）、`subjectId`、`scopeType`（公司/组织/团队/本人负责/协作等固定策略）、`scopeTargets`
- 不实现完整字段级策略引擎（字段策略属 IAM/W19 后续扩展）；本阶段实体可供范围过滤与列表

### 5.2 索引实现约定

- 在各 repository 目录提供 `indexes()` 纯函数返回 `IndexModel` 列表（或集中 `indexes` 片段）
- 集成阶段并入 `database/src/indexes.rs` 的 `ensure_indexes`
- 唯一约束必须由唯一索引保证，不仅依赖应用层

## 6. API 与权限草图

### 6.1 挂接方式

- 全部 admin：JWT + RBAC 中间件（现有 `routes/admin.rs` 模式）
- Handler：`#[permission_macros::permission(group, group_desc, desc, resource, action)]`
- 路由：`with_permission(method(handler), rbac, handler::*_permission_key())`
- 本阶段**不改** `routes/admin.rs`；PATCHNOTES 列出 nest 路径与 permission key

### 6.2 Permission key 命名建议

| resource | action 示例 | 说明 |
| --- | --- | --- |
| `source_system` | `list`/`create`/`update` | 来源系统 |
| `external_identity` | `list`/`resolve`/`approve_target` | 外部身份 |
| `business_document` | `list`/`get` | 只读注册查询（写由域事务） |
| `work_item` | `list`/`get`/`claim`/`action` | 统一待办 |
| `bulk_selection` | `create`/`get`/`confirm` | 批量快照 |
| `background_job` | `list`/`get`/`create`/`cancel`/`download` | 后台任务 |
| `file_asset` | `create`/`get`/`download` | 文件资产 |
| `document_attachment` | `list`/`create` | 附件关联 |
| `data_scope` | `list`/`create`/`update`/`delete` | 数据范围 |
| `audit_event` | `list`/`get` | 审计查询 |

### 6.3 REST 草图（前缀 `/admin`）

| 方法路径 | 说明 |
| --- | --- |
| `GET/POST /admin/source-systems`、`PUT .../{id}` | 来源系统 |
| `GET /admin/external-identities`、`POST .../resolve`、`POST .../{map_id}/targets` | 外部身份 |
| `GET /admin/business-documents`、`.../{id}`、relations/participants/workflow-actions | 单据注册只读 |
| `GET /admin/work-items`、`POST .../{id}/actions` | 统一待办 |
| `POST /admin/bulk-selections`、`GET/POST .../confirm` | 批量快照 |
| `GET/POST /admin/background-jobs`、cancel/download | 后台任务 |
| `POST /admin/file-assets`、download-token | 文件资产 |
| `GET/POST /admin/document-attachments` | 附件关联 |
| `GET/POST/PUT/DELETE /admin/data-scopes` | 数据范围 |
| `GET /admin/audit-events` | 审计查询 |

要点：

1. **Work item 动作单一入口** `POST /admin/work-items/{id}/actions`  
   Body 对齐 W02 §8.2：`WorkItemActionCommand`（`action.kind` = CLAIM|DEFER|TRANSFER|CLOSE|WORK_ITEM_ACTION；`expectedSubjectVersion`；可选 `decision` 占位由后续域扩展）。  
   返回 `WorkItemActionResult`：`workItemId`、`workItemStatus`、`recordId`、可选 `businessResult`/`subjectVersion`。
2. **正式完成不设独立 HTTP「complete」**：领域 service 调 `WorkItemService::complete_with_session`；handler 完成路径仅在已注册且 decision 类型安全时透传（本阶段可对未知 completion_action fail-closed）。
3. **background_job 创建** 需 `request_id` 幂等；下载接口再次鉴权并记 `audit_event`。
4. **file_asset 创建** 可与现有 `upload` 配合：先落盘/对象存储拿 `storage_object_key`，再登记元数据与 HMAC；禁止把签名 URL 当长期业务值。
5. **business_document 写入**默认不开放任意 POST；若需调试只读查询即可。注册方法 `register_with_session` 供销售/采购等域事务调用。

### 6.4 错误与并发

- 领取冲突、版本冲突、权限不足、关闭策略拒绝、复核策略未注册等：稳定错误码 + 业务文案（对齐 W02/W19 blocker 语义）
- 多集合写：`Transactional::with_transaction`；Repository 只提供 `_with_session`

## 7. 前端集成点

本阶段后端契约优先；前端仍可 mock，但 **types 字段必须对齐**，避免二次破坏。

| 触点 | 路径 | 对齐要求 |
| --- | --- | --- |
| W02 队列 | `erp-client/features/unified-task-queue/{types,queries,filter-work-items,queue-url}.ts` | `WorkItemStatus` 以 data-model 四态为准（mock 中 `PENDING`/`TRANSFERRED` 仅展示叠加，不落库新状态）；Query Key：`unifiedQueueKeys` 含用户/角色/筛选；后续替换 `mock/session-state` 为真实 API |
| W02 fixture | `erp-client/mock/work-items.ts` | `handlerKey`/`completionAction`/`closeAllowed`/`allowedActions` 与注册表一致 |
| W19 | `erp-client/features/access-audit/{types,queries,session}.ts` | `ScopeRow.scopeType/scopeTargets` ← `data_scope`；`AuditEventRow` ← `audit_event`；历史参与层 `HISTORICAL_PARTICIPANT` ← `document_participant` |
| W18 | `erp-client/features/import-opening/` | 进度字段对齐 `background_job` 计数；确认走 W02 `WorkItemActionCommand`，不私造完成接口 |
| W17 | `erp-client/features/mall-sync/` | 任务进度可关联 `background_job`；映射确认复用 W02 命令；来源身份展示 `external_identity_*` |
| 附件组件 | `erp-client/components/business/attachments.tsx` | 引用 `file_asset_id` + `document_attachment` |

Mock 替换策略：

1. 先固定 TS 类型与 query key 形状；
2. `api.ts` 增加真实 fetch，feature flag / 环境变量切换 mock；
3. 禁止前端根据角色名硬编码 `allowedActions`；一律信服务端。

用户可见文案：禁止把 `work_item`、租约、令牌、指纹等实现词上屏（`ui-glossary.md`）。

## 8. 实现任务清单

按「文件边界 → 建模 → 仓储 → 服务 → HTTP → 待集成说明 → 测试」推进。

### 8.1 文件边界与实体

1. 为每个 owns 实体目录建立 `mod.rs` + 按规则主题拆分（status/validation/data）。
2. 实现 newtype ID（如 `WorkItemId`、`SourceSystemId`）与 `*Data`/`*Update`。
3. 枚举固定码：`WorkItemType`、`WorkItemStatus`、`RelationRole`、`DocumentRelationType`、`SystemType` 等；未知码反序列化失败。
4. `work_item::registry`：编译期/常量注册表（type→handler_key/completion_action/business_object_type）；`IMPORT_BUSINESS_CONFIRMATION` 按 data-model 表固化。
5. 实体单测：规范化、唯一性相关不变式、状态迁移允许集（领取/暂挂/完成/关闭）。

### 8.2 仓储

6. 各集合 `Repository` 或专用 `XxxRepository`：`command`/`query`/`transaction`。
7. `work_item`：实现原子领取 `claim_if_unclaimed_with_session`（条件更新）。
8. `external_identity`：按 `(source_system_id, object_type, external_id_key)` 查建。
9. `background_job`：`request_id` 幂等查；item 批量写入。
10. 乐观锁更新统一封装（id+version）。
11. **不改** `extensions.rs`/`indexes.rs`；索引函数写在 owns 目录并列入 PATCHNOTES。

### 8.3 服务

12. `document_infra`：来源系统 CRUD 编排；身份 resolve/map；`register_business_document_with_session`；relation/participant/workflow 追加；`data_scope` CRUD；`audit_event` append + 查询（敏感字段脱敏）。
13. `work_item`：create（唯一有效任务校验）、claim、defer、transfer（原因必填）、close（reasonCode/替代任务）、`complete_with_session`（校验领取人+版本+completion_action）、队列 query（scope=mine|role_pool|team，排序：超期→优先级→due_at→created_at）。
14. `background_job`：create（可挂 selection_snapshot）、progress、cancel（只停未开始项）、download 鉴权。
15. `file_asset`：登记、扫描状态更新、attach、download token 编排。
16. 跨集合写必须事务：例「完成待办+workflow_action」「确认 mapping 由后续域调用 complete_with_session」。

### 8.4 HTTP

17. Handler 复用 service DTO；`permission` 宏齐全。
18. work_item actions 单一入口解析 `kind`。
19. 审计查询支持 W19 筛选：时间窗、actor、action、object、result、traceId、eventId；策略缺失时的保守窗口由服务端参数表达（本阶段可返回策略占位结构，不硬编码前端 24h）。
20. **不改 routes**：PATCHNOTES 写清路径表。

### 8.5 测试

21. 实体：happy + 非法枚举/超长/空字段。
22. Service：领取并发（第二人失败）、关闭策略拒绝、request_id 幂等、PRIMARY 唯一、complete 不改业务字段。
23. 至少各域一个失败路径（权限/版本冲突）。

### 8.6 文档产物

24. 写 `docs/dev-plan/S00-PATCHNOTES.md`：mod 清单、DatabaseExt 方法签名、索引名、路由表、permission keys、集合名。

## 9. Worktree / 并行约定

### 9.1 分支

```bash
git checkout -b feat/erp-s00-document-infra
```

### 9.2 禁止触碰

- 一切非 `owned_paths` / `owns_modules` 的业务域代码
- 共享汇合文件（见 §4.3）
- IAM 核心：`services/src/iam/**`、`casbin_adapter`、既有 `role`/`account` 行为变更（只读参考）
- 前端大规模 UI 重构（本阶段以契约稳定为主；若改 types 保持 mock 可运行）

### 9.3 与其它阶段接口边界

| 下游 | 依赖本阶段 |
| --- | --- |
| 销售/采购/库存/财务域 | `business_document.register_with_session`、`document_participant` 写入、`workflow_action.append`、`work_item.create/complete_with_session` |
| W17 映射 | `external_identity_*`、`work_item` 动作命令 |
| W18 导入 | `file_asset` 保留类、`background_job`、`IMPORT_BUSINESS_CONFIRMATION` 注册 |
| W19 | `data_scope`、`audit_event` 查询 |
| 导出/批量 | `bulk_selection_*` + `background_job` |

DTO 边界：对外稳定类型放在 `services/*/dto.rs`；后续域 **复用** 而非复制 `WorkItemActionCommand`、`BackgroundJobView`、`FileAssetRef`。

depends_on DAG：本阶段无上游；其它阶段 `depends_on` 含 S00 时，合并顺序先 S00 再域阶段，但编码可在契约稳定后并行（仍不改汇合文件）。

## 10. 验收标准

### 10.1 功能验收

- [ ] 十六张表对应实体可构造/校验，字段与 `erp-data-model.md` §5.1/§6.1 一致，无发明列
- [ ] `work_item` 原子领取：两人并发仅一人成功
- [ ] 固定 `work_item_type` 注册表完整；未知类型 fail-closed
- [ ] `IMPORT_BUSINESS_CONFIRMATION` 元数据与 data-model 表一致
- [ ] CLOSE 策略：审批/确认/结果未知/补偿类拒绝；重复/误派需 reasonCode
- [ ] TRANSFER 必填原因并写审计；不完成任务
- [ ] DEFER 回 `UNCLAIMED`
- [ ] `complete_with_session` 校验领取人+subject_version+completion_action；本身不写业务事实
- [ ] `external_identity` 唯一键与 PRIMARY 约束单测覆盖
- [ ] `business_document` 无法在无强类型配对语义下被「空注册」滥用（API/服务校验）
- [ ] `background_job` `request_id`/`job_no` 幂等；取消不回滚已提交 item
- [ ] `file_asset` 成功/失败保留类分离约束有校验入口
- [ ] `data_scope` CRUD 不破坏 Casbin 既有角色权限
- [ ] `audit_event` 查询不返回敏感旧/新值
- [ ] PATCHNOTES 完整，足以让集成阶段无歧义挂接

### 10.2 风格验收（rust-coding-standards / AGENTS）

- [ ] 分层无穿越；Handler 无 DB
- [ ] Service 按用例拆文件；`mod.rs` 无主业务堆叠
- [ ] Repository command/query/transaction 分离
- [ ] DTO 仅 services 定义；Handler 复用
- [ ] 事务边界在 Service；多集合写用 `with_transaction`
- [ ] 方法 ≤30 行有效代码；新增/修改方法均有完整文档注释
- [ ] 查询名词、写操作动词命名

### 10.3 文档范围验收

- [ ] 无 §3 禁止项实现
- [ ] 无未注册 `work_item_type`
- [ ] 前端 touchpoints 类型可映射到本阶段 DTO 字段
- [ ] 业务范围均可回溯至 data-model / phase-1 / W02 / W17 / W18 / W19

### 10.4 编译策略

- 本阶段 `must_compile=false`：worktree 内允许因未注册 mod/routes/indexes 而 `cargo check --workspace` 失败
- **合并进集成分支后，由最终集成汇合阶段**集中应用 PATCHNOTES，并保证：  
  `cargo fmt --all`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features`、`cargo test --workspace`

---

*阶段 ID：S00 · 分支：feat/erp-s00-document-infra · phase_tag：p1 · must_compile：false*
