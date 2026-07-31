# ERP Browser API v1 错误目录

本文冻结 `/api/v1` 的错误 envelope、稳定错误码、HTTP 映射、`retryable` 和前端动作。
可生成类型的枚举同时定义在 [erp-client-v1.openapi.yaml](erp-client-v1.openapi.yaml)。

错误码表达机器可处理的失败类别；中文 `message` 用于当前操作说明，不参与程序分支。前端
不得根据 HTTP 文案、数据库错误或供应商原始错误猜测业务动作。

---

## 1. 错误 envelope

所有非 2xx JSON 响应使用：

```json
{
  "error": {
    "code": "ETAG_MISMATCH",
    "category": "CONCURRENCY",
    "message": "销售单已被其他操作更新，请重新加载后比较差异。",
    "fieldErrors": [],
    "blockers": [],
    "retryable": false,
    "frontendAction": "RELOAD_AND_COMPARE",
    "requestId": "req_opaque",
    "details": [
      { "key": "objectType", "value": "SALES_ORDER" }
    ]
  }
}
```

字段规则：

| 字段 | 规则 |
| --- | --- |
| `code` | 本文登记的稳定大写错误码；不得返回供应商、数据库或框架异常类名 |
| `category` | 固定分类：`REQUEST`、`AUTHENTICATION`、`AUTHORIZATION`、`VALIDATION`、`BUSINESS_BLOCKED`、`CONCURRENCY`、`IDEMPOTENCY`、`MAINTENANCE`、`SYNC_EXTERNAL`、`QUERY_MODEL`、`SYSTEM` |
| `message` | 可直接展示的当前操作说明；不得包含密钥、Token、卡号、卡密、完整联系方式、SQL 或堆栈 |
| `fieldErrors` | 字段错误数组；每项含 `fieldPath`、稳定 `code` 和 `message`；非字段错误返回空数组 |
| `blockers` | 业务阻断数组；每项含错误码、动作码、说明和可选下一责任；没有则为空数组 |
| `retryable` | 只有严格按照 `frontendAction` 可以安全重试时才为 `true`；不表示可盲目自动重放 POST |
| `frontendAction` | 本文第 3 节的稳定动作枚举 |
| `requestId` | 与响应头 `X-Request-Id` 相同，用于审计和排障 |
| `details` | 白名单字符串键值；不能回显请求体、原始报文、加密值、对象存储 URL 或内部异常 |

字段校验失败时，顶层优先返回 `VALIDATION_FAILED`，具体字段使用 `FIELD_REQUIRED`、
`INVALID_FORMAT`、`DECIMAL_SCALE_EXCEEDED` 等 `fieldErrors[].code`。当请求整体只有一个明确
错误时，也允许该具体码作为顶层 `code`，其 HTTP、`retryable` 和前端动作仍按本文执行。

幂等重放可以重放原成功或原错误响应；两者都设置响应头
`Idempotency-Replayed: true`。因此所有共享错误响应声明该可选头，客户端不能只在 2xx
读取它。

---

## 2. HTTP 状态映射

| HTTP | 使用范围 |
| --- | --- |
| `400` | JSON/查询格式错误，缺 `Idempotency-Key` 等控制头，控制头格式不合法 |
| `401` | 未认证、Token 失效或过期 |
| `403` | 页面、动作、数据范围或敏感字段权限不足 |
| `404` | 稳定 ID 对应的业务对象确实不存在；不得掩盖已知的数据范围拒绝 |
| `409` | 业务状态、任务租约、草稿版本、内容指纹、映射或幂等冲突 |
| `410` | 不可变选择快照或短时下载授权已过期 |
| `412` | `If-Match` 与当前强 ETag 不匹配 |
| `413` | 文件超过允许大小 |
| `415` | HTTP 媒体类型或业务文件类型不允许 |
| `422` | JSON 结构合法，但请求字段、跨字段格式或守恒校验失败；正式业务不变量固定使用 `409` |
| `423` | 对象因维护窗口或主责迁移被冻结 |
| `429` | ERP 浏览器 API 或明确外部依赖限流；必须返回 `Retry-After` |
| `500` | 未预期的 ERP 内部错误 |
| `502` | 已调用的外部依赖返回不可接受结果、鉴权失败或结果未知 |
| `503` | ERP、查询模型或必要外部依赖暂时不可用；可返回 `Retry-After` |
| `504` | 网关或必要依赖超时，正式命令必须先确认原结果 |

