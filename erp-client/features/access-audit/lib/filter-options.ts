/**
 * W19 权限与审计 · 筛选字段的固定选项、URL 解析与业务文案。
 *
 * 解析器只接受页面已声明的枚举值：URL 中的非法值一律降级为默认（"all"），
 * 不得把未声明的值继续传给列表查询（docs/ui-filter-design.md §5.1 / §6.1）。
 */

export type AccessStatusFilterValue = "enabled" | "disabled"

export type AccessRiskFilterValue =
    | "HIGH_PRIVILEGE"
    | "EMPTY_SCOPE"
    | "EXPIRING_SOON"
    | "ACCESS_ADMIN"

export type AccessResultFilterValue = "SUCCESS" | "DENIED" | "FAILED" | "UNKNOWN"

export type AccessActionFilterValue =
    | "UPDATE_ROLE_PERMISSIONS"
    | "EMERGENCY_REVOKE_USER_ROLE"
    | "UPDATE_FIELD_POLICY"
    | "MANAGE_DATA_SCOPE"
    | "QUERY_AUDIT"
    | "OPEN_SUPPLIER"
    | "EXPORT_RECEIVABLE"
    | "CREATE_ADJUSTMENT"
    | "VIEW_CUSTOMER_SENSITIVE"
    | "PERMISSION_VERSION_BUMP"

export const STATUS_FILTER_VALUES: readonly AccessStatusFilterValue[] = [
    "enabled",
    "disabled",
]

export const RISK_FILTER_VALUES: readonly AccessRiskFilterValue[] = [
    "HIGH_PRIVILEGE",
    "EMPTY_SCOPE",
    "EXPIRING_SOON",
    "ACCESS_ADMIN",
]

export const RESULT_FILTER_VALUES: readonly AccessResultFilterValue[] = [
    "SUCCESS",
    "DENIED",
    "FAILED",
    "UNKNOWN",
]

export const ACTION_FILTER_VALUES: readonly AccessActionFilterValue[] = [
    "UPDATE_ROLE_PERMISSIONS",
    "EMERGENCY_REVOKE_USER_ROLE",
    "UPDATE_FIELD_POLICY",
    "MANAGE_DATA_SCOPE",
    "QUERY_AUDIT",
    "OPEN_SUPPLIER",
    "EXPORT_RECEIVABLE",
    "CREATE_ADJUSTMENT",
    "VIEW_CUSTOMER_SENSITIVE",
    "PERMISSION_VERSION_BUMP",
]

const ACTION_FILTER_LABELS: Record<AccessActionFilterValue, string> = {
    UPDATE_ROLE_PERMISSIONS: "修改模块权限",
    EMERGENCY_REVOKE_USER_ROLE: "紧急撤权",
    UPDATE_FIELD_POLICY: "修改字段策略",
    MANAGE_DATA_SCOPE: "修改数据范围",
    QUERY_AUDIT: "查询审计",
    OPEN_SUPPLIER: "打开供应商",
    EXPORT_RECEIVABLE: "导出应收明细",
    CREATE_ADJUSTMENT: "创建库存调整",
    VIEW_CUSTOMER_SENSITIVE: "短时揭示敏感字段",
    PERMISSION_VERSION_BUMP: "权限版本推进",
}

const STATUS_FILTER_LABELS: Record<AccessStatusFilterValue, string> = {
    enabled: "启用",
    disabled: "停用",
}

const RISK_FILTER_LABELS: Record<AccessRiskFilterValue, string> = {
    HIGH_PRIVILEGE: "高权限",
    EMPTY_SCOPE: "空数据范围",
    EXPIRING_SOON: "即将过期",
    ACCESS_ADMIN: "权限管理",
}

const RESULT_FILTER_LABELS: Record<AccessResultFilterValue, string> = {
    SUCCESS: "成功",
    DENIED: "拒绝",
    FAILED: "失败",
    UNKNOWN: "未知",
}

/** 固定单选（固定枚举行）：状态。 */
export const STATUS_FILTER_RADIO_OPTIONS = [
    { value: "all", label: "全部" },
    ...STATUS_FILTER_VALUES.map((value) => ({
        value,
        label: STATUS_FILTER_LABELS[value],
    })),
] as const

/** 固定单选（固定枚举行）：权限风险。 */
export const RISK_FILTER_RADIO_OPTIONS = [
    { value: "all", label: "全部" },
    ...RISK_FILTER_VALUES.map((value) => ({
        value,
        label: RISK_FILTER_LABELS[value],
    })),
] as const

/** 固定单选（固定枚举行）：审计结果。 */
export const RESULT_FILTER_RADIO_OPTIONS = [
    { value: "all", label: "全部结果" },
    ...RESULT_FILTER_VALUES.map((value) => ({
        value,
        label: RESULT_FILTER_LABELS[value],
    })),
] as const

/** 可搜索单选（网格字段）：动作。 */
export const ACTION_FILTER_OPTIONS = [
    { value: "all", label: "全部动作" },
    ...ACTION_FILTER_VALUES.map((value) => ({
        value,
        label: ACTION_FILTER_LABELS[value],
    })),
] as const

/** URL 解析：非法或缺失的枚举降级为默认（"all"），不进入查询。 */
function parseFilterValue<Value extends string>(
    raw: string | null,
    values: readonly Value[],
): Value | "all" {
    return values.find((value) => value === raw) ?? "all"
}

export function parseStatusFilter(
    raw: string | null,
): AccessStatusFilterValue | "all" {
    return parseFilterValue(raw, STATUS_FILTER_VALUES)
}

export function parseRiskFilter(raw: string | null): AccessRiskFilterValue | "all" {
    return parseFilterValue(raw, RISK_FILTER_VALUES)
}

export function parseResultFilter(
    raw: string | null,
): AccessResultFilterValue | "all" {
    return parseFilterValue(raw, RESULT_FILTER_VALUES)
}

export function parseActionFilter(
    raw: string | null,
): AccessActionFilterValue | "all" {
    return parseFilterValue(raw, ACTION_FILTER_VALUES)
}

export function statusFilterLabel(value: AccessStatusFilterValue): string {
    return STATUS_FILTER_LABELS[value]
}

export function riskFilterLabel(value: AccessRiskFilterValue): string {
    return RISK_FILTER_LABELS[value]
}

export function resultFilterLabel(value: AccessResultFilterValue): string {
    return RESULT_FILTER_LABELS[value]
}

export function actionFilterLabel(value: AccessActionFilterValue): string {
    return ACTION_FILTER_LABELS[value]
}

/** 审计时间范围校验：提交时执行，失败不写 URL。 */
export function auditDateRangeError(from: string, to: string): string | null {
    if (from && to && from > to) {
        return "截止日期不能早于起始日期"
    }
    return null
}
