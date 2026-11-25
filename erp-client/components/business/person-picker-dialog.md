# 人员选择弹窗接入合同

## 组件入口

从 `@/components/business/person-picker-dialog` 导入 `PersonPickerDialog`、`PersonPickerDepartment` 和 `PersonPickerPerson`。公共入口 `@/components/business` 同步导出上述组件及类型。

## 输入与回填

| 参数                        | 合同                                                                                                                                     |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `id`                        | 同页唯一、稳定的小写 kebab-case 前缀；组件据此派生搜索、展开、选择、移除、关闭和确认按钮的 DOM id。                                      |
| `open` / `onOpenChange`     | 调用方控制开关。触发按钮由调用方提供，并设置独立的原生 DOM id。                                                                          |
| `mode="single"`             | `value` 为人员 ID 或 `null`；`onConfirm` 接收人员 ID 或 `null`。                                                                         |
| `mode="multiple"`           | `value` 为人员 ID 数组；`onConfirm` 接收去重后的人员 ID 数组，保留选择顺序。                                                             |
| `departments`               | 完整、已授权的部门集合。字段为 `id`、`name`、可空的 `parentId`。                                                                         |
| `people`                    | 完整、已授权的人员集合，包含初始已选人员。字段为 `id`、`name`、可空的 `departmentId`，可选 `description`、`disabled`、`disabledReason`。 |
| `loading`                   | 正在获取或刷新完整数据时传 `true`；期间禁止选择和确认，保留草稿，允许取消及移除。                                                        |
| `error` / `onRetry`         | 查询失败时传用户可读的错误信息及重试回调；禁止使用旧候选继续选择和确认。                                                                 |
| `allowEmpty`                | 默认 `false`。传 `true` 后可确认空选择，单选回填 `null`，多选回填 `[]`。                                                                 |
| `title` / `description`     | 可覆盖弹窗标题和说明；默认标题为“选择人员”，未传 `description` 时不渲染说明槽位。                                                        |
| `unassignedDepartmentLabel` | 缺少可见部门归属时的分组及回显文案，默认“未分配部门”；无法读取归属的场景必须改用“未显示部门归属”等准确文案。                             |

## 数据适配

1. 请求由 feature 内的 TanStack Query hook 执行；弹窗不直接访问 API，不推导业务权限。
2. 部门和人员必须使用非空、稳定且唯一的业务 ID，不使用名称或数组下标作为 ID。每位人员按 `departmentId` 展示一次。
3. 调用方必须取得完整候选集合后才结束 `loading`。禁止只传人员目录第一页并把本地搜索当作全量搜索；现有人员目录接口每页 20 条，跨页读取必须遵守其 `scope_version` 合同。此弹窗不包含远程分页能力。
4. 销售、采购和通用业务人员筛选须分别使用原有授权目录。分配、转交等命令场景须沿用该命令的候选来源。禁止用组织管理接口返回的全部人员替代业务候选。
5. 适配 `PersonDirectoryItem` 时，`primary_org_unit_id` 映射为 `departmentId`，`account` 可映射为 `description`；`status !== "active"` 映射为 `disabled: true`，并提供禁用原因。
6. 无部门的人员归入“未分配部门”；部门引用缺失的人员归入“部门信息不可用”。缺失上级的部门作为根节点；循环组织关系断开为根节点，禁止因此隐藏人员。
7. 人员数据中没有职务字段时，不填充示例职务。`description` 只展示调用方提供的真实信息。

## 交互规则

1. 打开时读取 `value` 初始化独立草稿。勾选、移除、清空、搜索和折叠均不回填调用方。
2. 点击确认时调用 `onConfirm`，随后调用 `onOpenChange(false)`。`onConfirm` 负责同步更新调用方字段；业务保存、接口校验和权限复核由调用方负责。
3. 取消、关闭按钮、Escape 或遮罩关闭均丢弃本次草稿。重新打开时使用最新 `value`，搜索清空，部门默认全部展开。
4. 同一次打开期间外部 `value` 变化不覆盖用户草稿。切换业务对象时调用方必须先关闭弹窗；切换模式会重新初始化草稿。
5. 部门按钮只控制展开，不批量选择部门人员。人员行全行可点；单选只保留一位，多选可跨部门选择。
6. 搜索忽略首尾空格及大小写，匹配姓名、`description` 或部门名称。部门命中时展示整个子树；人员命中时保留祖先路径。搜索期间强制展开结果，清除搜索后恢复原折叠状态。
7. 多选展示独立已选名单及数量，支持逐个移除和清空。单选在底部展示姓名及部门路径，支持清除当前草稿。
8. 已选人员缺失或变为不可选时保留其 ID，不静默丢弃，并阻止确认；用户须移除或重新选择。禁止将“未加载完成”当成空结果。
9. 弹窗统一采用 `DialogContent size="lg" scroll="body"` + `DialogHeader` + `DialogBody` + `DialogFooter` 标准骨架，搜索栏固定于正文上方；桌面多选使用左右双栏，单选使用单栏；窄窗口中多选已选名单排在组织架构之后，各区域独立滚动，底部保留已选状态、取消和确认。
10. 输入框打开后获得焦点。部门按钮支持 Tab、Enter、Space；组织树采用紧凑行高（`min-h-control`）、冰蓝选中态（`bg-sidebar-accent`）与等宽数字计数（`.num`）；空态和失败态复用 `BusinessEmptyState` / `BusinessFailureState`；选择控件复用共享 Checkbox / RadioGroup，折叠部门的人员控件不挂载。

## 客户负责销售接入

1. `CustomerAssignmentDialog` 的销售人员字段使用 `CustomerAssignmentPersonPicker` 打开单选弹窗；确认只更新 TanStack Form 的 `userId`，原“添加负责销售”按钮继续提交生效日期、结束日期和调整原因。
2. 候选沿用 `useOwnerOptionsQuery` 的 `/admin/admins` 账号集合。仅在具有 `org_unit:list` 权限时查询组织树，并按候选人员 ID 补充可见的主属部门；不得从组织查询中追加候选。
3. 没有组织读取权限时保留原账号选择能力，并明确显示未开放部门信息。未返回可见部门归属的人员统一归入“未显示部门归属”，不得推断其没有部门。
4. 人员、权限或已授权的组织查询失败时显示重试入口并阻止确认。打开人员弹窗时重新读取账号候选；取消不改变原表单值。

## 调用示例

下列变量由调用方的查询和表单状态提供。`departments`、`people` 必须按上述合同适配完成。

```tsx
import { PersonPickerDialog } from "@/components/business/person-picker-dialog"

// 单选：ownerId 为 string | null；onConfirm 中只更新当前表单字段。
<PersonPickerDialog
    id="customer-owner-picker"
    mode="single"
    open={ownerPickerOpen}
    onOpenChange={setOwnerPickerOpen}
    departments={departments}
    people={people}
    value={ownerId}
    onConfirm={setOwnerId}
    loading={directoryLoading}
    error={directoryError}
    onRetry={reloadDirectory}
/>

// 多选：participantIds 为 string[]；允许确认空选择以清除原值。
<PersonPickerDialog
    id="inquiry-participants-picker"
    mode="multiple"
    open={participantsPickerOpen}
    onOpenChange={setParticipantsPickerOpen}
    departments={departments}
    people={people}
    value={participantIds}
    onConfirm={setParticipantIds}
    allowEmpty
    loading={directoryLoading}
    error={directoryError}
    onRetry={reloadDirectory}
/>
```
