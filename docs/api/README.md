# ERP 浏览器 API 契约 v1

本文冻结 `erp-client` 与 ERP 后端之间的浏览器业务 API。它定义稳定的 HTTP 资源、命令、
数据格式、并发与幂等语义、权限结果和错误处理。可生成客户端的核心规范见
[erp-client-v1.openapi.yaml](erp-client-v1.openapi.yaml)，稳定错误码见
[error-catalog.md](error-catalog.md)。

本契约只服务 ERP 浏览器客户端，不是商城或供应商的系统间集成契约。

---

## 1. 事实依据和维护顺序

API 只能翻译现有业务事实，不得借接口设计改变业务范围、单据性质或状态机。发生冲突时
按以下顺序处理：

1. [第一期建设说明](../erp-phase-1.md) 与 [第二期建设方案](../erp-phase-2.md)：业务范围、
   主责系统、部门职责、单据性质和业务流转；
2. [两期数据模型](../erp-data-model.md)：稳定身份、字段、状态、版本、约束、幂等与审计；
3. [商城数据映射](../erp-mall-data-mapping.md)：旧商城字段和来源接口兼容，不能反向改变规范模型；
4. [页面与导航地图](../erp-page-map.md)：页面职责、入口、角色和启用阶段；
5. [信息架构与作业流规范](../erp-information-architecture.md)：导航分组、工作区页签、任务接力、
   命令面板、默认列集和单据视图形态；
6. [页面布局与交互规范](../erp-interaction-spec.md)：页面操作、反馈、批量、并发和异常恢复；
7. 本目录：把上述事实冻结成前后端 HTTP 契约；下层不一致时修改 API 契约，不静默改变上层事实。

版本规则：

- 浏览器 API 的主版本前缀固定为 `/api/v1`；
- 新增可选响应字段和新的只读端点可以在 v1 内发布，客户端必须忽略未知响应字段；
- OpenAPI 中以 `enum` 声明的值域是闭合集合。新增、删除或改名同样属于生成客户端的破坏性
  变更，不能作为 v1 内的“兼容新增”；可扩展代码必须从一开始建模为受格式约束的字符串；
- 删除字段、改变字段语义、收紧既有请求或改变命令副作用必须发布新的 API 主版本；
- 页面阶段、P0/P1 和主责系统不进入 URL。阶段差异由 `context.features`、`ownerSystem`、
  `allowedActions` 和 `actionBlockers` 表达。

---

## 2. 契约边界

### 2.1 浏览器 API

以下内容属于本契约：

- ERP SPA 的上下文、全局搜索、工作台、任务、预警、后台任务和受控文件；
- 页面所需的 ERP 规范资源、查询投影和强类型业务命令；
- 浏览器可见的商城同步、执行投影、消费、供应商订单、接口错误和对账结果；
- 当前用户的动作权限、数据范围结果、阻断原因、下一责任和数据新鲜度；
- 后台任务进度、批量预览与确认、下载授权和统一错误。

浏览器只调用 ERP 后端。即使页面展示商城或供应商数据，也只能读取 ERP 已保存的事实、
投影、同步任务或错误任务，不能从浏览器直连商城、供应商或消息系统。

### 2.2 外部系统接口

下列接口必须使用独立契约，不得加入 `erp-client-v1.openapi.yaml`，也不得让前端生成这些
接口的客户端：

| 独立文件名 | 调用双方 | 边界 |
| --- | --- | --- |
| `mall-phase1-read-v1.openapi.yaml` | ERP 同步服务 → 商城 | 第一期基线、增量、全量清单和按单补拉的只读来源接口 |
| `mall-phase2-collaboration-v1.openapi.yaml` | ERP ↔ 商城 | 第二期执行投影、商品发布、原结果查询和受控协同命令 |
| `events/mall-business-facts-v1.asyncapi.yaml` | 商城 → ERP 消息入口 | 支付、退款、余额恢复、订单和其他不可变业务事实 |
| `supplier-connector-v1.openapi.yaml` | ERP 内部编排 → Supplier Connector | 统一供应商命令、原结果查询、取消、退款和标准化结果 |

外部接口使用各自的签名、回调、事件幂等键和脱敏日志规则；浏览器的 Bearer 身份、ETag、
页面动作和错误展示语义不应泄漏到外部协议。商城旧字段只能在映射层出现，不能成为浏览器
规范 DTO 的字段名。

---

## 3. HTTP 与数据格式

### 3.1 基础规则

- OpenAPI 版本：`3.1.0`；
- 编码：UTF-8 JSON，文件上传意图和下载授权也返回 JSON；
- 字段命名：`camelCase`；
- ERP 稳定 ID：不透明 `string`，客户端不得解析、排序或拼接业务含义；
- 可展示编号：单独使用 `orderNo`、`documentNo` 等字段，不能作为路由主键；
- 日期：ISO `YYYY-MM-DD`；
- 时间：RFC 3339 带偏移量的时间点，服务端响应使用 UTC `Z`，页面按
  `context.businessTimezone` 展示；
- 金额、单价、数量和比率：十进制定点数的 JSON `string`，禁止 JSON number；
- 币种：当前业务固定 `CNY`，金额对象仍显式返回 `currency: "CNY"`；
- 布尔值使用 JSON boolean，不使用 `0/1` 或字符串；
- 可空字段显式使用 `null`，缺失字段表示该字段不属于本响应视图或调用者无字段权限。

十进制精度：

| 语义 | 字符串格式 |
| --- | --- |
| 含税、不含税、税额和合计 | 固定 2 位小数，可带负号，不使用科学计数法 |
| 单价 | 最多 4 位小数 |
| 数量 | 最多 6 位小数 |
| 税率、配赠率 | 最多 6 位小数，值为比率而非百分号文本 |
| 卡张数 | 正整数 |

销售与采购金额必须先逐行舍入到分，再汇总已经舍入的行。客户端只展示服务端结果，不自行
重建正式金额口径。

### 3.2 认证和请求追踪

