import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

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

    const url = React.useMemo(
        () => parseSalesOrdersSearchParams(searchParams),
        [searchParams],
    )

    const pushUrl = React.useCallback(
        (patch: Partial<SalesOrdersUrlState>) => {
            const next = { ...url, ...patch }
            const qs = mergeSalesOrdersSearchParams(searchParams, next)
            router.replace(`${pathname}${qs}`, { scroll: false })
        },
        [pathname, router, searchParams, url],
    )

    React.useEffect(() => {
        const normalized = normalizedSalesOrdersSearchParams(searchParams, url)
        if (normalized === undefined) return
        router.replace(`${pathname}${normalized}`, { scroll: false })
    }, [pathname, router, searchParams, url])

    return { url, pushUrl }
}