权限语义固定：对象存在但当前用户没有数据范围时返回 `403 DATA_SCOPE_DENIED`；只有真实
不存在时返回 `404 OBJECT_NOT_FOUND`。列表和搜索只统计授权范围，不能通过总数泄漏范围外对象。

---

## 3. 前端动作

| `frontendAction` | 前端行为 |
| --- | --- |
| `NONE` | 保持当前页面并展示结果；不发起自动请求 |
| `FIX_INPUT` | 定位 `fieldErrors`，保留输入并等待用户修正 |
| `LOGIN` | 清理失效会话并进入登录流程，登录后恢复安全的读请求 |
| `REQUEST_ACCESS` | 展示缺少的页面、动作、数据范围或字段权限，不伪装为空数据 |
| `RELOAD` | 重新读取详情和服务端动作；不保留将覆盖正式事实的旧写入 |
| `RELOAD_AND_COMPARE` | 保留本地草稿，重新读取 ETag/版本并打开差异比较 |
| `RECLAIM_TASK` | 保留用户输入，重新领取任务并取得新租约后再提交 |
| `RETRY` | 按 `Retry-After` 或退避重试安全读请求；写请求仍遵守幂等规则 |
| `RETRY_SAME_IDEMPOTENCY_KEY` | 使用原请求体和原 `Idempotency-Key` 重试，不生成新 key |
| `POLL_RESULT` | 查询原后台任务、原业务对象或原外部动作结果，不直接重复创建 |
| `WAIT` | 展示维护、冻结、资源繁忙或限流原因，按 `Retry-After` 等待 |
| `REFRESH_QUERY` | 刷新或等待查询模型水位；依赖新鲜度的动作保持禁用 |
| `OPEN_TASK` | 打开服务端返回的强类型待办或业务处理入口 |
| `OPEN_ERROR_CENTER` | 打开接口错误任务，执行查询原结果、原 key 重试或人工补偿 |
| `RESELECT_SCOPE` | 清除旧选择，按当前筛选重新预览并生成新选择快照 |
| `UPLOAD_AGAIN` | 保留业务表单，重新创建上传意图并上传合规文件 |
| `NARROW_FILTERS` | 保留当前筛选，提示缩小期间、主体或对象范围 |
| `CONTACT_ADMIN` | 展示 requestId 和脱敏原因，联系系统管理员或研发运维 |

---

## 4. 稳定错误码

### 4.1 请求契约 `REQUEST`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `REQUEST_MALFORMED` | 400 | `false` | `FIX_INPUT` | JSON 无法解析、body 与 schema 根类型不符或请求语法损坏 |
| `QUERY_PARAMETER_INVALID` | 400 | `false` | `FIX_INPUT` | 未声明的排序字段、非法分页、枚举或日期范围查询参数 |
| `HEADER_REQUIRED` | 400 | `false` | `FIX_INPUT` | 端点要求的 `Idempotency-Key`、`If-Match` 或其他控制头缺失 |
| `UNSUPPORTED_MEDIA_TYPE` | 415 | `false` | `FIX_INPUT` | 请求 `Content-Type` 不属于端点声明类型 |

### 4.2 认证 `AUTHENTICATION`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `AUTHENTICATION_REQUIRED` | 401 | `false` | `LOGIN` | 未提供 Bearer 凭证或凭证不可识别 |
| `AUTHENTICATION_EXPIRED` | 401 | `false` | `LOGIN` | 当前凭证已过期或会话已失效 |

### 4.3 权限与对象 `AUTHORIZATION`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `PERMISSION_DENIED` | 403 | `false` | `REQUEST_ACCESS` | 缺页面或强类型动作权限 |
| `DATA_SCOPE_DENIED` | 403 | `false` | `REQUEST_ACCESS` | 对象存在，但不在当前客户、团队、供应商或责任域范围 |
| `FIELD_ACCESS_DENIED` | 403 | `false` | `REQUEST_ACCESS` | 查看完整敏感字段、附件、导出或下载权限不足 |
| `OBJECT_NOT_FOUND` | 404 | `false` | `NONE` | 稳定 ID 在端点对应对象类型中真实不存在 |

