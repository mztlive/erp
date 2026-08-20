# 阶段 07：前端审批流程配置

> 阶段性质：P4 前端工作包
>
> 阶段目标：交付以固定单据类型为目录的审批流程管理工作面
>
> 允许状态：可使用阶段 06 的冻结 DTO；后端未部署时仅使用类型化 fixture 完成组件测试

## 1. 文件责任

新增：

```text
erp-client/app/(workspace)/system/approval-processes/page.tsx
erp-client/features/approval-processes/
├── api.ts
├── types.ts
├── queries.ts
├── pages/approval-processes-page.tsx
├── components/process-catalog.tsx
├── components/definition-editor.tsx
├── components/node-list-editor.tsx
├── components/assignee-combobox.tsx
├── components/publish-dialog.tsx
├── components/retire-dialog.tsx
├── components/version-history.tsx
└── **/*.test.tsx
```

本阶段不修改共享 `workspace-registry.ts`、`lib/api/**` 和生成权限文件。通用 feature 稳定后，由 P0-C 注册 W24 和生成权限；发现额外共享需求时另行发起单主题 P0 amendment。

## 2. 工作面注册合同

P0-C 使用当前空缺编号 `W24` 注册：

```text
路径：/system/approval-processes
名称：审批流程配置
主权限：approval_process:read
```

页面只能展示服务端返回的固定 `DocumentType` 目录（合同 §4.3 的 20 行），不得让用户创建自定义单据类型或组织级覆盖。「销售单（实物及服务）」与「卡券销售单」是两个独立目录行。

## 3. 类型与请求层

`types.ts` 必须与 API 枚举逐项对齐，并将服务端版本字符串保持为字符串。不得复用当前卡券审批 DTO。

前端只建模审批产品合同，不得暴露或允许提交 `bpm::ProcessKind`、`SubjectRef`、`TransitionPlan`、内部 BPM 事件或 crate 结构。单据类型、流程定义 ID、节点和状态只能来自阶段 06 的 Service/HTTP DTO；前端不得自行维护 `DocumentType -> ProcessKind` 映射。

业务页面和交互组件必须使用 `"use client"`。`api.ts` 只负责序列化和统一 `ResultAsync` 错误 envelope；组件不得裸 `fetch`。`queries.ts` 使用 TanStack Query，Query Key 固定包括：

```text
approvalProcesses.catalog
approvalProcesses.versions(documentType)
approvalProcesses.detail(definitionId)
approvalProcesses.eligibleAssignees(documentType, search)
```

Mutation 成功后按服务端返回的定义 ID、单据类型精确失效目录、版本和详情；不得全局清空 QueryClient。

## 4. 目录页面

目录表必须显示：单据类型、审批政策、当前版本、配置状态和允许动作。

显示规则：

- `NO_APPROVAL`：显示“无需审批 / 不适用”，不展示创建入口；
- `PROCESS_REQUIRED + PUBLISHED`：显示当前版本和“查看/创建新草稿”；
- `PROCESS_REQUIRED + DRAFT`：显示草稿版本和“继续编辑”；
- `PROCESS_REQUIRED + MISSING_CONFIGURATION`：显示明确阻断状态，不得显示为“无需审批”。

按钮只能按服务端 `allowed_actions` 和生成权限共同收窄；前端权限不得替代服务端授权。

创建草稿对话框必须要求管理员显式选择“空白流程”或“复制当前已发布版本”，并分别提交 `draft_source=EMPTY|CURRENT_PUBLISHED`。当前无已发布版本时必须禁用复制选项；前端不得提交源定义 ID、任意历史版本或隐式默认值。`SalesOrder + EMPTY` 打开编辑器时可预置名为「采购确认」的普通节点，允许删除；`node_purpose` 仍不得进入请求。

## 5. 草稿编辑器

表单必须使用 `useAppForm` 和 Zod/Standard Schema，提交副作用必须调用 TanStack Query `useMutation`。节点列表只允许：