- 浏览器业务 API 使用 `Authorization: Bearer <access-token>`；
- 缺失、失效或过期凭证返回 `401`；
- 客户端可以发送 `X-Request-Id`，服务端始终在响应头和 envelope 中返回最终 `requestId`；
- Token、连接密钥、签名原文、卡号和卡密不得进入 URL、错误 `details`、日志或导出。

### 3.3 查询、筛选、分页与排序

所有列表采用服务端分页并返回准确总数：

- `page`：从 `1` 开始，默认 `1`；
- `pageSize`：只允许 `20`、`50`、`100`，默认 `50`；
- `sort`：逗号分隔的稳定字段，字段前 `-` 表示降序，例如
  `sort=-updatedAt,orderNo`；每个端点只接受 catalog 声明的字段；
- `q`：只用于端点声明的编号或名称模糊搜索，不替代精确筛选；
- 多值筛选：同一 query 参数重复出现，例如
  `commercialStatus=EFFECTIVE&commercialStatus=UNDER_REVIEW`；
- 时间范围：使用 `<field>From` 和 `<field>To`，均包含边界；
- 布尔、枚举和 ID 使用具名 query 参数，不接受可执行表达式、任意 JSON filter 或 SQL 片段；
- 非敏感筛选可进入浏览器 URL；手机号、邮箱、税号、银行账号、证件号和地址不得进入 URL。

列表 `data` 固定包含 `items` 和 `pageInfo`。`pageInfo` 包含 `page`、`pageSize`、
`totalItems`、`totalPages`。外部同步水位和事件游标不复用浏览器分页参数。

### 3.4 成功 envelope

所有 JSON 成功响应使用：

```json
{
  "data": {},
  "meta": {
    "requestId": "req_opaque",
    "dataAsOf": "2026-07-31T06:30:00Z",
    "warnings": []
  }
}
```

- `data` 为端点声明的精确 schema；
- `requestId` 用于审计和排障；
- `dataAsOf` 是本响应业务数据的水位，不等同于 HTTP 响应时间；
- `warnings` 只表达成功结果的非阻断提醒，不能承载失败；
- 创建资源返回 `201` 和必需的 `Location`；进入后台执行返回 `202`、必需的 `Location` 及
  `BackgroundJob`；
- `complete-upload` 是文件状态机例外：返回 `202 FileAsset(status=SCANNING)`，`Location`
  指向 `GET /files/{fileId}` 供轮询，不创建 `BackgroundJob`；
- 正式动作返回 `FormalActionResult`，固定包含新状态、生成单据或版本、下游任务、下一责任和
  `nextActions`，不能只返回 toast 文案或空响应。

`FormalActionResult.nextActions` 是任务接力契约，落实信息架构第 3 章。每项固定包含：

| 字段 | 语义 |
| --- | --- |
| `actionCode` | 稳定动作码，前端据此选择图标与文案变体，不解析业务含义 |
| `label` | 服务端下发的具体业务动词，例如"创建采购入库"，不允许"确定"这类无义文案 |
| `href` | 目标页面的稳定路由，含预填 query；不得指向阶段名或非稳定身份 |
| `prefill` | 目标页面创建层的预填内容，仅含调用者有权查看的字段 |
| `enabled` | 当前用户此刻是否可执行 |
| `blocker` | `enabled=false` 时的稳定阻断码与说明 |
| `responsibility` | 下一责任部门与责任人；责任人不是当前用户时仍返回，前端禁用而不隐藏 |

`nextActions` 由服务端按阶段、主责、状态、权限、数据范围和岗位分离计算，前端不得自行推导
后续步骤。空数组表示该动作确实没有后继，前端展示"等待<责任部门>处理"，不展示空白区域。

### 3.5 状态、动作和阻断

销售单状态必须分别返回：

- `commercialStatus`；
- `reviewStatus`；
- `fulfillmentProgress`；
- `collectionProgress`；
- `invoiceProgress`；
- `closeStatus`；
- 适用时返回 `mallCollaborationStatus` 和 `exceptionStatus`。

不得把这些轨道压缩成一个通用 `status`。每个可操作详情和列表摘要同时返回：

- `allowedActions`：服务端根据阶段、主责、状态、权限、数据范围、岗位分离和新鲜度计算的
  当前可执行动作；
- `actionBlockers`：针对不可执行动作的稳定阻断码、说明、字段路径和下一责任；
- `nextResponsibilities`：并行责任轨道，不宣称整张单据只有一个全局下一动作。

前端只能使用上述结果控制入口。前端可以做即时格式校验，但不得复制状态机或自行推导
“按钮应该可用”。服务端仍需在命令事务中重新校验全部条件。

### 3.6 权限、403 与 404

- `401`：未认证或凭证失效；
- `403 PERMISSION_DENIED`：缺页面或动作权限；
- `403 DATA_SCOPE_DENIED`：对象存在，但当前用户不在可见或可操作数据范围；
- `403 FIELD_ACCESS_DENIED`：尝试查看完整敏感字段或下载无权文件；
- `404 OBJECT_NOT_FOUND`：稳定 ID 在当前业务对象类型中确实不存在；
- 列表和搜索只统计调用者有权看到的对象，不返回范围外数量或命中摘要；
- 有页面权限但无数据权限时必须返回 `403`，不得伪装为 `404`；
- 详情深链、关联对象、文件查看与每次下载都重新校验当前权限和数据范围。

### 3.7 ETag 与 If-Match

- 可编辑主数据、工作副本和业务详情 `GET` 返回强 `ETag`；
- 更新主数据、保存工作副本，以及依赖当前聚合版本的正式命令必须发送 `If-Match`；
- `If-Match` 必须原样使用下表指定 GET 的聚合 ETag，不能由客户端根据字段计算；
- 不匹配返回 `412 ETAG_MISMATCH`，并要求重新加载或比较；
- 工作副本保存还必须提交服务端 `draftVersion` 和 `contentHash`；
- 正式提交还必须提交不可变审批对象的 `subjectHash`；
- 列表中的 `lockVersion` 只用于展示和批量预览冻结，不替代详情 ETag。

