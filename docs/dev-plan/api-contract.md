# HTTP API 契约（P0-4.1 固化）

> 本文是**后端 HTTP 传输层契约**的唯一权威文档，P1–P5 各域接口实现必须遵守。
> 契约来源核对：`backend/apps/web-api/src/core/response.rs`、`backend/apps/web-api/src/core/errors.rs`、
> `backend/services/src/page.rs`、`backend/database/src/repository/base.rs`、
> `backend/services/src/query.rs`、`backend/services/src/audit/dto.rs`、
> `backend/apps/web-api/build.rs`、`erp-client/lib/api/errors.ts`、`erp-client/lib/fixed-decimal.ts`。

---

## 1. 统一信封

所有 HTTP 接口（含成功与失败）的响应体统一为 `ApiResponse<T>`（`backend/apps/web-api/src/core/response.rs`）：

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `status` | `u16`（整数） | 业务状态码，**恒等于 HTTP 状态码**，无独立业务码体系 |
| `errorMessage` | `string` | 成功为 `"OK"`；失败为面向用户的稳定错误文案 |
| `data` | `T \| null` | 业务数据；无数据时为 `null` |
| `success` | `boolean` | 成功为 `true`，失败为 `false` |

### 1.1 成功响应

```json
{
  "status": 200,
  "errorMessage": "OK",
  "data": { "id": "c_xxx", "name": "示例客户" },
  "success": true
}
```

- 成功码约定：**一律 `200` + `success=true`**，不使用 201/204 等变体。
- `data` 为 `null` 的场景（`ApiResponse::ok()`，无 body 的操作类接口，如删除）：
  HTTP 仍为 200，`success=true`，`data=null`。

```json
{ "status": 200, "errorMessage": "OK", "data": null, "success": true }
```

### 1.2 失败响应

```json
{
  "status": 404,
  "errorMessage": "资源不存在: xxx",
  "data": null,
  "success": false
}
```

- 失败时 HTTP 状态码 = 信封 `status` 字段。
- 失败时 `data` **恒为 `null`**，错误细节只放 `errorMessage`。
- 内部错误（500）不得泄漏底层信息：`Internal`/`Repository` 统一返回
  `"系统内部错误"`（`errors.rs` 单测 `internal_error_does_not_expose_underlying_message` 固化）。

---

## 2. 错误码枚举与前端映射

错误枚举定义在 `backend/apps/web-api/src/core/errors.rs`（`impl IntoResponse for Error`），
完整映射如下（域内不得新增顶层错误语义，见 conventions.md §6.8）：

| Error 变体 | HTTP 状态 | errorMessage | 前端 `ApiErrorKind` |
| --- | --- | --- | --- |
| `Unauthorized` | 401 | `Unauthorized` | `Auth` |
| `Forbidden` | 403 | 原文透传（`权限不足: ...`） | `Http` |
| `BadRequest` | 400 | `请求参数错误: ...` | `Validation` |
| `Validation`（validator 校验失败） | 400 | validator 错误文案 | `Validation` |
| `NotFound` | 404 | `资源不存在: ...` | `Http` |
| `Conflict` | 409 | `数据冲突: ...` | `Http` |
| `Unprocessable` | 422 | `业务规则不满足: ...` | `Http` |
| `Logic`（entities 业务规则） | 422 | 原文透传 | `Http` |
| `InsufficientStorage` | 507 | `上传存储空间不足，请稍后重试` | `Http` |
| `RateLimited` | 429 | `请求过于频繁，请稍后重试`（响应带 `Retry-After` 头） | `Http` |
| `RateLimited`（限流状态不可用） | 500 | `系统内部错误` | `Http` |
| `OutcomeUnknown` | 500 | `操作结果暂无法确认，请查询当前状态后再决定是否重试` | `Http` |
| `Internal` | 500 | `系统内部错误` | `Http` |
| `Repository` | 500 | `系统内部错误` | `Http` |

仓储错误折叠规则（`From<database::Error>`）：

- `DuplicateKey` → `Conflict`（409，`数据已存在，请勿重复提交`）
- `OptimisticLockingError` → `Conflict`（409，`数据已被其他请求修改，请刷新后重试`）
- `CommitOutcomeUnknown` → `OutcomeUnknown`（500）

