import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

import {
    mergeSalesOrdersSearchParams,
    normalizedSalesOrdersSearchParams,
    parseSalesOrdersSearchParams,
    type SalesOrdersUrlState,
} from "@/features/sales-orders/lib/url-state"

/** 列表页 URL 单一数据源：解析当前参数、按补丁写回并归一化非法参数。 */
export function useSalesOrdersListUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions
    const sanitize = React.useCallback(
        (state: SalesOrdersUrlState) => {
            const next = { ...state }
            if (!hasPermission(permissions, "customer:list"))
                next.customerId = undefined
            if (!hasPermission(permissions, "contract:list"))
                next.contractId = undefined
            if (!hasPermission(permissions, "business_person:list"))
                next.createdBy = undefined
            if (!hasPermission(permissions, "sales_person:list"))
                next.ownerUserIds = undefined
            if (!hasPermission(permissions, "org_unit:list")) {
                next.orgUnitIds = undefined
                next.includeDescendants = false
            }
            if (
                next.customerId !== state.customerId ||
                next.contractId !== state.contractId ||
                next.createdBy !== state.createdBy ||
                next.ownerUserIds !== state.ownerUserIds ||
                next.orgUnitIds !== state.orgUnitIds ||
                next.includeDescendants !== state.includeDescendants
            ) {
                next.page = 1
            }
            return next
        },
        [permissions],
    )
    const url = React.useMemo(
        () => sanitize(parseSalesOrdersSearchParams(searchParams)),
        [sanitize, searchParams],
    )

    const pushUrl = React.useCallback(
        (patch: Partial<SalesOrdersUrlState>) => {
            const next = sanitize({ ...url, ...patch })
            const qs = mergeSalesOrdersSearchParams(searchParams, next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router, sanitize, searchParams, url],
    )

    React.useEffect(() => {
        if (!account.data) return
        const normalized = normalizedSalesOrdersSearchParams(searchParams, url)
        if (normalized === undefined) return
        router.replace(`${pathname}${normalized}`, { scroll: false })
    }, [account.data, pathname, router, searchParams, url])

    return { url, pushUrl }
}
