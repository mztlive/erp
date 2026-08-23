/**
 * 权限目录：由 build.rs 生成的 PERMISSION_GROUPS 派生。
 *
 * 三件事在这里一次做完，页面与 hook 只消费结果：
 * - 按权限编码去重（同一编码可能挂多个接口，界面上必须只出现一次）；
 * - 按「对象 × 动作」组织成矩阵，取代 397 条平铺清单；
 * - 把 resource / action 翻译成业务语言（界面不出现英文编码与实现术语）。
 *
 * 纯数据与纯函数，供权限面板组件、状态 hook 与角色列表摘要共用。
 */

import { PERMISSION_GROUPS } from "@/lib/permissions.generated"

/** 权限编码对应的后端接口；同一权限可覆盖多个接口。 */
export type PermissionEndpoint = {
    method: string
    path: string
}

export type PermissionItemOption = {
    /** 后端权限字符串（resource:action）。 */
    code: string
    resource: string
    action: string
    description: string
    endpoints: readonly PermissionEndpoint[]
    /** 高风险动作：删除、作废、冲销、撤权、退款、查看敏感信息。 */
    dangerous: boolean
}

export type PermissionGroupOption = {
    name: string
    description: string
    items: PermissionItemOption[]
}

export type PermissionPanelTab = "business" | "system"

export const PERMISSION_PANEL_TAB_LABEL: Record<PermissionPanelTab, string> = {
    business: "业务",
    system: "系统",
}

/** 归属「系统」维度的权限组名（平台 / 治理 / 访问控制类）；其余归「业务」。 */
const SYSTEM_GROUP_NAMES = new Set([
    "账号管理",
    "角色管理",
    "系统审计",
    "来源注册",
    "单据注册",
    "统一待办",
    "批量任务",
    "文件资产",
    "权限与审计",
    "集成治理",
])

export function isSystemGroup(name: string): boolean {
    return SYSTEM_GROUP_NAMES.has(name)
}

/** 高风险动作：勾选后可造成不可逆后果或泄露敏感值。 */
const DANGEROUS_ACTIONS = new Set([
    "delete",
    "void",
    "reverse",
    "revoke",
    "reveal",
    "refund",
])

export function isDangerousAction(action: string): boolean {
    return DANGEROUS_ACTIONS.has(action)
}

/** 动作中文名；界面一律用这里的措辞，不展示英文动作名。 */
const ACTION_LABEL: Record<string, string> = {
    list: "查看列表",
    detail: "查看详情",
    read: "查看",
    preview: "预览",
    assignable_list: "查看可分配范围",
    create: "新建",
    update: "修改",
    edit: "编辑",
    update_role: "调整角色",
    register: "登记",
    submit: "提交",
    decide: "审批",
    review: "复核",
    confirm: "确认",
    confirm_requirement: "确认需求",
    apply: "应用",
    reapply: "重新应用",
    publish: "发布",
    upgrade_binding: "升级绑定",
    retire: "下架",
    expire: "设为失效",
    post: "记账",
    process: "处理",
    process_pending: "处理待办",
    operate: "操作",
    execute: "执行",
    command: "下发指令",
    writeback: "回写",
    complete: "完成",
    close: "关闭",
    resume: "恢复",
    reassign: "改派",
    resolve: "标记已解决",
    investigate: "排查",
    request_source_fix: "请求源端修复",
    cancel: "取消",
    cancel_approval: "撤回审批",
    cancel_blocked: "取消受阻审批",
    reject: "驳回",
    revoke: "撤销授权",
    reverse: "冲销",
    refund: "退款",
    void: "作废",
    delete: "删除",
    reveal: "查看敏感信息",
}

/** 矩阵列顺序：查看类在前、变更类居中、高风险动作垫后。 */
const ACTION_ORDER: readonly string[] = [
    "list",
    "detail",
    "read",
    "preview",
    "assignable_list",
    "create",
    "update",
    "edit",
    "update_role",
    "register",
    "submit",
    "decide",
    "review",
    "confirm",
    "confirm_requirement",
    "apply",
    "reapply",
    "publish",
    "upgrade_binding",
    "retire",
    "expire",
    "post",
    "process",
    "process_pending",
    "operate",
    "execute",
    "command",
    "writeback",
    "complete",
    "close",
    "resume",
    "reassign",
    "resolve",
    "investigate",
    "request_source_fix",
    "cancel",
    "cancel_approval",
    "cancel_blocked",
    "reject",
    "revoke",
    "reverse",
    "refund",
    "void",
    "delete",
    "reveal",
]

export function actionLabel(action: string): string {
    return ACTION_LABEL[action] ?? action
}

/**
 * 对象中文名手工口径。
 *
 * 其余对象由接口描述推导（见 deriveResourceLabel）；这里只覆盖推导不出、
 * 推导有歧义，或推导结果会把实现术语（投影 / 事实 / 水位）带进界面的对象。
 */