### 前端映射规则

`erp-client/lib/api/errors.ts` 定义 `ApiError { kind, message, status?, responseData?, cause? }`，
`ApiErrorKind = "Network" | "Http" | "Auth" | "Parse" | "Validation" | "Unknown"`。
`client.ts` 拆包规则（P0-4.2 落地，规则以本文为准）：

1. `fetch` 抛错（断网/超时/Abort）→ `Network`（`fromFetchError`）；
2. HTTP 401 或信封 `status=401` → `Auth`（`fromAuth`，提示重新登录）；
3. 非 2xx 且信封 `status=400`（`BadRequest`/`Validation`，业务校验失败）→ `Validation`；
4. 其余非 2xx → `Http`（`fromHttpResponse`，携带 `status` 与 `responseData`）；
5. 响应体 JSON 解析失败 → `Parse`（`fromParse`）；
6. 兜底 → `Unknown`（`fromUnknown`）。

注意：403（权限不足）归类为 `Http` 而非 `Auth`——401 与 403 语义分离，
前端按 403 隐藏/禁用操作，不触发重新登录。

---

## 3. 分页响应形状

契约目标形状（P0-4.1 决策，`P0-foundation.md` §4.1）：

```json
{
  "status": 200,
  "errorMessage": "OK",
  "data": {
    "items": [],
    "total": 42,
    "page": 1,
    "page_size": 20
  },
  "success": true
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `items` | `T[]` | 当前页数据 |
| `total` | 整数（i64） | 满足筛选条件的**总数**（非当前页条数） |
| `page` | 整数 | 当前页码（1 起） |
| `page_size` | 整数 | 请求的分页大小 |

**真实代码现状（必须知晓）**：`backend/services/src/page.rs` 的 `services::Page<T>` 与
`backend/database/src/repository/base.rs` 的 `PageResult<T>` 当前**只序列化 `items` + `total`**
（两者均为冻结文件，单测断言 `{"items": [...], "total": 1}` 形状）。
`page`/`page_size` 目前只存在于请求参数侧。P3 实现列表接口时按契约目标形状补齐
`page`/`page_size`（域 DTO 补充或走地基修订统一扩展 `Page<T>`），
不允许静默沿用 `{items,total}` 直出。

分页语义（`database::Pagination`，`repository/base.rs`）：页码 1 起，
`page=0` 归一化为第一页；`skip = (max(page,1)-1) * page_size`；
仓库默认按 `created_at` 降序返回（`search()` 内建排序）。

---

## 4. 列表查询参数统一

列表接口查询参数（Query String，全部可选，扁平化）：

| 参数 | 类型 | 默认 | 约束 |
| --- | --- | --- | --- |
| `page` | 整数 | 1 | ≥ 1（validator `range(min=1)`） |
| `page_size` | 整数 | 20 | 1–100（validator `range(min=1, max=100)`，内部再 clamp，见 `services/src/query.rs`） |
| `sort_by` | 字符串 | 域自定（仓库默认 `created_at`） | 排序字段**白名单在 Service 层校验**（P3-service-api.md §4.3），禁止任意字段透传 |
| `sort_dir` | `asc` \| `desc` | `desc` | 非法值按契约默认处理或 400（由域 Service 决定，白名单逻辑须有测试） |
| 域内筛选字段 | 各域定义 | — | **扁平传递**，不做嵌套对象；如 `status=ACTIVE&q=xxx&actor_account=admin` |

域内筛选字段（如 `status`、`q`、`owner`、`mappingType`）一律与分页参数同级扁平传递，
禁止 `filter[status]=...` 这类嵌套形态。空值/空白字符串视为未提供（`normalized_text`）。

URL 示例：

```
GET /admin/customers?page=2&page_size=20&sort_by=created_at&sort_dir=desc&status=ACTIVE&q=华东
GET /admin/audit-logs?page=1&page_size=50&actor_account=admin01&action=customer.update
```

真实实现参照：`backend/services/src/audit/dto.rs` 的 `AuditLogListParams`
（`page`/`page_size` + 扁平筛选字段，`Validate` 校验 + `normalized()` 归一化）。

---

## 5. 时间与数值传输

### 5.1 时间：秒级 Unix 时间戳

**最终结论（P0 定）**：所有时间字段以**秒级 Unix 时间戳**（整数）传输。

- 真实代码佐证：`BaseModel.created_at/updated_at/deleted_at` 为 `u64` 秒
  （`entity-core`，写入用 `chrono::Local::now().timestamp()`）；
  `AuditLogItem.created_at: u64` 序列化为整数（`audit/dto.rs` 单测断言 `"created_at": 42`）。
- 精度：**秒级**，无毫秒；业务时间（`Instant`）持久化统一时基（UTC 秒），
  展示层由前端转业务时区（conventions.md §5）。
- 示例：`1754438400` 表示 `2025-08-06T00:00:00Z`（= 北京时间 `2025-08-06 08:00:00 +08:00`）。
- 前端消费：真实接口返回秒级整数，前端在 api.ts 边界转换
  `new Date(seconds * 1000).toISOString()` 后交给现有 `lib/datetime.ts` 的 `formatDateTime` 展示
  （该函数接受 ISO 字符串）；禁止直接把秒级整数塞给 `new Date(seconds)`（会被当作毫秒）。

### 5.2 金额与数量：一律字符串

**最终结论（P0 定）**：金额、单价、数量、税率等定点数值一律以**十进制字符串**传输。

- 原因：JS 浮点失真（`0.1 + 0.2 !== 0.3`），禁止 `f64`/`number` 直传。
- 后端形态：`entities::money` 定点 newtype（`Amount` 2 位、`UnitPrice` 4 位、
  `Quantity` 6 位、`Rate` 6 位），BSON 持久化为 `Decimal128`；序列化为字符串（如 `"1234.56"`）。
- 禁止字段形态：`f64`、裸 `number`、带 `%` 的百分号字符串。

前端消费函数（`erp-client/lib/fixed-decimal.ts`，签名如下，P4 直接复用）：

| 函数 | 签名 | 用途 |
| --- | --- | --- |
| `parseDecimal` | `(value: string, options: { maxScale: number; allowNegative?: boolean }) => ParsedDecimal` | 解析规范十进制字符串，校验小数位超限 |
| `canonicalDecimal` | `(value: string, options: { maxScale: number; allowNegative?: boolean }) => string` | 规范化字符串（去多余尾零） |
| `normalizeFixed` | `(value: string, options: { maxScale; outputScale; allowNegative? }) => string` | 按输出精度舍入 |
| `multiplyFixed` | `(left: string, right: string, options: { leftMaxScale; rightMaxScale; outputScale }) => string` | 定点乘法 |
| `sumFixed` | `(values: readonly string[], options: { maxScale; outputScale; allowNegative? }) => string` | 定点求和 |
| `splitGrossByFractionRate` | `(grossAmount: string, taxRate: string) => { gross; net; tax }` | 分数税率拆含税/不含税/税额 |
| `splitGrossByPercentRate` | `(grossAmount: string, taxRatePercent: string) => { gross; net; tax }` | 百分比税率拆分 |
| `compareDecimal` | `(left: string, right: string, maxScale: number) => -1 \| 0 \| 1` | 字符串比较（不经 number） |

前端不做二次运算取整：后端返回已舍入值，前端只展示与格式化（conventions.md §8.6）。

---

## 6. 权限生成物

- **产出位置**：`backend/apps/web-api/build.rs` 写入
  `erp-client/lib/permissions.generated.ts`（内联 `PermissionItem` 类型，2 空格缩进，匹配 erp-client 风格）
- **生成命令**：`cargo build -p web-api`（build.rs 解析 `src/core/routes/admin.rs` 路由与
  handler 的 `#[permission_macros::permission(...)]`，`rerun-if-changed` 监听路由与 handler 文件）。
  生成文件头为 `// @generated by apps/web-api/build.rs. Do not edit.`，**禁止手工编辑**。