| 写操作 | `If-Match` 来源 |
| --- | --- |
| 任务 `claim`、租约、转交和关闭 | `GET /tasks/{workItemId}` |
| 销售草稿保存、提交、采购确认 | `GET /sales-orders/{salesOrderId}`；不得用草稿 hash 或列表版本替代 |
| 后台任务取消和续跑 | `GET /jobs/{jobId}` |
| 批量确认 | `GET /bulk-previews/{selectionSnapshotId}` |
| 文件完成上传和创建下载授权 | `GET /files/{fileId}` |

### 3.8 Idempotency-Key

创建正式事实、提交审批、通过/驳回、过账、冲正、退款、迁移执行、确认批量、创建后台
任务和重试外部动作的 POST 必须发送 `Idempotency-Key`。规则如下：

- ASCII 字符串，长度 `8..128`；
- 作用域为当前主体、HTTP 方法和规范化路径；
- 同一业务意图在超时、断网或人工重放后必须复用原 key；
- 相同 key 与相同请求指纹返回原结果，并设置 `Idempotency-Replayed: true`；
- 相同 key 与不同请求指纹返回 `409 IDEMPOTENCY_KEY_REUSED`；
- 仍在处理时返回 `409 IDEMPOTENT_REQUEST_IN_PROGRESS`，前端轮询原任务或用同 key 重试；
- 客户端不得在收到未知结果后生成新 key 重复正式动作。

所有可能被幂等重放的成功和错误响应都可以返回 `Idempotency-Replayed: true`。缺失或非法
控制头固定返回 `400`。正式 POST 明确声明 `502`、`503`、`504` 恢复分支：`502/504`
结果未知时先按原业务对象或后台任务查询结果，再以原请求体和原 key 恢复；`503` 仅在
`retryable=true` 时按 `Retry-After` 使用原 key 重试。

### 3.9 正式事实和命令

- 正式事实、不可变版本、库存流水、收付款、发票、核销、消费事实和供应商事实不提供
  `DELETE`；
- 禁止 `PATCH /{resource}/{id}` 携带通用 `status` 直接跳状态；
- 草稿和可编辑主数据使用具名 `PUT`，状态变化使用强类型
  `POST /{resource}/{id}/commands/{business-command}`；
- 作废、冲正、退款、红票和调整必须生成相应强类型反向事实并返回 `FormalActionResult`；
- 采购二次确认是销售提交上的命令和处理记录：
  `POST /sales-orders/{salesOrderId}/commands/confirm-procurement`，不是可创建、编辑、删除的
  `procurement-confirmations` 单据资源；
- `GET /sales-orders/{salesOrderId}` 在存在当前待处理提交时返回 `pendingSubmission`：
  `submissionId`、`submissionNo`、`workingCopyVersion`、`subjectHash`、提交时间和不可变行；
  每行返回 `submissionLineId` 及采购确认所需的商品、数量、履约和金额摘要；
- `Task.subjectRef` 固定包含 `type/id/version/hash/href`。销售提交任务的 `id/hash` 必须等于
  `pendingSubmission.submissionId/subjectHash`，`href` 打开统一销售单详情；采购确认请求只能
  使用该详情和已领取任务响应中取得的 ID 与租约，不接受客户端自行构造的提交 ID；
- `POST /sales-orders` 只需 `businessType`，响应 `data` 是可立即保存的初始工作副本，并明确
  返回 `salesOrderId` 与 `workingCopyId`；后续 `PUT /sales-orders/{salesOrderId}/draft` 使用该
  `workingCopyId`，不依赖可能为空的详情 `activeDraft`；
- 销售草稿 input/response 以 `businessType` 为 discriminator。草稿允许空白和半填写：行仅
  强制 `lineType/clientLineKey/lineNo`，`lineId` 可省略或为空，业务输入和派生金额可为空；
  实物与服务草稿只能包含 `0..500` 条 `GOODS_SERVICE` 行，卡券草稿只能包含 `0..1` 条
  `VOUCHER` 行。自动保存不因字段未填写完成而拒绝；`validation.isSubmittable=false` 和字段
  错误用于表达尚不可提交；
- 正式提交重新校验完整性并冻结不可变内容：实物与服务至少一行、卡券恰好一行，所有必填
  业务字段与派生金额完整，且两类专用字段互斥，不允许 mixed lines；`pendingSubmission` 与
  正式 revision 只保存满足这些严格约束的行；
- 财务纠错是客户退款、供应商退款、回款冲正、付款冲正和红票的只读联合投影；
- 业务异常是 `taskType=BUSINESS_EXCEPTION` 的任务投影；
- 两者都不能产生通用 `correction` 或 `businessException` 正式实体。实际变更必须调用投影
  指向的强类型业务命令。

---

## 4. 批量操作、后台任务与文件

### 4.1 批量流程

批量操作统一使用：

```text
预览 → 不可变选择快照 → 用户确认 → 后台任务 → 结果明细
```

1. `POST /bulk-previews` 根据显式 ID 或当前筛选创建预览；
2. 服务端冻结对象 ID、逐项版本、筛选摘要、排序摘要、数据截止水位和权限范围摘要；显式
   选择中的稳定 ID 必须唯一，同一 ID 即使携带不同 `expectedVersion` 也返回校验错误；
3. `POST /bulk-previews/{selectionSnapshotId}/commands/confirm` 使用
   `Idempotency-Key` 和 `If-Match` 确认；
4. 服务端返回 `202 BackgroundJob`；
5. 后台逐项重新校验当前权限、数据范围、状态和版本，预览后新增对象不会自动进入；
6. 结果按成功、跳过、失败分组，正式单据以单据为事务边界。

主责迁移是明确例外：一个客户批次为原子事务，不允许部分成功。失败时该客户批次没有
任何 `ownerSystem` 变化，保持冻结并使用原批次和原幂等键续跑；其他已成功客户批次不回退。

正式审批、采购二次确认、卡券审批、库存调整复核、财务纠错复核和供应商结算复核禁止
批量通过，统一任务队列使用“处理并打开下一条”。

### 4.2 后台任务