- 增加/删除节点；
- 调整线性顺序；
- 修改节点名称；
- 搜索并选择一个具体审批用户。

页面不得暴露连线编辑器、角色池、候选人、条件表达式、处理器、业务动作或驳回目标。顺序保存前规范化为从 1 开始连续递增；新增节点不得生成或提交 `node_key`，已有节点只回传服务端 `node_id`，重新排序不得尝试换 key。

`SalesOrder` 空白草稿可预置名为「采购确认」的普通节点，允许删除、改名和调整顺序。其他类型不得显示采购确认用途。`node_purpose` 不得进入写请求；发布不再校验该用途。

审批人选择器必须调用 `eligible-assignees`，显示用户状态和资格结果；不得下载全量账号后在浏览器过滤。服务端仍为最终权威。

## 6. 发布与退役

发布确认必须展示最终线性路径和固定驳回说明：

```text
张三 → 李四 → 王五
任一层驳回后，将从张三开始下一轮审批。
```

发布/退役请求必须携带当前 `definition_lock_version` 和新幂等键。`409` 时关闭提交态、保留本地输入、刷新服务端版本并提示用户重新确认，不得自动覆盖。

已发布/已退役详情永久只读。修改人员必须创建更高版本草稿。

## 7. 状态与错误处理

页面必须分别处理：首次加载、刷新、无权限、配置缺失、空历史、409 冲突和 422 发布校验失败。TanStack Query 的背景刷新不得把已有内容替换为整页骨架。

错误按稳定 `code` 映射中文文案；未知错误显示 correlation ID。不得匹配后端 message 文本。

所有新增或修改的函数、hook 和组件必须按 `erp-client/AGENTS.md` 添加有意义的 JSDoc；用户可见字符串必须通过 `docs/ui-glossary.md`，枚举必须显式映射中文，不得渲染原值或内部 ID。

## 8. 测试

必须覆盖：

1. 20 个固定类型完整渲染，含 `VoucherSalesOrder`；
2. 8 个 `NO_APPROVAL` 类型没有写入口；
3. 草稿节点增删、排序、单人选择和连续序号；
4. 请求体不包含 transition、role、handler 或 action；
5. 发布预览显示固定驳回语义；
6. 陈旧版本冲突不会静默覆盖；
7. 已发布/退役版本只读；
8. 按 `allowed_actions` 和权限隐藏/禁用操作；
9. Query Key 和精确失效行为。
10. 新增节点请求无 key、已有节点只提交 node ID；
11. `useAppForm + Zod + useMutation` 提交流程和 `ResultAsync` 错误分支；
12. 业务组件均为 Client Component，不存在 RSC/SSR 业务取数。
13. 草稿来源必须显式选择，无已发布版本时不能选择 `CURRENT_PUBLISHED`，请求不含源定义 ID。
14. `SalesOrder + EMPTY` 首次编辑可预置名为「采购确认」的普通节点，允许删除；其他类型不预置该节点，写请求不含 `node_purpose`。

## 9. 阶段验收

- [ ] 页面只能配置固定类型的线性用户审批节点。
- [ ] 所有业务请求使用 TanStack Query，所有表单使用 TanStack Form。
- [ ] API 边界使用现有 `ResultAsync` 合同，组件内不存在裸请求或 `useEffect` 拉取。
- [ ] 页面不包含角色池、任意连线和动态业务动作概念。
- [ ] `SalesOrder` 预置采购确认节点可删除，客户端不能提交或改写用途。
- [ ] 页面和请求模型不包含 `ProcessKind`、`SubjectRef`、`TransitionPlan` 或内部 BPM 事件。
- [ ] 配置缺失是显式阻断状态。
- [ ] 用户可查看历史版本但不能修改发布事实。
- [ ] 新增方法具备 JSDoc，用户文案通过术语表。
- [ ] `conventions.md` 第 6 节全部前端门禁和本 feature 测试通过；全量浏览器验收由阶段 11 执行。