- **CI 漂移校验**：P3 验收 gate `permissions_no_drift`（`_meta.json`）；
  校验命令 `git diff --exit-code erp-client/lib/permissions.generated.ts`
  （P3-service-api.md §5.1）。P0-6.2 将落地
  `check-permissions-drift.sh`（CI 脚本，当前仓库尚未出现该文件，属并行 P0 任务产物）；
  生成物必须随 PR 提交且 CI 校验无漂移（conventions.md §6.9）。

---

## 7. 与 erp-client 现有 mock 的差异清单

依据对 `erp-client/features/*/api.ts` 与 `types.ts` 的实际核对，真实后端契约与 mock 形态存在以下差异。
**P4 替换 `api.ts` 实现时按契约修正**（mock 文件随后删除，见 conventions.md §8.3）。

| # | 差异点 | mock 现状 | 契约要求 | 涉及 feature |
| --- | --- | --- | --- | --- |
| 1 | 时间字段形态 | ISO 8601 字符串（如 `new Date().toISOString()`、`"2026-08-01T09:00:00+08:00"`） | 秒级 Unix 时间戳整数 | **全部 feature**（`submittedAt`/`createdAt`/`postedAt`/`sealedAt`/`freshness` 等） |
| 2 | 分页参数命名 | `page` + `pageSize`（camelCase）；部分 feature 用嵌套 `pageInfo: { page, pageSize, total }` | `page` + `page_size`（snake_case，扁平） | sales-orders、mall-consumption-orders、supplier-orders、execution-projections 等全部列表接口 |
| 3 | 分页响应附加字段 | mock 携带 `metrics`、`queriedAt`、`permissionVersion`、`context` 等演示字段 | 契约只保证 `items`/`total`/`page`/`page_size` | sales-orders、mall-sync 等（演示字段不入真实接口，由前端本地派生或删除） |
| 4 | 金额/数量 | **大部分已是字符串**（sales-orders `quantity`/`unitPriceGross`/`amountGross`、mall-consumption-orders `paidAmount`、card-business-analytics `consumptionGross`/`rate`）——与契约一致，**无需改动** | 字符串 | — |
| 5 | 少数 number 残留 | `card-business-analytics` `ratePercent: number`（覆盖率百分比） | 若属 Rate 语义字段须改字符串；纯展示派生值可由前端本地计算 | card-business-analytics（P4 核对处理） |
| 6 | 秒数/计数类 number | `mall-sync` `syncLagSeconds: number`、各列表 `total`/指标 `count` | 允许（非定点金额/数量，计数与秒数按整数传输） | 无需改动 |