后台任务状态固定为 `QUEUED`、`RUNNING`、`PARTIALLY_SUCCEEDED`、`SUCCEEDED`、
`FAILED`、`CANCELLED`。任务返回总数、已处理、成功、跳过、失败、开始时间、完成时间和
最近进展。主责迁移任务不使用 `PARTIALLY_SUCCEEDED`。

取消和续跑必须使用端点声明的命令；续跑保留原选择快照和业务幂等身份，不重新扩大范围。

### 4.3 文件

- 浏览器先创建上传意图，再向短时授权地址上传，最后调用完成上传命令；
- 浏览器可声明的 `UploadPurpose` 仅为 `ATTACHMENT`、`IMPORT`、`EVIDENCE`；
  `EXPORT_RESULT` 和 `VALIDATION_REPORT` 只能由服务端后台任务生成；
- 服务端在文件可用于业务前完成类型、大小、安全扫描和结构校验；
- `complete-upload` 返回 `202`、扫描中的 `FileAsset` 和必需 `Location`，客户端轮询文件
  详情直至 `AVAILABLE` 或 `REJECTED`；这是文件扫描例外，不进入后台任务中心；
- 每次下载创建短时授权，并重新校验当前字段权限、业务对象权限和用途；
- 业务响应只持有 `fileId` 和元数据，不持久化长期 URL；
- 原始 SQL、商城原始导出、含禁止字段的文件不进入普通文件 API 的长期附件范围；
- 列表、快速预览、导出和下载沿用同一遮罩与字段权限；
- 卡号、卡密、绑定手机号、连接密钥正文永不通过文件接口提供。

---

## 5. 浏览器 endpoint 与 command catalog

本节冻结 v1 的资源名、路径和业务命令。页面路由是导航契约，API 路径是领域契约，两者
不能机械相等：一个页面可以组合多个资源，同一资源也可以支撑多个固定页面视图。

OpenAPI 当前 29 个 path 是诚实、可生成客户端的核心规范；本 catalog 覆盖页面地图其余
业务域并冻结后续展开时的资源名和命令名，不为尚未精确定义的 schema 编造响应结构。

表中 `GET collection/detail` 表示 `GET /resource` 与 `GET /resource/{id}`；`create/update`
只适用于草稿或主数据，并仍受 ETag 和权限约束。所有路径均相对 `/api/v1`。

### 5.1 通用上下文与工作台

| 页面/能力 | 查询端点 | 命令端点 | 契约说明 |
| --- | --- | --- | --- |
| 应用上下文 | `GET /context` | 无 | 当前用户、角色、权限摘要、业务时区、阶段功能、切换门禁、导航工作区和命令面板清单 |
| 全局搜索 | `GET /search` | 无 | 只搜索已授权资源；返回稳定对象类型、ID、编号、标题和可打开页面 |
| 我的工作 | `GET /workbench` | 无 | 组合待办、预警、最近对象和后台任务，不注册新业务实体 |
| 我的单据 | `GET /my-documents` | 无 | `ERP-WK-005`；当前用户维度的既有单据只读投影，不注册新实体 |
| 单据视图 | `GET /{resource}/{id}/document-view` | 无 | 纸质投影所需的正式事实快照，见 5.11 |
| 统一任务 | `GET /tasks`、`GET /tasks/{workItemId}` | `claim`、`renew-lease`、`release`、`transfer`、`close` | 领取和租约管理任务；正式处理必须调用强类型领域命令 |
| 统一预警 | `GET /warnings`、`GET /warnings/{warningId}` | `convert-to-task`、`acknowledge` | 预警不直接修改正式业务事实 |
| 后台任务 | `GET /jobs`、`GET /jobs/{jobId}`、`GET /jobs/{jobId}/items` | `cancel`、`resume` | `resume` 沿用原范围和幂等身份 |
| 批量操作 | `POST /bulk-previews`、`GET /bulk-previews/{selectionSnapshotId}` | `confirm` | 预览本身不修改业务数据 |
| 文件 | `POST /files/upload-intents`、`GET /files/{fileId}` | `complete-upload`、`create-download` | 每次下载重新鉴权；短时 URL 不进入业务对象 |
| 保存视图 | collection/detail `/saved-views` | `publish`、`copy`、`disable` | 团队视图不能被个人直接覆盖 |

固定 `taskType`：

`PROCUREMENT_CONFIRMATION`、`LOW_MARGIN_MANAGER_CONFIRMATION`、
`PURCHASE_ORDER_REVIEW`、`CARD_FUNDS_REVIEW`、`CARD_FUNDS_DELTA_REVIEW`、
`CARD_SALES_MANAGER_APPROVAL`、`CARD_SALES_OPERATION_APPROVAL`、
`OWNERSHIP_MIGRATION_SALES_CONFIRMATION`、`OWNERSHIP_MIGRATION_FINANCE_CONFIRMATION`、
`INVENTORY_ADJUSTMENT_REVIEW`、`FINANCE_CORRECTION_REVIEW`、
`SUPPLIER_SETTLEMENT_REVIEW`、`INTEGRATION_RESULT_UNKNOWN`、`BUSINESS_EXCEPTION`。

页面 `/exceptions` 是 `GET /tasks?taskType=BUSINESS_EXCEPTION` 的固定投影；没有
`/business-exceptions` CRUD。

#### 5.1.1 导航与命令面板

`GET /context` 除既有内容外返回 `navigation` 和 `commands`，落实信息架构第 2 章和第 4 章：

- `navigation`：当前用户可见的工作区及其页签。每个工作区返回稳定 `workspaceCode`、标题、
  默认路由和页签数组；每个页签返回页面编号、标题和稳定路由。服务端按角色、权限和阶段计算，
  不返回无权限或未启用的页签。工作区分组是导航呈现，不是权限边界。
- `commands`：命令面板的动作与前往清单。每项返回 `commandCode`、分组（`ACTION` 或
  `NAVIGATE`）、标题、匹配关键词和目标路由。服务端只返回当前用户有权执行的命令，无权限项
  直接不返回，不返回禁用项。`NAVIGATE` 命令必须覆盖全部有权访问的页面，包括当前角色导航中
  未显示的页面。

