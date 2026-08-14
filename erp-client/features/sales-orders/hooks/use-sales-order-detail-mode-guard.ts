"use client"

import * as React from "react"

import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    isSalesOrderDraft,
    rejectionAllowsResubmit,
} from "@/features/sales-orders/lib/sales-order-detail-model"

/**
 * `mode=edit` 只对「采购驳回后可改完再报」或草稿单有意义；
 * 其它状态下进入时清掉该参数，回到普通详情视图。
 */
export function useSalesOrderDetailModeGuard({
    order,
    pageMode,
    replaceOrderHref,
}: {
    order: SalesOrderDetailView | null | undefined
    pageMode: string | null
    replaceOrderHref: (patch: {
        section?: string
        mode?: string | null
    }) => void
}) {
    const canResubmit = Boolean(order && rejectionAllowsResubmit(order))

    React.useEffect(() => {
        if (!order) return
        if (pageMode !== "edit") return
        if (canResubmit || isSalesOrderDraft(order)) return
        replaceOrderHref({ mode: null })
    }, [canResubmit, order, pageMode, replaceOrderHref])
}