> 核对依据：`features/sales-orders/api.ts`（`SalesOrderListView = { items, total, page, pageSize, metrics, queriedAt }`）、
> `features/mall-consumption-orders/types.ts`（`pageInfo: { page, pageSize, total }`）、
> `features/mall-sync/api.ts`（全部 ISO 时间字符串）、`features/card-business-analytics/types.ts`（`ratePercent: number`）。

---

## 8. 审批运行与待办接口合同

本节与 `docs/approval-workflow-contract.md` 共同生效。P3、P4 不得继续沿用旧
`claim / complete / UNCLAIMED / IN_PROGRESS` 形态。

### 8.1 队列查询

```text
GET /admin/work-items?scope=mine|team|managed|history&...
```

- `scope=mine`：只返回 `status=OPEN` 且 `owner_user_id=当前用户` 的任务；
- `scope=team`：只返回当前用户有资格处理、`status=OPEN`、`assignment_mode=POOL` 且
  `owner_user_id` 为空的任务；
- `scope=managed`：只向具备任务责任管理权限的主管返回其授权组织和数据范围内全部开放任务，
  包括未分派责任池任务和已由下属负责的任务；不得接受任意组织或用户扩大范围；
- `scope=history`：只返回当前用户曾负责、曾完成、曾关闭，或当前具有组织级历史查看权的
  `COMPLETED/CLOSED` 任务；历史任务只读；
- `mine/team/managed` 只允许 `status=OPEN`，`history` 只允许
  `status=COMPLETED|CLOSED`；不兼容组合返回 400；
- 查询范围由服务端根据 JWT 用户、角色、组织、数据范围和对象参与权形成；
  接口不得接受任意 `owner_user_id` 代替权限过滤；