两者体积小且随权限变化，客户端在登录后一次取得并缓存到会话结束；命令面板在本地模糊匹配，
输入过程不请求服务端。权限或阶段变化时服务端在任意响应的 `meta.warnings` 返回
`CONTEXT_STALE`，客户端重新取得 `GET /context`。

`ACTION` 命令只负责打开目标页面或确认层，不携带副作用，也不绕过 3.5 节的 `allowedActions`
与确认层约束。

#### 5.1.2 我的单据

`GET /my-documents` 是当前用户维度的跨资源只读投影，支撑 `ERP-WK-005`：

- `view` 参数固定为 `DRAFT`、`IN_FLIGHT`、`REJECTED`、`PARTICIPATED`；
- 返回项统一为 `objectType`、稳定 ID、可展示编号、标题、对手方、金额摘要、主状态、
  `primaryBlocker`、`updatedAt` 和 `href`；
- 该端点不注册新业务实体，也不提供任何命令。全部动作在目标单据自身的强类型端点上执行；
- 数据范围与各来源资源一致，不因聚合而放宽。用户看不到的单据不因"我参与过"而可见。

#### 5.1.3 阻塞点

列表摘要在 3.5 节的多轨状态之外增加 `primaryBlocker`，支撑信息架构 §6.3 的阻塞点列：

| 字段 | 语义 |
| --- | --- |
| `blockerCode` | 稳定阻断码，与 `actionBlockers` 使用同一码表 |
| `summary` | 一行阻塞说明 |
| `responsibleParty` | 责任部门与责任人 |
| `dueAt` | 该阻塞的到期时间，可为 `null` |
| `additionalCount` | 同时存在的其他阻塞数量 |

`primaryBlocker` 由服务端从 `actionBlockers` 和 `nextResponsibilities` 计算，取最早到期的一条；
无阻塞时返回 `null`。它是派生展示字段，不替代多轨状态，客户端不得据此推导可执行动作。

### 5.2 销售与客户

| 页面对象 | 查询端点 | 创建/更新 | 强类型命令 |
| --- | --- | --- | --- |
| 客户 | collection/detail `/customers`；`/customers/{customerId}/versions` | `POST /customers`、`PUT /customers/{customerId}` | `enable`、`disable`、`reveal-sensitive-field` |
| 合同 | collection/detail `/contracts`；`/contracts/{contractId}/versions` | `POST /contracts`、`PUT /contracts/{contractId}` | `enable`、`disable` |
| 统一销售单 | collection/detail `/sales-orders`；`/sales-orders/{salesOrderId}/versions`；版本详情与比较 | `POST /sales-orders`、`PUT /sales-orders/{salesOrderId}/draft` | `submit`、`confirm-procurement`、`confirm-low-margin`、`approve-by-sales-manager`、`approve-by-operations`、`void-draft`、`close` |
| 销售变更 | collection/detail `/sales-change-orders`；版本与差异 | `POST /sales-change-orders`、`PUT /sales-change-orders/{changeOrderId}/draft` | `submit`、`confirm-fulfillment-impact`、`review-financial-impact`、`approve`、`reject`、`abandon` |
| 客户验收 | collection/detail `/sales-acceptances` | `POST /sales-acceptances` 仅创建草稿 | `submit-result`、`post`、`reverse` |
| 销售退货与拒收 | collection/detail `/sales-returns` | `POST /sales-returns` 仅创建草稿 | `submit`、`confirm-return-receipt`、`complete`、`cancel-draft` |

`GET /sales-orders?businessType=VOUCHER` 支撑卡券销售单固定视图，仍是同一销售资源。
不得建立 `/voucher-sales-orders`。`businessType` 创建后不可变。

### 5.3 采购与供应商

| 页面对象 | 查询端点 | 创建/更新 | 强类型命令 |
| --- | --- | --- | --- |
| 供应商 | collection/detail `/suppliers`；能力、资质和版本子资源 | `POST /suppliers`、`PUT /suppliers/{supplierId}` | `enable`、`disable`、`reveal-sensitive-field` |
| 可销售项目 | collection/detail `/sellable-items`；版本 | `POST /sellable-items`、`PUT /sellable-items/{sellableItemId}` | `enable`、`disable` |
| 采购单 | collection/detail `/purchase-orders`；版本 | `POST /purchase-orders`、`PUT /purchase-orders/{purchaseOrderId}/draft` | `submit-for-review`、`approve`、`reject`、`void-draft` |
| 采购变更 | collection/detail `/purchase-change-orders` | `POST /purchase-change-orders`、`PUT .../draft` | `submit`、`review-inventory-impact`、`review-financial-impact`、`approve`、`reject` |
| 采购退货 | collection/detail `/purchase-returns` | `POST /purchase-returns` 仅创建草稿 | `submit`、`post-outbound`、`record-refund`、`complete` |
| 统一交付队列 | `GET /fulfillment-queue` | 无 | 只读跨资源投影，见下 |
| 仓发与代发 | `GET /deliveries?deliveryType=WAREHOUSE_SHIP` 或 `GET /deliveries?deliveryType=SUPPLIER_DIRECT`；`GET /deliveries/{deliveryId}` | `POST /deliveries` 仅创建草稿 | `dispatch`、`confirm-delivered`、`reverse` |
| 电子交付 | collection/detail `/electronic-deliveries` | `POST /electronic-deliveries` 仅创建草稿 | `deliver`、`submit-for-acceptance`、`reverse` |
| 服务履约 | collection/detail `/service-fulfillments` | `POST /service-fulfillments` 仅创建草稿 | `complete-service`、`submit-for-acceptance`、`reverse` |
| API 供应商连接 | collection/detail `/supplier-api-connections` | `POST /supplier-api-connections`、`PUT /supplier-api-connections/{connectionId}` | `configure-capabilities`、`test-connection`、`enable`、`disable` |
| 供应商订单 | collection/detail `/supplier-orders`；动作和状态历史子资源 | 无通用更新 | `query-original-result`、`cancel`、`refund`、`transfer-to-manual`、`retry-original-action` |

