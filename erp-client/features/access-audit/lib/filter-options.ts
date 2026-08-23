/**
 * 审计查询 · 筛选字段的固定选项、URL 解析与业务文案。
 *
 * 解析器只接受页面已声明的取值：URL 中的非法值一律降级为默认（"all"），
 * 不得把未声明的值继续传给列表查询（docs/ui-filter-design.md §5.1 / §6.1）。
 *
 * 角色 / 用户授权侧不提供结构化筛选：后端目前没有组织、账号状态与风险标记，
 * 提供只会筛出空列表，故只保留关键词搜索。
 */

import { auditActionLabel } from "@/features/access-audit/lib/audit-labels"

export type AccessResultFilterValue = "SUCCESS" | "DENIED" | "FAILED" | "UNKNOWN"

/** 审计动作取值即后端 action_type（`<对象>.<动作>`），选项由查询结果实时归纳。 */
export type AccessActionFilterValue = string

export const RESULT_FILTER_VALUES: readonly AccessResultFilterValue[] = [
    "SUCCESS",
    "DENIED",
    "FAILED",
    "UNKNOWN",
]

const RESULT_FILTER_LABELS: Record<AccessResultFilterValue, string> = {
    SUCCESS: "成功",
    DENIED: "拒绝",
    FAILED: "失败",
    UNKNOWN: "未知",
}

/** 固定单选（固定枚举行）：审计结果。 */
export const RESULT_FILTER_RADIO_OPTIONS = [
    { value: "all", label: "全部结果" },
    ...RESULT_FILTER_VALUES.map((value) => ({
        value,
        label: RESULT_FILTER_LABELS[value],
    })),
] as const

/** URL 解析：非法或缺失的枚举降级为默认（"all"），不进入查询。 */
function parseFilterValue<Value extends string>(
    raw: string | null,
    values: readonly Value[],
): Value | "all" {
    return values.find((value) => value === raw) ?? "all"
}

export function parseResultFilter(
    raw: string | null,
): AccessResultFilterValue | "all" {
    return parseFilterValue(raw, RESULT_FILTER_VALUES)
}

/** 动作取值形如 `user_role.assign`；不符合该形状的一律降级。 */
export function parseActionFilter(
    raw: string | null,
): AccessActionFilterValue | "all" {
    const value = raw?.trim()
    if (!value) return "all"
    return /^[a-z0-9_]+\.[a-z0-9_]+$/.test(value) ? value : "all"
}

export function resultFilterLabel(value: AccessResultFilterValue): string {
    return RESULT_FILTER_LABELS[value]
}

export function actionFilterLabel(value: AccessActionFilterValue): string {
    return auditActionLabel(value)
}

/** 审计时间范围校验：提交时执行，失败不写 URL。 */
export function auditDateRangeError(from: string, to: string): string | null {
    if (from && to && from > to) {
        return "截止日期不能早于起始日期"
    }
    return null
}
