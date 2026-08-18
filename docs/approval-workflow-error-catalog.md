# 审批流程错误目录

> 状态：线协议合同
>
> 权威位置：本文件是稳定错误码、HTTP 映射、可重试性、业务事实提交和前端动作的唯一目录。
> 路径与信封形状见 [approval-workflow-openapi.yaml](./approval-workflow-openapi.yaml)。
> 业务语义见 [approval-workflow-contract.md](./approval-workflow-contract.md)。
> 运维处置见 [runbooks/approval-workflow.md](./runbooks/approval-workflow.md)。

本目录只记录生效合同。Handler 必须把服务层稳定码映射为本表 HTTP 状态，不得把 BPM、MongoDB 或权限策略细节暴露给客户端。

## 1. 信封与暴露规则

成功响应必须为：

```text
status=200
errorMessage=OK
success=true
data=<有权读取的最新视图>
```

失败响应必须为：

```text
status=<HTTP>
errorMessage=<本表允许的消息>
code=<稳定码>
success=false
data=<仅 409 可带 correlation_id 与有权查看的最新版本；403/404/500 必须为 null>
```

固定规则：

1. 全部乐观锁版本必须是十进制正整数字符串。
2. 403/404 不得附带可推断资源存在性、内部版本、账号隐私、授权策略或数据库结构的详情。
3. 409 必须包含稳定码、`correlation_id` 和调用者有权查看的最新任务/实例/定义锁版本。
4. 幂等重复成功必须先重验当前动作权限及合同 §4.6 对应门禁；失权时返回不泄露存在性的 403/404，不得返回收据引用。仍有权时返回 2xx 最新摘要，不得把当前可变视图描述为原请求时刻快照，也不得返回「重复请求」冲突。
5. `APPROVAL_POLICY_NOT_REGISTERED` 必须映射为 500，并同时使 readiness 失败。不得映射为 4xx，不得作为客户端 422。
6. 内部数据损坏应进入 `BLOCKED` 并记录结构化 `blocker_code`；无法形成合法阻塞事实时返回 500、告警并冻结该实例写入。
7. 决定时检测到人员失效的命令必须先提交 `BLOCKED` 事实，再把 `DecisionOutcome::Blocked` 映射为 409 `APPROVAL_INSTANCE_BLOCKED`；不得通过错误传播回滚阻塞状态。

## 2. 通用信封码

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `UNAUTHENTICATED` | 401 | 缺少或失效的管理员 JWT | 否 | 否 | 清理会话并回到登录 | 「登录状态已失效，请重新登录」 |
| `PERMISSION_DENIED` | 403 | 动作级权限失败，或无权得知资源存在性 | 否 | 否 | 停止当前操作，不展示资源是否存在 | 「当前账号没有执行此操作的权限」；`data=null` |
| `NOT_FOUND` | 404 | 资源不存在，或与无权存在性合并 | 否 | 否 | 停止当前操作，不推断资源是否存在 | 通用「资源不存在」；`data=null` |
| `INVALID_REQUEST` | 400 | 版本不是正整数字符串，或协议字段无法解析 | 否 | 否 | 修正字段后由用户显式重提 | 字段级协议说明，不得暴露内部类型 |
| `BUSINESS_RULE_BLOCKED` | 422 | 未知字段、非法 view/status、超页上限、空白原因或跨 view cursor | 否 | 否 | 按字段提示修正，不得改写成伪空列表 | 面向调用方的校验说明；`data=null` |
| `INTERNAL_ERROR` | 500 | 未分类内部失败 | 否 | 否 | 停止业务操作并上报关联 ID | 只允许「系统内部错误」；`data=null` |

## 3. 审批稳定码

「可重试」仅表示用户刷新最新版本后允许再次提交同一业务意图；不表示客户端自动重试。
「提交业务事实」表示该响应返回时，目标实例/执行/任务/收据是否已经写入并提交。