供应商连接接口只返回密钥管理引用和脱敏健康摘要；供应商协议、签名和真实端点属于
`supplier-connector-v1.openapi.yaml`，不暴露给浏览器。

`GET /fulfillment-queue` 支撑 `ERP-PUR-022`，落实交互规范 §9.5 中"三种履约方式共用同一流程"
的既有事实：

- 跨 `deliveries`、`electronic-deliveries`、`service-fulfillments` 和 `purchase-receipts`
  返回统一的待履约投影，每项含 `objectType`、稳定 ID、`fulfillmentMethod`、来源销售单、
  客户、SKU 与数量、要求交期、`primaryBlocker`、`allowedActions` 和 `href`；
- `fulfillmentMethod` 使用既有的 `WAREHOUSE_SHIP`、`SUPPLIER_DIRECT`、`ELECTRONIC`、
  `SERVICE` 和 `PURCHASE_RECEIPT`，不新增履约方式；
- 该端点只读，不提供任何命令。履约动作仍调用各自资源上的强类型命令，先款后货门禁、
  预占校验和事务边界不因聚合入口而改变；
- 不建立 `fulfillment_queue` 正式实体，也不产生新的稳定业务身份。

### 5.4 商品与供应

| 页面对象 | 查询端点 | 创建/更新 | 强类型命令 |
| --- | --- | --- | --- |
| 商品与 SKU | collection/detail `/products`；`/products/{productId}/skus`；版本 | `POST /products`、`PUT /products/{productId}`、`POST /products/{productId}/skus`、`PUT /skus/{skuId}` | `enable`、`disable` |
| 卡券类目 | `GET /skus?productKind=VOUCHER`、`GET /skus/{skuId}`；类目修订 | 使用商品和 SKU 主数据更新 | `enable`、`disable` |
| 外部商品同步 | collection/detail `/catalog-sync-jobs`；明细 | 无普通更新 | `start`、`resume`、`cancel` |
| 外部商品映射 | collection/detail `/external-product-mappings` | 无通用更新 | `link-existing-sku`、`create-and-link-sku`、`confirm-critical-change`、`reject` |
| 供应商供给 | collection/detail `/supplier-offerings`；版本 | `POST /supplier-offerings`、`PUT /supplier-offerings/{offeringId}` | `confirm-price-change`、`enable`、`disable` |
| 商品发布 | collection/detail `/product-publications`；修订和投递 | `POST /product-publications` 仅创建发布草稿 | `publish`、`pause`、`republish-original-version`、`retry-original-delivery` |

卡券类目是 `productKind=VOUCHER` 的商品/SKU 固定视图，不建立独立卡券类目主表，浏览器
API 不接收玩法规则、卡号、卡密、绑定手机号、生产或激活字段。

### 5.5 库存与履约

| 页面对象 | 查询端点 | 创建/更新 | 强类型命令 |
| --- | --- | --- | --- |
| 仓库 | collection/detail `/warehouses`；版本 | `POST /warehouses`、`PUT /warehouses/{warehouseId}` | `enable`、`disable` |
| 库存余额 | `GET /stock-balances`、`GET /stock-balances/{stockBalanceId}` | 无 | 无；余额由流水投影 |
| 库存预占 | collection/detail `/stock-reservations`；流水子资源 | 无通用更新 | `reserve`、`release`、`consume` |
| 库存流水 | collection/detail `/stock-movements` | 无 | 无；追加式事实不可编辑、删除 |
| 采购入库 | collection/detail `/purchase-receipts` | `POST /purchase-receipts` 仅创建草稿 | `post`、`reverse` |
| 仓发出库 | `GET /deliveries?deliveryType=WAREHOUSE_SHIP`、detail `/deliveries` | 见交付聚合 | `dispatch`、`confirm-delivered`、`reverse` |
| 库存调整 | collection/detail `/stock-adjustments` | `POST /stock-adjustments`、`PUT /stock-adjustments/{adjustmentId}/draft` | `submit`、`review-by-warehouse`、`review-by-finance`、`execute`、`reject` |

库存余额是查询模型，库存流水是追加式事实。任何端点都不得以“校正余额”为名直接覆盖余额。

### 5.6 财务结算

| 页面对象 | 查询端点 | 创建/更新 | 强类型命令 |
| --- | --- | --- | --- |
| 客户应收 | collection/detail `/receivables`；分录和余额子资源 | 无 | `review-card-opening-balance`、`review-card-amount-delta` |
| 客户回款 | collection/detail `/customer-receipts`；核销子资源 | `POST /customer-receipts` 仅创建草稿 | `post`、`allocate`、`reverse-allocation`、`create-refund`、`reverse-receipt` |
| 销项发票 | `GET /invoices?direction=SALES`、detail `/invoices`；核销子资源 | `POST /invoices` 仅创建草稿 | `post`、`allocate-sales`、`reverse-allocation`、`issue-red-invoice` |
| 供应商应付 | collection/detail `/payables`；分录和余额子资源 | 无 | 无；由采购或结算事实派生 |
| 供应商付款 | collection/detail `/supplier-payments`；核销子资源 | `POST /supplier-payments` 仅创建草稿 | `post`、`allocate`、`reverse-allocation`、`create-refund`、`reverse-payment` |
| 进项发票 | `GET /invoices?direction=PURCHASE`、detail `/invoices`；核销子资源 | `POST /invoices` 仅创建草稿 | `post`、`allocate-purchase`、`reverse-allocation`、`issue-red-invoice` |
| 多对多核销 | `GET /allocation-workspaces/{sourceType}/{sourceId}` | 无通用 CRUD | 调用上表来源对象上的强类型 `allocate` 或 `reverse-allocation` |
| 成本费用 | collection/detail `/cost-entries`；分配子资源 | `POST /cost-entries` 仅创建草稿 | `post`、`allocate`、`reverse` |
| 财务纠错 | `GET /finance-corrections`、`GET /finance-corrections/{documentType}/{businessDocumentId}` | 无 | 跳转并调用 `customer-refunds`、`supplier-refunds`、回款/付款冲正或红票的强类型命令 |
| 供应商结算 | collection/detail `/supplier-settlements`；差异子资源 | `POST /supplier-settlements`、`PUT .../draft` | `submit`、`confirm-difference`、`approve`、`reject`、`form-payable` |

