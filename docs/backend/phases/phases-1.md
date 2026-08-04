# Phase 1：平台治理、权限、审计与统一待办内核

## 1. 分支与交付边界

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-01-platform-governance` |
| 基线 | 与 Phase 2～10 使用同一个冻结 `BACKEND_PHASE_BASE_SHA`，从 `main` 直接创建 |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时不得由本分支自行选择 |
| 独占目录 | `backend/modules/platform-governance/**` |
| 编译要求 | 本 phase 不要求接入根工程或编译通过，但领域代码、测试向量和端口必须完整 |
| 禁止修改 | 根构建清单、锁文件、应用入口、全局路由、全局 OpenAPI、正式数据库迁移、其他 phase 目录、`erp-client/**` |

本 phase 是并行业务包，不是其他 phase 的前置依赖。其他 phase 不得 import 本
phase 的代码；它们在各自目录声明本地端口，Phase 10 再统一适配。

## 2. 目标与范围

实现第一期所有领域共用、但不替任何领域作业务决定的平台语义：

- ERP 稳定身份、来源系统、外部身份映射与单据注册；
- 模块、动作、数据范围、字段、对象状态四层之外再加业务 blocker 的服务端鉴权；
- 只追加的 `workflow_action`、操作审计和敏感字段访问审计；
- `work_item` 固定任务协议、领取、续租、非终结动作、完成、关闭和转交；
- W01 今日工作台和 W02 统一待办所需的查询投影；
- 冻结的批量选择、后台任务、受控文件资产、短时下载和下载再鉴权；
- 幂等操作结果、请求追踪和一期所需的 outbox 抽象。

不实现销售、采购、履约、库存、票款、商城同步等领域事实，也不为尚未登记的
业务任务临时创建类型。

依据：`erp-data-model.md` §4、§6.1、§6.21、§8；
`ui-workspaces/w01-today-workspace.md`、`w02-unified-task-queue.md`、
`w19-permissions-audit.md`。

## 3. 独立实现结构

```text
backend/modules/platform-governance/
  domain/
    identity/
    access/
    audit/
    workflow/
    work_item/
    bulk_selection/
    background_job/
    file_asset/
    outbox/
  application/
    commands/
    queries/
  ports/
  contracts/
  persistence-spec/   # 逻辑约束，不写数据库方言 DDL
  fixtures/
  tests/
  DECISIONS.md
```

不得建立仓库级 `common`、`shared`、HTTP router 或 migration 注册表。跨域对象只用
不透明稳定 ID、版本、指纹和强类型结果表达。

## 4. 必须实现的领域契约

### 4.1 身份、来源与审计

- ERP `id` 无业务含义且不可复用；外部对象通过
  `source_system_id + object_type + external_id_key` 唯一定位。
- 外部键按规范化 UTF-8 二进制语义比较，不折叠大小写或 Unicode。
- 正式事实区分 `occurred_at`、`recorded_at`、`recorded_by`、`source_type` 和
  `source_reference`。
- 审计只追加，至少记录当时角色、动作、对象、请求/追踪号、结果和变更字段名；
  敏感旧值/新值、密钥、完整银行账号、签名 URL 不得进入日志或审计正文。

### 4.2 权限

- 服务端先按权限和数据范围过滤，再返回字段；禁止先返回全量再依赖前端隐藏。
- 历史参与权来自正式参与关系，不得因当前负责人变化被抹除。
- 权限管理员不因能配置权限而自动获得业务敏感正文。
- 所有查询返回 `permissionVersion` / `dataScopeVersion`；正式动作提交时重验。
- 敏感字段揭示使用短时授权并单独审计，权限收回后旧授权立即失效。

### 4.3 统一待办

实现下列领域无关命令和查询语义，不固定 HTTP 路径：

- `ListAccessibleWorkItems`、`GetWorkItem`、`GetTodayWorkspace`；
- `ClaimWorkItem`、`RenewWorkItemLease`；
- `ActOnWorkItem`：只承载保存证据、暂挂等非终结动作；
- `CompleteWorkItem`、`CloseWorkItem`、`TransferWorkItem`。

正式完成信封必须校验 `workItemId`、`claimToken`、`leaseVersion`、
`expectedSubjectVersion`、`expectedSubjectHash`、岗位分离和 `idempotencyKey`。
查询永不返回原始 `claimToken`；令牌只由领取/续租结果返回并仅存会话内存。

任务完成必须与领域正式事实处于同一事务。由于本 phase 不拥有领域事实，它只定义
事务参与端口和验证逻辑；真实同事务适配器由 Phase 10 实现。禁止提供独立的
“标记完成”接口。

### 4.4 批量、后台任务、文件与 outbox

- 批量选择确认后冻结对象集合、水位和预期版本；执行时逐项重新鉴权和校验状态。
- 后台任务取消只停止未开始项，已形成的正式事实不可回滚。
- 导出冻结选择、字段和遮罩，下载时再次鉴权并记录审计。
- 文件扫描通过且属于允许保留类型后才能关联正式对象；成功白名单资产、失败诊断、
  临时导出不得复用同一资产身份。
- 只定义 outbox 原子写入、唯一 `event_id` / `idempotency_key` 和投递状态语义；
  未解决文档冲突前不启用一期对外事件类型。

## 5. 测试要求

至少先写出可运行或可移植的测试向量：

1. 外部身份大小写、并发建映射和单一有效 `PRIMARY`。
2. 无模块权限、越数据范围、字段遮罩、历史参与者和权限收回。
3. 两人并发领取只成功一人；过期 token、旧租约、旧版本、旧指纹均拒绝。
4. 同幂等键重放不重复动作；结果未知不自动移动队列。
5. 转交原任务失效、原租约失效和后继任务创建具有原子语义。
6. 批量快照冻结、逐项重验、部分失败和取消后不回滚已完成事实。
7. 文件扫描失败隔离、敏感日志脱敏、下载再鉴权和保留期分类。
8. outbox 写入失败使业务事务整体失败；投递重试不重复领域动作。

## 6. 未决项与 fail-closed

- W01 Q1～Q3：个人/角色池口径、工作时区、最近打开存储策略未确认；不得静默定值。
- W01 Q4：没有预警可隐藏策略时，不允许用户隐藏正式高风险预警。
- W01 Q5：`pagePreviewLimit` 只接受服务端版本化配置；缺失时的 5 条仅是 UI 安全展示，
  不能写进后端接口或业务验收契约。
- W02 Q1～Q2：有效租约下的管理接管和窄屏正式动作白名单未确认。
- W19 Q1：高风险授权复核任务未登记，命中时返回
  `REVIEW_POLICY_UNCONFIGURED`。
- W19 Q2：时间策略缺失时只允许立即紧急撤权，其余角色写入阻断。
- W19 Q3：字段权限粒度未定时字段策略只读。
- W19 Q4：审计窗口和导出阈值缺失时仅允许服务端保守查询，导出禁用。
- `erp-data-model.md` §5.4 将 outbox 列入二期扩展，但 §10 又要求一期启用
  outbox 基础。当前 phase 只实现抽象和测试，不启用投递器；Phase 10 先修正文档口径。

候选建议不是已确认规则。`DECISIONS.md` 必须逐项记录来源、当前 blocker、需要谁确认、
确认后要改动的契约和测试。

## 7. 完成标准

- 代码和测试只出现在独占目录；
  `git diff --name-only "$BACKEND_PHASE_BASE_SHA"...HEAD` 无越权路径。
- 平台内核没有导入任何其他 phase 实现。
- W01/W02/W19 所需强类型输入、输出、错误和测试向量齐备。
- 所有未决策略均能在缺失时明确拒绝，不存在“先用前端默认值”的路径。
- 输出给 Phase 10 的交接清单包含：逻辑表约束、端口、任务注册候选、错误码、
  事务参与点和未决项；不声称已完成真实数据库、HTTP 或端到端验证。