### 3.1 部署与配置

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `APPROVAL_POLICY_NOT_REGISTERED` | 500 | 固定 `DocumentType` 缺少政策，或政策缺少必需动作/类型权限；readiness 必须同时失败 | 否 | 否 | 不得当 4xx 处理；停止全部审批写操作并告警运维 | 对外只允许「系统内部错误」和稳定码；不得暴露缺失政策内容 |
| `APPROVAL_PROCESS_NOT_CONFIGURED` | 409 | `PROCESS_REQUIRED` 类型没有可绑定的已发布定义 | 否 | 否 | 引导具备定义管理权的用户发布定义；不得创建无绑定单据 | 说明该单据类型尚未配置可绑定的已发布审批流程；可回读 correlation ID |
| `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER` | 409 | `PROCESS_REQUIRED` 类型尚未完成目标 rollout | 否 | 否 | 停止该类型提交；不得回退旧审批路径 | 说明该类型尚未启用新审批；不得提示旧入口 |
| `APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE` | 409 | `draft_source=CURRENT_PUBLISHED` 但当前无发布定义 | 否 | 否 | 改为 `EMPTY` 或先发布定义 | 「当前没有可复制的已发布定义」 |

### 3.2 定义管理

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `APPROVAL_DEFINITION_NOT_DRAFT` | 409 | 对已发布或已退役定义执行节点替换 | 否 | 否 | 重新打开活动草稿；不得就地改已发布图 | 「只能修改草稿定义」 |
| `APPROVAL_DEFINITION_VERSION_CONFLICT` | 409 | 定义锁版本过期 | 是 | 否 | 读取最新 `definition_lock_version` 后由用户显式重提 | 最新定义锁版本 |
| `APPROVAL_DEFINITION_INVALID` | 422 | 图、节点数量/顺序、人员、用途或岗位分离静态校验失败 | 否 | 否 | 按校验说明修正节点；不得提交连线或 `node_purpose` | 面向配置者的图/人员校验说明，不得暴露内部处理器 |
| `APPROVAL_DEFINITION_BINDING_CORRUPTED` | 409 | 单据绑定缺失或不一致 | 否 | 否 | 停止提交；运行管理员按 runbook 走受阻取消或前向修复 | 「单据审批绑定缺失或不一致」；不得暴露集合结构 |

### 3.3 启动、决定与版本

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `APPROVAL_ALREADY_STARTED` | 409 | 同一提交版本已有非终态实例 | 否 | 否 | 打开已有实例；不得再发启动 | 「该提交版本已有未结束的审批实例」 |
| `APPROVAL_TASK_NOT_OPEN` | 409 | 任务已完成或关闭 | 否 | 否 | 刷新任务/实例；不得对旧任务再决定 | 「审批任务已完成或关闭」 |
| `APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR` | 403 | 当前用户不是任务、执行、实例三方一致责任人 | 否 | 否 | 停止决定；不得展示任务或实例存在性 | 「当前账号没有执行此操作的权限」；`data=null` |
| `APPROVAL_TASK_VERSION_CONFLICT` | 409 | 任务版本过期 | 是 | 否 | 用回读 `latest_task_version` 刷新后由用户显式重提 | 最新任务版本 |
| `APPROVAL_INSTANCE_VERSION_CONFLICT` | 409 | 实例并发变化 | 是 | 否 | 用回读 `latest_instance_version` 刷新后由用户显式重提 | 最新实例版本 |
| `APPROVAL_EXECUTION_VERSION_CONFLICT` | 409 | 节点执行并发变化 | 是 | 否 | 刷新当前执行后由用户显式重提 | 调用者有权查看的最新执行版本 |
| `APPROVAL_SUBJECT_VERSION_CONFLICT` | 409 | 单据提交版本与冻结 `subject_version` 不一致 | 否 | 否 | 停止当前提交；不得改写冻结版本 | 「单据提交版本不一致」 |
| `APPROVAL_REJECT_REASON_REQUIRED` | 422 | 驳回原因为空或仅空白 | 否 | 否 | 要求填写原因后再提交 | 「驳回必须填写原因」 |
| `APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT` | 409 | 同幂等键用于不同 canonical payload | 否 | 否 | 停止自动重放；为新意图生成新幂等键并由用户确认 | 「相同幂等键已用于不同请求内容」 |