const RESOURCE_LABEL_OVERRIDES: Record<string, string> = {
    approval_instance: "审批实例",
    user_role: "用户角色",
    permission: "权限定义",
    data_scope: "数据范围",
    approval_process: "审批流程",
    work_item: "待办",
    customer_scope: "客户范围",
    customer_sensitive: "客户敏感信息",
    supplier_sensitive: "供应商敏感信息",
    receivable_funds_review: "回款复核",
    supplier_refund_fact: "供应商退款记录",
    integration_task: "集成任务",
    cost_entry: "成本记录",
    mall_order_fact: "商城订单关键信息",
    legacy_import_confirmation: "导入确认记录",
    sales_order_projection: "执行信息",
    sales_order_projection_revision: "执行信息版本",
    sales_order_projection_delivery: "执行信息下发记录",
    mall_sales_sync_cursor: "同步进度",
    supplier_settlement_source_evidence: "结算来源证据",
    bulk_selection_snapshot: "批量选择记录",
    bulk_selection_item: "批量选择逐项结果",
}

/**
 * 权限组名手工口径：生成的组名带实现术语时在这里改写。
 * 其余组名沿用生成结果。
 */
const GROUP_NAME_OVERRIDES: Record<string, string> = {
    执行投影: "执行信息",
}

/** 组描述清洗：去掉文档编号（W05 等），把实现术语换成业务语言。 */
function sanitizeGroupText(text: string): string {
    return text
        .replace(/（[^（）]*W\d+[^（）]*）|\([^()]*W\d+[^()]*\)/g, "")
        .replace(/投影/g, "执行信息")
        .replace(/事实/g, "记录")
        .trim()
}

/** 从接口描述里剥掉动词与「列表 / 详情」后缀，得到对象名。 */
function deriveResourceLabel(
    items: readonly { action: string; description: string }[],
): string | null {
    const pick = (action: string) =>
        items.find((item) => item.action === action)?.description
    const candidates: string[] = []
    const created = pick("create")
    if (created) {
        candidates.push(
            created.replace(/^(创建|新建|新增|提交|登记|注册|录入)/, ""),
        )
    }
    const listed = pick("list")
    if (listed) {
        candidates.push(
            listed
                .replace(/^(分页查询|查询|获取|列出|读取)/, "")
                .replace(/(列表|清单)$/, ""),
        )
    }
    const detailed = pick("detail")
    if (detailed) {
        candidates.push(
            detailed
                .replace(/^(查询|获取|读取)/, "")
                .replace(/(详情|明细)$/, ""),
        )
    }
    const updated = pick("update")
    if (updated) {
        candidates.push(
            updated.replace(/^(更新|修改|编辑)/, "").replace(/信息$/, ""),
        )
    }
    const usable = candidates
        .map((text) => text.trim())
        .filter((text) => text.length > 0 && text.length <= 12)
    if (usable.length === 0) return null
    usable.sort((a, b) => a.length - b.length)
    return usable[0]!
}

type RawPermission = {
    method: string
    path: string
    description: string
    resource: string
    action: string
}

/** 组内按权限编码去重：合并接口清单，保留最短的一条描述。 */
function dedupeGroupItems(
    permissions: readonly RawPermission[],
): PermissionItemOption[] {
    const byCode = new Map<string, PermissionItemOption>()
    for (const raw of permissions) {
        const code = `${raw.resource}:${raw.action}`
        const endpoint: PermissionEndpoint = {
            method: raw.method,
            path: raw.path,
        }
        const existing = byCode.get(code)
        if (!existing) {
            byCode.set(code, {
                code,
                resource: raw.resource,
                action: raw.action,
                description: raw.description,
                endpoints: [endpoint],
                dangerous: isDangerousAction(raw.action),
            })
            continue
        }
        byCode.set(code, {
            ...existing,
            description:
                raw.description.length < existing.description.length
                    ? raw.description
                    : existing.description,
            endpoints: [...existing.endpoints, endpoint],
        })
    }
    return [...byCode.values()]
}

const RAW_GROUPS = PERMISSION_GROUPS.map((group) => ({
    name: GROUP_NAME_OVERRIDES[group.name] ?? sanitizeGroupText(group.name),
    description: sanitizeGroupText(group.description),
    permissions: group.permissions.map((permission) => ({
        method: permission.method,
        path: permission.path,
        description: permission.description,
        resource: permission.permission.resource,
        action: permission.permission.action,
    })),
}))

/** 权限目录：按编码去重后的分组清单。 */
export const PERMISSION_CATALOG: readonly PermissionGroupOption[] =
    RAW_GROUPS.map((group) => ({
        name: group.name,
        description: group.description,
        items: dedupeGroupItems(group.permissions),
    }))