- 响应使用 `status: OPEN | COMPLETED | CLOSED`、
  `assignment_mode: DIRECT | POOL`、责任组织、可空 `owner_user_id` 和必填 `task_version`，不得返回客户端租约或领取令牌；
- `task_version` 是 `work_item` 自身持久化乐观锁版本的 API 名称。任何队列、对象详情或业务工作面
  只要嵌入可操作任务摘要，都必须返回同一字段；不得用业务对象 `subject_version` 代替。
- 阻塞步骤保留的开放待办必须返回 `processing_state=APPROVAL_BLOCKED`、权限安全的结构化阻塞摘要和空
  `allowed_actions`；其它可处理任务返回 `processing_state=READY`。

### 8.2 建立和调整责任

| 方法与路径 | 用途 | 关键约束 |
| --- | --- | --- |
| `POST /admin/work-items/{id}/start-processing` | 从“团队待处理”建立本人责任 | 只适用于开放 `POOL` 任务；同一用户重复请求幂等 |
| `POST /admin/work-items/{id}/release-to-team` | 退回团队 | 只清空原开放任务责任；原因必填；不创建后继任务 |
| `POST /admin/work-items/{id}/reassign` | 受控转交 | 目标用户、权限、数据范围和岗位分离由服务端重验 |
| `POST /admin/work-items/{id}/close` | 关闭重复、误派或已有替代的任务 | 专门权限和原因必填；不得关闭未完成审批、确认或补偿任务 |

所有写接口必须接收统一幂等键和预期任务版本；冲突返回 409 及新的服务端任务摘要。
请求中的 `expected_task_version` 必须来自最近一次查询的 `task_version`；客户端不得生成或递增。
客户端不得提交 `assignment_mode`、`owner_role`、下一步骤或任意动作代码改变服务端路由。

### 8.3 审批和正式任务动作

`start_approval`、`submit_decision`、`cancel_approval`、`recover_approval` 是服务端应用端口，不是允许客户端
任意启动定义或选择完成动作的公共 HTTP 接口。每个业务工作面必须使用已注册的强类型接口，
由 Handler 根据任务类型构造对应命令并调用运行时端口。

每个决定命令至少携带：

```text
work_item_id
approval_instance_id
expected_task_version
expected_instance_version
expected_step_version
expected_subject_version
decision
reason
idempotency_key
```

服务端必须重验活动步骤、当前责任人、对象版本、权限、数据范围和岗位分离。
响应必须同时返回审批实例结果、当前任务结果、业务对象最新状态，以及存在时的下一开放任务摘要。

不得提供以下接口或等价能力：

```text
POST /admin/work-items/{id}/claim
POST /admin/work-items/{id}/complete
```

非审批任务同样由任务类型绑定的强类型领域命令完成。客户端不得提交
`completion_action`，也不得仅凭 HTTP 2xx 本地推进下一步骤或业务状态。

### 8.4 阻塞审批管理

| 方法与路径 | 用途 | 关键约束 |
| --- | --- | --- |
| `GET /admin/approval-instances?status=BLOCKED` | 查询授权范围内受阻审批 | 专门诊断权限；返回实例、当前步骤、阻塞码和各自版本，以及可选当前任务摘要 |
| `POST /admin/approval-instances/{id}/recover` | 重试当前步骤 | 专门恢复权限；只允许 `recovery_action=RETRY_CURRENT_STEP` |

恢复请求必须包含 `current_step_instance_id`、`expected_instance_version`、
`expected_step_version`、存在开放待办时的 `expected_task_version`、结构化原因和幂等键。
请求不得包含审批决定、目标步骤、目标用户或业务字段。服务端只有在阻塞原因已被证明消除后，
才能恢复原步骤并创建或校正唯一开放待办；否则保持 `BLOCKED`。冲突返回 409 和最新阻塞摘要。

### 8.5 BPM 关联

业务 HTTP 契约不因 `runtime_kind=INTERNAL|BPM` 改变。`external_instance_id`、
`external_activity_id` 和消息关联键只向具备诊断权限的管理接口展示，不进入普通工作面动作参数。
接入 BPM 前必须先完成 outbox/inbox、幂等消费、结果查询和人工恢复的独立基础设施修订。
