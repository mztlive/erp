/** 资金往来范围分页 wire 形态（M07/M08/M09 共用，后端 snake_case）。 */

export type FundsOwnerOption = Readonly<{
    value: string
    label: string
}>

export type FundsPersonShareWire = Readonly<{
    owner_user_id: string
    visible_share: string
}>

export type FundsSummaryWire = Readonly<{
    grouped: readonly FundsPersonShareWire[]
    unassigned: string
    whole_total: string | null
    permission_limited: boolean
    scope_version: string
}>

export type FundsScopedPageWire<T> = Readonly<{
    items: readonly T[]
    total: number
    summary: FundsSummaryWire
    owner_options: readonly FundsOwnerOption[]
    page: number
    page_size: number
    scope_version: string
    policy_version: number
    organization_version: number
    as_of: string
    empty_reason: string | null
    scope_summary: string
    ownership_basis: string
}>

export type FundsScopedResultWire<T> = Readonly<{
    data: T
    scope_version: string
    policy_version: number
    organization_version: number
    as_of: string
    empty_reason: string | null
    scope_summary: string
    ownership_basis: string
}>

/** 范围版本冲突（跨页或导出重验失败）；调用方须回到第一页重新查询。 */
export function isScopeChangedError(error: unknown): boolean {
    if (typeof error !== "object" || error === null) return false
    const code =
        "code" in error && typeof error.code === "string"
            ? error.code
            : undefined
    if (code === "DATA_SCOPE_CHANGED") return true
    const message =
        "message" in error && typeof error.message === "string"
            ? error.message
            : ""
    return message.includes("DATA_SCOPE_CHANGED")
}

/** URL 中的逗号分隔稳定 ID 列表；空值返回 undefined（参数缺省）。 */
export function parseScopeIdList(raw: string | null): string | undefined {
    const value = raw?.trim()
    return value ? value : undefined
}

/** 已选 ID 数量（逗号分隔），用于已生效条件标签。 */
export function countScopeIds(value: string | undefined): number {
    if (!value) return 0
    return value.split(",").filter((part) => part.trim().length > 0).length
}