export const BUSINESS_GROUPS: readonly PermissionGroupOption[] =
    PERMISSION_CATALOG.filter((group) => !isSystemGroup(group.name))
export const SYSTEM_GROUPS: readonly PermissionGroupOption[] =
    PERMISSION_CATALOG.filter((group) => isSystemGroup(group.name))

/** 权限编码 → 目录条目。 */
export const PERMISSION_BY_CODE: ReadonlyMap<string, PermissionItemOption> =
    new Map(
        PERMISSION_CATALOG.flatMap((group) =>
            group.items.map((item) => [item.code, item] as const),
        ),
    )

/** 权限编码 → 所属权限组名。 */
export const GROUP_NAME_BY_CODE: ReadonlyMap<string, string> = new Map(
    PERMISSION_CATALOG.flatMap((group) =>
        group.items.map((item) => [item.code, group.name] as const),
    ),
)

const RESOURCE_LABELS: ReadonlyMap<string, string> = (() => {
    const byResource = new Map<string, RawPermission[]>()
    for (const group of RAW_GROUPS) {
        for (const permission of group.permissions) {
            const bucket = byResource.get(permission.resource)
            if (bucket) bucket.push(permission)
            else byResource.set(permission.resource, [permission])
        }
    }
    const labels = new Map<string, string>()
    for (const [resource, permissions] of byResource) {
        const override = RESOURCE_LABEL_OVERRIDES[resource]
        labels.set(resource, override ?? deriveResourceLabel(permissions) ?? resource)
    }
    return labels
})()

export function resourceLabel(resource: string): string {
    return RESOURCE_LABELS.get(resource) ?? resource
}

/**
 * 权限编码 → 界面文案，如 `customer:create` → 「客户 · 新建」。
 * 通配编码按「全部权限 / 全部动作」表述；目录外编码原样返回。
 */
export function permissionLabel(code: string): string {
    if (code === "*:*") return "全部权限"
    const item = PERMISSION_BY_CODE.get(code)
    if (item) {
        return `${resourceLabel(item.resource)} · ${actionLabel(item.action)}`
    }
    const [resource, action] = code.split(":")
    if (!resource || !action) return code
    const label = RESOURCE_LABELS.get(resource)
    if (!label) return code
    return action === "*" ? `${label} · 全部动作` : `${label} · ${actionLabel(action)}`
}

/** 矩阵一行：一个业务对象在本组内的全部动作。 */
export type PermissionMatrixRow = {
    resource: string
    label: string
    /** 与所在组的 actions 顺序一一对齐；该对象没有此动作时为 null。 */
    cells: readonly (PermissionItemOption | null)[]
    codes: readonly string[]
}

/** 矩阵一组：列为动作，行为业务对象。 */
export type PermissionMatrixGroup = {
    name: string
    description: string
    tab: PermissionPanelTab
    actions: readonly string[]
    rows: readonly PermissionMatrixRow[]
    codes: readonly string[]
}

function toMatrixGroup(group: PermissionGroupOption): PermissionMatrixGroup {
    const actions = [...new Set(group.items.map((item) => item.action))].sort(
        (a, b) => {
            const ai = ACTION_ORDER.indexOf(a)
            const bi = ACTION_ORDER.indexOf(b)
            if (ai === bi) return a.localeCompare(b)
            if (ai < 0) return 1
            if (bi < 0) return -1
            return ai - bi
        },
    )
    const resources = [...new Set(group.items.map((item) => item.resource))]
    const rows = resources.map((resource) => {
        const items = group.items.filter((item) => item.resource === resource)
        return {
            resource,
            label: resourceLabel(resource),
            cells: actions.map(
                (action) =>
                    items.find((item) => item.action === action) ?? null,
            ),
            codes: items.map((item) => item.code),
        }
    })
    return {
        name: group.name,
        description: group.description,
        tab: isSystemGroup(group.name) ? "system" : "business",
        actions,
        rows,
        codes: group.items.map((item) => item.code),
    }
}

/** 权限矩阵：面板渲染的唯一数据源。 */
export const PERMISSION_MATRIX: readonly PermissionMatrixGroup[] =
    PERMISSION_CATALOG.map(toMatrixGroup)

export function matrixGroupsForTab(
    tab: PermissionPanelTab,
): readonly PermissionMatrixGroup[] {
    return PERMISSION_MATRIX.filter((group) => group.tab === tab)
}

/** 关键词匹配：编码、描述、对象名、动作名与接口路径都参与匹配。 */
export function matchesKeyword(item: PermissionItemOption, q: string): boolean {
    if (!q) return true
    return [
        item.code,
        item.description,
        resourceLabel(item.resource),
        actionLabel(item.action),
        ...item.endpoints.map((endpoint) => endpoint.path),
    ]
        .join(" ")
        .toLowerCase()
        .includes(q)
}

