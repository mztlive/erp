/**
 * 选品册菜单角标：还需要销售处理的状态集合。
 */

import type { BookletStatus } from "@/features/sales-selection/types"

/** 草稿、准备中、待发布均算待处理；已发布起交给客户或已结束。 */
export const ACTIONABLE_BOOK_STATUSES = [
    "DRAFT",
    "PREPARING",
    "PENDING_PUBLISH",
] as const satisfies readonly BookletStatus[]

export type ActionableBookStatus = (typeof ACTIONABLE_BOOK_STATUSES)[number]

/**
 * 判断选品册是否仍需销售处理。
 * @param status 选品册状态
 */
export function isActionableBookStatus(
    status: BookletStatus,
): status is ActionableBookStatus {
    return (ACTIONABLE_BOOK_STATUSES as readonly BookletStatus[]).includes(
        status,
    )
}

/**
 * 把各待处理状态的总数加总为角标数字。
 * @param totals 与 ACTIONABLE_BOOK_STATUSES 对齐的各状态 total
 */
export function sumActionableBookCounts(totals: readonly number[]): number {
    return totals.reduce((sum, total) => sum + total, 0)
}