### 4.4 字段与业务数据校验 `VALIDATION`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `VALIDATION_FAILED` | 422 | `false` | `FIX_INPUT` | 一个或多个字段、跨字段或行级校验失败；细节在 `fieldErrors` |
| `FIELD_REQUIRED` | 422 | `false` | `FIX_INPUT` | 必填字段缺失、空字符串或必选关联对象为空 |
| `INVALID_FORMAT` | 422 | `false` | `FIX_INPUT` | 日期、编号、字符串格式或字段类型不符合契约 |
| `DECIMAL_SCALE_EXCEEDED` | 422 | `false` | `FIX_INPUT` | 金额、单价、数量或比率超过固定小数精度 |
| `DATE_RANGE_INVALID` | 422 | `false` | `FIX_INPUT` | 起止时间倒置、期限早于允许时间或自然日边界非法 |
| `MONEY_NOT_BALANCED` | 422 | `false` | `FIX_INPUT` | 行金额、税额、表头汇总、核销或分摊不守恒 |
| `VOUCHER_LINE_COUNT_INVALID` | 422 | `false` | `FIX_INPUT` | 卡券销售提交不是恰好一条稳定卡券明细 |
| `FILE_TOO_LARGE` | 413 | `false` | `UPLOAD_AGAIN` | 声明或实际上传大小超过该文件用途上限 |
| `FILE_TYPE_NOT_ALLOWED` | 415 | `false` | `UPLOAD_AGAIN` | 扩展名、MIME 或文件签名不在该用途白名单 |
| `FILE_SECURITY_SCAN_FAILED` | 422 | `false` | `UPLOAD_AGAIN` | 安全扫描拒绝文件；不返回恶意内容正文 |
| `FILE_NOT_READY` | 409 | `true` | `WAIT` | 文件仍在上传、扫描或业务校验，尚不能关联或下载 |

### 4.5 业务阻断 `BUSINESS_BLOCKED`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `ACTION_NOT_ALLOWED` | 409 | `false` | `RELOAD` | 当前阶段、主责、状态或角色没有该强类型动作 |
| `STATE_TRANSITION_NOT_ALLOWED` | 409 | `false` | `RELOAD` | 命令不能从当前固定状态迁移到目标状态 |
| `BUSINESS_INVARIANT_VIOLATION` | 409 | `false` | `OPEN_TASK` | 正式命令违反不可拆分数量、金额上限、唯一事实或其他业务断言 |
| `DEPENDENCY_NOT_READY` | 409 | `false` | `OPEN_TASK` | 资质、映射、付款门槛、前置审批或下游事实未就绪 |
| `SUBJECT_HASH_MISMATCH` | 409 | `false` | `RELOAD_AND_COMPARE` | 任务或审批针对的不可变内容指纹与提交内容不一致 |
| `SEGREGATION_OF_DUTIES_VIOLATION` | 409 | `false` | `REQUEST_ACCESS` | 经办、复核、授权或管理员自授权违反岗位分离 |
| `DATA_FRESHNESS_REQUIRED` | 409 | `true` | `REFRESH_QUERY` | 依赖库存、票款、同步或查询模型的数据超过动作新鲜度阈值 |
| `OWNER_SYSTEM_READ_ONLY` | 409 | `false` | `NONE` | 第一期商城主责卡券单尝试从 ERP 修改商业字段或来源状态 |
| `VOUCHER_CUTOVER_NOT_OPEN` | 409 | `false` | `NONE` | 全部迁移、P0/P1 门禁、轮询停止和唯一 `T` 尚未完成，禁止 ERP 新建卡券单 |
| `TASK_LEASE_REQUIRED` | 409 | `false` | `RECLAIM_TASK` | 正式处理要求任务租约，但请求未提供有效领取令牌 |
| `TASK_LEASE_LOST` | 409 | `true` | `RECLAIM_TASK` | 租约超时、已释放、已转交或已被后继任务替代 |
| `TASK_NOT_CLOSABLE` | 409 | `false` | `NONE` | 审批、确认、结果未知或未完成补偿任务不能人工关闭 |
| `JOB_NOT_CANCELLABLE` | 409 | `false` | `NONE` | 任务已完成、进入不可取消提交区或本类型不允许取消 |
| `SELECTION_SCOPE_CHANGED` | 409 | `false` | `RESELECT_SCOPE` | 确认数量、水位、筛选摘要或逐项版本与预览不一致 |
| `DUPLICATE_EXTERNAL_IDENTITY` | 409 | `false` | `OPEN_ERROR_CENTER` | 同一来源系统、对象类型和外部 ID 映射到多个稳定对象 |
| `MAPPING_REQUIRED` | 409 | `false` | `OPEN_TASK` | 客户、合同、结算主体、卡券类目、SKU 或唯一明细映射未完成 |