/** 按关键词过滤组内权限项，仅保留含匹配项的组；空关键词返回原数组（引用不变）。 */
export function filterGroupsByKeyword(
    groups: readonly PermissionGroupOption[],
    q: string,
): readonly PermissionGroupOption[] {
    if (!q) return groups
    return groups
        .map((group) => ({
            ...group,
            items: group.items.filter((item) => matchesKeyword(item, q)),
        }))
        .filter((group) => group.items.length > 0)
}

/**
 * 按关键词过滤矩阵：命中项所在的行与列保留，其余剔除；空关键词返回原数组。
 * 组名或组描述命中时整组保留，便于按模块名定位。
 */
export function filterMatrixByKeyword(
    groups: readonly PermissionMatrixGroup[],
    q: string,
): readonly PermissionMatrixGroup[] {
    if (!q) return groups
    return groups
        .map((group) => {
            const groupHit = `${group.name} ${group.description}`
                .toLowerCase()
                .includes(q)
            if (groupHit) return group
            const hitCells = group.rows.flatMap((row) =>
                row.cells.filter(
                    (cell): cell is PermissionItemOption =>
                        cell !== null && matchesKeyword(cell, q),
                ),
            )
            if (hitCells.length === 0) return null
            const hitActions = new Set(hitCells.map((cell) => cell.action))
            const actions = group.actions.filter((action) =>
                hitActions.has(action),
            )
            const hitCodes = new Set(hitCells.map((cell) => cell.code))
            const rows = group.rows
                .map((row) => {
                    const cells = group.actions
                        .map((action, index) =>
                            actions.includes(action) ? row.cells[index]! : null,
                        )
                        .map((cell) =>
                            cell && hitCodes.has(cell.code) ? cell : null,
                        )
                    return {
                        ...row,
                        cells,
                        codes: cells
                            .filter(
                                (cell): cell is PermissionItemOption =>
                                    cell !== null,
                            )
                            .map((cell) => cell.code),
                    }
                })
                .filter((row) => row.codes.length > 0)
            if (rows.length === 0) return null
            return {
                ...group,
                actions,
                rows,
                codes: rows.flatMap((row) => row.codes),
            }
        })
        .filter((group): group is PermissionMatrixGroup => group !== null)
}

/** 统计各维度已选数量；不在目录中的编码忽略。 */
export function countSelectedByTab(
    selected: readonly string[],
): Record<PermissionPanelTab, number> {
    const counts: Record<PermissionPanelTab, number> = {
        business: 0,
        system: 0,
    }
    for (const code of selected) {
        const groupName = GROUP_NAME_BY_CODE.get(code)
        if (!groupName) continue
        counts[isSystemGroup(groupName) ? "system" : "business"] += 1
    }
    return counts
}

export type PermissionGroupCount = {
    name: string
    count: number
}

/** 角色权限摘要：按权限组归并计数，供列表列与已选面板复用。 */
export type PermissionSummary = {
    /** 是否为通配全权（`*:*`）。 */
    wildcard: boolean
    /** 在目录内的权限条数。 */
    total: number
    /** 目录外的编码数量（后端新增但前端目录未同步时不静默丢弃）。 */
    unknown: number
    /** 按条数降序的权限组。 */
    groups: readonly PermissionGroupCount[]
}

export function summarizePermissions(
    codes: readonly string[],
): PermissionSummary {
    const wildcard = codes.some((code) => code === "*:*")
    const counts = new Map<string, number>()
    let total = 0
    let unknown = 0
    for (const code of codes) {
        if (code === "*:*") continue
        const groupName = GROUP_NAME_BY_CODE.get(code)
        if (!groupName) {
            unknown += 1
            continue
        }
        total += 1
        counts.set(groupName, (counts.get(groupName) ?? 0) + 1)
    }
    const groups = [...counts.entries()]
        .map(([name, count]) => ({ name, count }))
        .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name))
    return { wildcard, total, unknown, groups }
}

/** 已选编码按组归并，供「已选摘要」逐组展开。 */
export function selectedItemsByGroup(
    selected: readonly string[],
): readonly { name: string; items: readonly PermissionItemOption[] }[] {
    const byGroup = new Map<string, PermissionItemOption[]>()
    for (const code of selected) {
        const groupName = GROUP_NAME_BY_CODE.get(code)
        const item = PERMISSION_BY_CODE.get(code)
        if (!groupName || !item) continue
        const bucket = byGroup.get(groupName)
        if (bucket) bucket.push(item)
        else byGroup.set(groupName, [item])
    }
    return PERMISSION_CATALOG.filter((group) => byGroup.has(group.name)).map(
        (group) => ({
            name: group.name,
            items: byGroup.get(group.name)!,
        }),
    )
}