`finance-corrections` 是强类型事实的联合读模型，不提供 `POST`、`PUT`、`PATCH`、`DELETE`。

### 5.7 卡券销售与商城协同的浏览器投影

| 页面对象 | 查询端点 | 浏览器命令 | 外部边界 |
| --- | --- | --- | --- |
| 商城同步任务 | collection/detail `/mall-sales-sync-jobs`；水位和明细 | `start-baseline`、`resume-from-watermark`、`reconcile-all`、`pull-by-order-no` | 实际读取商城接口属于 `mall-phase1-read-v1.openapi.yaml` |
| 商城同步差异 | collection/detail `/mall-sales-sync-differences` | `assign`、`confirm-mapping`、`replay-original-snapshot`、`close-with-evidence` | 不直接改商城商业事实 |
| 商城每日核对 | collection/detail `/mall-sales-reconciliations`；差异 | `start`、`resume`、`pull-difference` | 使用完整内容指纹，不只比状态或金额 |
| 销售单执行投影 | collection/detail `/sales-order-projections`；修订和投递 | `retry-original-version`、`transfer-to-error-center` | 下发协议属于商城协同外部契约 |
| 主责迁移 | collection/detail `/ownership-migration-batches`；候选 `/ownership-migration-candidates` | `create-customer-batch`、`confirm-sales-checklist`、`confirm-finance-checklist`、`confirm-final-baseline`、`execute`、`resume-original-batch`、`open-cutover` | 每客户原子；开放切换是唯一商城级动作 |
| 商城消费订单 | collection/detail `/mall-orders`；支付来源、退款和供应商履约子资源 | 无通用状态命令 | 页面读取 ERP 已接收事实，不直连商城订单 API |
| 卡券消费 | collection/detail `/mall-consumptions`；成本评估、余额快照和归集历史 | `assign-attribution`、`confirm-cost-basis` | 不修改原消费事实 |
| 历史消费回填 | collection/detail `/mall-consumption-backfill-jobs`；明细 | `start-until-cutover`、`resume` | 截止唯一切换时间 `T`，只回填台账，不触发供应商下单 |

上述页面 API 仍是浏览器契约，因为它们读取和操作 ERP 内部任务/投影。ERP 与商城之间的
同步请求、事件和执行投递不属于本文件。

### 5.8 异常、接口治理和对账

| 页面对象 | 查询端点 | 强类型命令 | 说明 |
| --- | --- | --- | --- |
| 业务异常 | `GET /tasks?taskType=BUSINESS_EXCEPTION`、任务详情 | 任务 `claim/transfer/close`；实际处理调用原业务域变更、退货、退款、冲正或调整命令 | 不建立通用异常正式实体 |
| 接口错误 | collection/detail `/integration-error-tasks`；尝试历史 | `query-original-result`、`retry-original-request`、`assign`、`transfer-to-manual`、`record-compensation`、`close-with-evidence` | 重试使用原幂等键 |
| 接口对账 | collection/detail `/reconciliation-jobs`；差异 `/reconciliation-differences` | `start`、`rerun-query-model`、`create-resolution-task`、`confirm-difference` | 不直接覆盖任一侧正式事实 |

### 5.9 经营分析

| 页面 | 查询端点 | 约束 |
| --- | --- | --- |
| 客户经营质量 | `GET /analytics/customer-quality`、`GET /analytics/customer-quality/{customerId}` | 返回口径、期间、数据水位和授权下钻 |
| 实际经营结果 | `GET /analytics/operating-results` | 履约期限前的卡券结果不得命名为最终利润 |
| 履约与票款 | `GET /analytics/fulfillment-funds` | 多轴状态独立返回 |
| 卡券经营 | `GET /analytics/card-operations` | 展示消费率、成本覆盖率、未消费余额和结果口径 |
| 供应商履约质量 | `GET /analytics/supplier-fulfillment` | 只下钻授权供应商订单 |
| 接口运行质量 | `GET /analytics/integration-quality` | 只下钻授权错误和对账对象 |

分析 API 只返回查询模型；所有下钻继续按目标业务对象权限校验。查询模型过期时通过
`meta.dataAsOf`、warning 或 `QUERY_MODEL_STALE` 明示，不能以旧数据执行依赖新鲜度的命令。

### 5.10 系统管理

| 页面对象 | 查询端点 | 命令端点 | 约束 |
| --- | --- | --- | --- |
| 角色与权限 | `/access-control/roles`、`/access-control/permissions`、`/access-control/data-scopes` | `grant`、`revoke`、`submit-segregation-review`、`approve-segregation-review` | 管理员不能给自己授予新业务权限或单独降低岗位分离 |
| 操作审计 | `GET /audit-events`、`GET /audit-events/{auditEventId}` | 无 | 只读、追加式；敏感值和密钥不进入差异正文 |
| 数据导入 | collection/detail `/imports`；校验与结果明细 | `create-upload`、`validate`、`confirm`、`resume` | 固定预校验流程，确认后进入后台任务 |
| 期初迁移 | collection/detail `/initial-migrations`；验证报告 | `validate-in-staging`、`confirm-business-checklist`、`execute-production`、`resume` | 原始 SQL 不进入长期存储或下载 |
| 接口监控 | `GET /integration-health`、`GET /integration-health/{connectionId}` | `run-diagnostic`、`create-error-task` | 只返回脱敏摘要，不返回密钥或完整报文 |
| 主责迁移 | 见 5.7 | 见 5.7 | 复用稳定销售单，不复制单号和版本 |

### 5.11 单据视图投影

`GET /{resource}/{id}/document-view` 返回统一的 `DocumentView`，支撑信息架构第 5 章的单据视图、
对比视图和审批处理台左栏。支持该端点的资源固定为：