### 4.6 并发 `CONCURRENCY`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `ETAG_MISMATCH` | 412 | `false` | `RELOAD_AND_COMPARE` | `If-Match` 不是当前强 ETag；响应返回当前 ETag |
| `DRAFT_VERSION_CONFLICT` | 409 | `false` | `RELOAD_AND_COMPARE` | 工作副本 `draftVersion` 已被其他保存推进 |
| `CONTENT_HASH_CONFLICT` | 409 | `false` | `RELOAD_AND_COMPARE` | 客户端提交的草稿或审批内容指纹与服务端当前内容不一致 |
| `RESOURCE_BUSY` | 409 | `true` | `WAIT` | 短事务竞争或受控资源正在处理；可按 `Retry-After` 重试 |

### 4.7 幂等 `IDEMPOTENCY`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `IDEMPOTENCY_KEY_REQUIRED` | 400 | `false` | `FIX_INPUT` | 正式命令、批量确认或后台任务创建缺少 `Idempotency-Key` |
| `IDEMPOTENCY_KEY_INVALID` | 400 | `false` | `FIX_INPUT` | key 长度、字符或作用域不符合 `/api/v1` 契约 |
| `IDEMPOTENCY_KEY_REUSED` | 409 | `false` | `NONE` | 同一主体、方法和规范化路径下，同 key 对应不同请求指纹 |
| `IDEMPOTENT_REQUEST_IN_PROGRESS` | 409 | `true` | `POLL_RESULT` | 原 key 的请求仍在处理；查询原任务或以原 key 重试 |

相同 key、相同请求指纹且原请求已完成时不返回错误，而是重放原成功或失败响应，并设置
`Idempotency-Replayed: true`。

### 4.8 过期资源

| code | category | HTTP | retryable | frontendAction | 使用条件 |
| --- | --- | ---: | --- | --- | --- |
| `SELECTION_SNAPSHOT_EXPIRED` | `BUSINESS_BLOCKED` | 410 | `false` | `RESELECT_SCOPE` | 批量选择快照超过有效期，必须按当前权限和版本重新预览 |
| `DOWNLOAD_GRANT_EXPIRED` | `AUTHORIZATION` | 410 | `true` | `RETRY` | 短时下载授权过期；重新鉴权并创建新下载授权 |

### 4.9 维护和冻结 `MAINTENANCE`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `MAINTENANCE_WINDOW_ACTIVE` | 503 | `true` | `WAIT` | 当前命令在系统维护窗口内暂停；响应可给 `Retry-After` |
| `OBJECT_FROZEN_FOR_MIGRATION` | 423 | `true` | `WAIT` | 对象处于主责迁移客户批次冻结，不能并发修改商业字段 |

### 4.10 同步与外部依赖 `SYNC_EXTERNAL`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `EXTERNAL_DEPENDENCY_UNAVAILABLE` | 503 | `true` | `RETRY_SAME_IDEMPOTENCY_KEY` | 商城或 Connector 临时不可用，且原动作结果已确认未受理 |
| `EXTERNAL_RATE_LIMITED` | 429 | `true` | `WAIT` | 外部依赖明确限流；必须遵守 `Retry-After` |
| `EXTERNAL_AUTHENTICATION_FAILED` | 502 | `false` | `CONTACT_ADMIN` | 外部鉴权或签名失败；停止自动重试并告警 |
| `EXTERNAL_RESULT_UNKNOWN` | 502 | `false` | `OPEN_ERROR_CENTER` | 超时或断连后无法确认原动作结果；先查询原结果，不直接重下单 |
| `SYNC_WATERMARK_CONFLICT` | 409 | `false` | `OPEN_ERROR_CENTER` | 续跑请求的水位、同刻游标或来源版本不再等于原任务 |
| `SOURCE_DATA_MAPPING_FAILED` | 409 | `false` | `OPEN_TASK` | 来源事实已保存，但映射或规范化失败，不能形成错误应收或经营归属 |