### 3.4 受阻、恢复、改派与取消

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `APPROVAL_INSTANCE_BLOCKED` | 409 | 当前实例已受阻，不能继续决定；人员失效决定路径必须先提交 BLOCKED 事实 | 否 | **是**（人员失效决定路径已提交 BLOCKED、关闭旧 OPEN 任务并写收据） | 读取 `recovery-options`，只展示返回的唯一动作；不得继续决定或重开旧任务 | 受阻说明、correlation ID、有权查看的最新实例/任务版本；不得暴露账号隐私 |
| `APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER` | 409 | 实例非 `BLOCKED`，或当前 blocker 不属于人员失效 | 否 | 否 | 按 `recovery-options` 改调唯一合法动作 | 「当前受阻原因不允许恢复原审批人」 |
| `APPROVAL_CURRENT_APPROVER_NOT_RECOVERED` | 409 | 恢复命令执行时原审批人仍不合格 | 否 | 否 | 保持受阻；需要换人时改调改派 | 「原审批人仍不合格，不能恢复」 |
| `APPROVAL_CURRENT_APPROVER_RECOVERED` | 409 | 改派时原审批人已经恢复有效 | 否 | 否 | 只能改调恢复端口 | 「原审批人已恢复，只能调用恢复接口」 |
| `APPROVAL_REASSIGN_TARGET_INELIGIBLE` | 422 | 改派目标不满足账号、任职、资格、读取权、DataScope 或岗位分离 | 否 | 否 | 重新搜索合格候选人；不得手填绕过 | 「改派目标不满足审批资格」；不得暴露具体策略细节 |
| `APPROVAL_REASSIGN_NOT_ALLOWED_FOR_BLOCKER` | 409 | 实例非 `BLOCKED`，或当前 blocker 不属于人员失效 | 否 | 否 | 按 `recovery-options` 改调；不得当普通转签 | 「当前受阻原因不允许改派」 |
| `APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED` | 409 | 实例非 `BLOCKED`，或当前 blocker 属于人员失效 | 否 | 否 | 人员失效只能恢复或改派 | 「当前受阻原因不允许受阻取消」 |

### 3.5 通用待办保护

| 错误码 | HTTP | 触发条件 | 可重试 | 提交业务事实 | 前端动作 | 允许暴露 |
| --- | --- | --- | --- | --- | --- | --- |
| `APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN` | 409 | 通用 `reassign`/`close`/`complete`/`transfer` 试图修改 `DocumentApproval` 或带 `approval_node_execution_id` 的任务 | 否 | 否 | 改走审批决定、恢复、改派或受阻取消端口 | 「审批任务不能通过通用待办接口修改」 |
| `WORK_ITEM_VERSION_CONFLICT` | 409 | 非审批任务版本过期 | 是 | 否 | 刷新任务版本后由用户显式重提 | 有权查看的最新任务摘要 |
| `WORK_ITEM_RESPONSIBILITY_CONFLICT` | 409 | 非审批任务当前责任已变化 | 否 | 否 | 刷新任务；不得继续原责任命令 | 有权查看的最新任务摘要 |

## 4. 禁止映射

1. 不得把 `APPROVAL_POLICY_NOT_REGISTERED` 映射为 400/403/404/409/422。
2. 不得把人员失效决定路径的已提交 `BLOCKED` 事实回滚后改报 403。
3. 不得把无权读取伪装成空列表；`mine`/`blocked` 的非法 `status` 必须 422。
4. 不得保留或映射 `approval_instance:recover`、`approval_instance:diagnose`、`RETRY_CURRENT_STEP`、`TERMINATE_APPROVAL`、`REJECT_TO_APPLICANT`。
5. 政策缺少受阻取消所需动作属于 `APPROVAL_POLICY_NOT_REGISTERED`/500，不得改报 `APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED`。
6. 未接入类型必须返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`，不得回退旧运行时或默认办理人。