`sales-orders`、`sales-change-orders`、`purchase-orders`、`purchase-receipts`、`deliveries`、
`invoices`、`customer-receipts`、`supplier-payments`、`supplier-settlements`。

`DocumentView` 固定结构：

| 字段 | 语义 |
| --- | --- |
| `issuer` | 出具方展示名称；仅用于抬头，不推导签约主体 |
| `title`、`subtitle` | 单据名称与业务性质说明 |
| `documentNo`、`revision` | 可展示编号与版本，均不作为路由主键 |
| `status` | 主状态的展示标签与语义色调 |
| `parties` | 固定两方，各含角色标签、名称、编号引用和字段数组 |
| `metadata` | 单据级信息字段数组，例如合同、负责人、履约期限、付款条件 |
| `lines` | 明细行数组，含服务端已按分舍入的金额 |
| `columns` | 明细列定义，含标题、对齐和数值标记；按业务性质裁剪 |
| `totals` | 汇总项数组，含标签、值、口径说明和强调标记 |
| `remarks` | 备注正文 |
| `signature`、`seal` | 签署与用章的展示内容 |

约束：

- `DocumentView` 是只读投影，不提供 `POST`、`PUT`、`PATCH`、`DELETE`，也不携带 `allowedActions`；
  正式动作仍从详情端点取得；
- 金额、税额、数量、汇总和大小写金额全部由服务端计算并按 3.1 节格式返回；客户端只排版，
  不重建任何正式口径；
- 敏感字段沿用 7 节的遮罩规则。银行账号、联系人手机号和税号在 `parties` 中默认遮罩，
  完整值仍只能通过独立 reveal 命令取得，不因进入打印投影而放宽；
- 版本对比时对同一资源的两个 `revision` 分别请求，服务端不提供合并差异的投影；差异高亮由
  客户端按 `lines` 的稳定明细身份比对，不改变任一侧事实；
- 卡券销售单的 `lines` 恰好一条，且不包含玩法规则、卡号、卡密、绑定手机号和激活字段。

---

## 6. 阶段、ownerSystem 与单写边界

### 6.1 第一期

- `businessType=VOUCHER` 且 `ownerSystem=MALL` 的商业字段由商城唯一写入；
- ERP 只通过独立商城只读接口形成不可变快照和统一销售版本；
- 浏览器销售单 API 对这些商业字段只读，不开放创建卡券单、销售变更、直接作废或手工推进
  商城来源状态；
- ERP 仍独立写入自己的应收、回款、发票、核销、财务复核、同步差异和审计；
- 商城状态不能覆盖 ERP 的回款、开票、履约期限和关闭口径。

### 6.2 第二期准备、迁移和开放

- P0 与 P1 必须同一生产发布；退款、余额恢复、结算、对账和人工补偿未就绪时不得切换；
- 主责迁移只对同一 `salesOrderId` 做一次 `ownerSystem: MALL -> ERP`，不复制销售单、单号、
  应收、回款、发票或版本；
- 迁移按客户批次原子执行，失败批次保持冻结并以原批次续跑；
- 全部客户批次成功、P0/P1 门禁通过、一期轮询停止后，商城级 `open-cutover` 命令登记唯一
  时间 `T`；
- 不提供 `ERP -> MALL` owner 回退命令，也不恢复一期轮询；
- 只有切换完成后 `context.features.voucherSalesCreationEnabled=true`，浏览器才可能在
  `allowedActions` 中得到 `CREATE_VOUCHER_SALES_ORDER`；
- 只有支付成功时间不早于 `T` 且已形成 ERP 供应商订单的业务，才能驱动供应商取消或退款。

### 6.3 服务端与前端责任

服务端必须在每次命令中重新验证阶段、切换门禁、`ownerSystem`、状态、任务租约、权限、
数据范围、岗位分离、ETag、内容指纹和业务新鲜度。前端只根据服务端 `context`、
`allowedActions` 和 `actionBlockers` 表达入口，不能把隐藏按钮当作安全控制。

---

## 7. 敏感信息和日志

- 银行账号、联系人手机号、邮箱、税号、证照和收货信息默认遮罩；完整查看使用单独的
  审计命令并受字段权限控制；
- 普通 DTO 的敏感文本只能使用 `MaskedSensitiveText`，其中 `masked` 固定为 `true`，
  `displayValue` 只允许遮罩文本；完整值只能由独立、短时、审计且 `Cache-Control: no-store`
  的 reveal 响应提供，不能混入普通详情；
- 列表、快速预览、详情、导出和文件下载分别校验字段权限，导出权限不继承页面查看权限；
- 稳定卡实例引用必须不可反推卡号或卡密；
- 卡号、卡密、绑定手机号、玩法个人执行数据和连接密钥正文不属于浏览器 DTO；
- 外部原始报文、对象存储地址、签名 URL、HMAC、key version 和加密密文不进入普通业务响应；
- 错误 envelope 的 `details` 只能包含白名单业务上下文，不得回显请求体、Token、密钥、
  完整联系方式或数据库错误；
- 下载、完整字段查看、导出和权限变更必须写审计事件。

---

## 8. 客户端生成和兼容性检查

前后端合并前至少执行：

1. 使用 OpenAPI 3.1 解析器校验 YAML；
2. 从 `erp-client-v1.openapi.yaml` 生成 TypeScript 类型和请求客户端；
3. 检查所有 operationId 唯一且稳定；
4. 检查所有错误码存在于 `error-catalog.md`；
5. 检查正式命令均声明 `Idempotency-Key`，依赖当前版本的命令声明 `If-Match`；
6. 检查正式事实没有 `DELETE`，不存在通用 `PATCH status`；
7. 检查 OpenAPI 不包含商城来源接口、商城事件入口或 Supplier Connector 协议；
8. 检查列表分页、十进制字符串、RFC 3339、opaque ID 和 envelope 没有被局部端点改写。

README catalog 冻结尚未展开进 OpenAPI 的资源名与命令名。新增具体 schema 时只能细化这些
已登记端点；若需要改变名称、语义或副作用，必须先按第 1 节更新事实依据和版本决策。