浏览器不接收供应商专用错误码。后端必须把外部错误归一为本节代码，并把脱敏技术摘要留在
接口错误任务，不放入普通业务错误的 `message`。

### 4.11 查询模型 `QUERY_MODEL`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `QUERY_MODEL_NOT_READY` | 503 | `true` | `REFRESH_QUERY` | 首次构建、重建或回填期间尚无可用一致水位 |
| `QUERY_MODEL_STALE` | 503 | `true` | `REFRESH_QUERY` | 查询水位超过页面或动作允许的新鲜度阈值 |
| `SEARCH_INDEX_UNAVAILABLE` | 503 | `true` | `RETRY` | 全局搜索索引暂不可用，不影响按稳定 ID 读取原对象 |
| `REPORT_SCOPE_TOO_LARGE` | 422 | `false` | `NARROW_FILTERS` | 分析、对账或导出范围超过同步查询上限，应缩小范围或转后台任务 |

### 4.12 系统错误 `SYSTEM`

| code | HTTP | retryable | frontendAction | 使用条件 |
| --- | ---: | --- | --- | --- |
| `RATE_LIMITED` | 429 | `true` | `WAIT` | ERP 浏览器 API 限流；必须返回 `Retry-After` |
| `INTERNAL_ERROR` | 500 | `true` | `RETRY` | 未预期内部故障；只返回 requestId，不返回堆栈或数据库信息 |
| `SERVICE_UNAVAILABLE` | 503 | `true` | `RETRY` | ERP 必要服务暂不可用；可返回 `Retry-After` |
| `GATEWAY_TIMEOUT` | 504 | `true` | `POLL_RESULT` | 请求超时；读请求可重试，正式写命令先查询原结果并复用原 key |

---

## 5. 重试规则

1. 前端只有在 `retryable=true` 时才提供重试或轮询入口；仍须执行对应 `frontendAction`。
2. GET 可按指数退避和抖动重试；`429`、`423`、`503` 优先遵守 `Retry-After`。
3. 正式 POST 超时、`500`、`503` 或 `504` 后，必须保留原请求体和原
   `Idempotency-Key`；不得生成新单号意图或新 key。
4. `EXTERNAL_RESULT_UNKNOWN` 不直接重试创建、取消或退款，先执行
   `query-original-result` 或进入接口错误中心。
5. `ETAG_MISMATCH`、草稿版本冲突和内容指纹冲突不自动覆盖；保留输入并比较差异。
6. `TASK_LEASE_LOST` 先重新领取任务，新的租约仍须重新校验对象版本、内容指纹和岗位分离。
7. 批量快照过期或范围变化必须重新预览，不能把旧确认应用到新命中对象。

---

## 6. 服务端映射约束

- 领域层必须显式返回本文错误码，HTTP 适配层只做确定映射；
- 数据库唯一键、外键、序列化和锁异常不得直接暴露，必须转换为业务码或 `INTERNAL_ERROR`；
- 外部商城与 Supplier Connector 错误先标准化，再决定是否自动重试、查询原结果或创建错误任务；
- 一个响应只选最能代表失败结果的顶层 `code`，并用 `fieldErrors` 和 `blockers` 补充多个原因；
- 服务端返回 `allowedActions` 后，到命令执行之间状态仍可能变化；命令必须再次校验并返回
  `STATE_TRANSITION_NOT_ALLOWED`、`ETAG_MISMATCH` 或其他精确冲突；
- 维护冻结、主责系统和权限错误不得被降级为字段校验错误；
- 正式事实没有通用 DELETE，也不使用通用 PATCH status；相关请求应返回
  `ACTION_NOT_ALLOWED`，而不是实现隐藏的兼容路径。
