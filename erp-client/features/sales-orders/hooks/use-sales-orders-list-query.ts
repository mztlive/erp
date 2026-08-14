import * as React from "react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useSalesOrdersQuery } from "@/features/sales-orders/hooks/queries"
import {
    buildSalesOrdersListQuery,
    salesOrdersListIdentityReady,
} from "@/features/sales-orders/lib/sales-orders-list-query"
import type { SalesOrdersUrlState } from "@/features/sales-orders/lib/url-state"

/** 列表页查询派生：URL 状态 + 登录人身份 → 列表查询（含身份就绪门控）。 */
export function useSalesOrdersListQuery(url: SalesOrdersUrlState) {
    const profileQuery = useAccountProfileQuery()
    const currentUserId = profileQuery.data?.userid?.trim() ?? ""

    const query = React.useMemo(
        () => buildSalesOrdersListQuery(url, currentUserId),
        [url, currentUserId],
    )
    const identityReady = salesOrdersListIdentityReady(url, currentUserId)
    const ordersQuery = useSalesOrdersQuery(query, identityReady)

    return { ordersQuery, query, currentUserId, identityReady }
}
