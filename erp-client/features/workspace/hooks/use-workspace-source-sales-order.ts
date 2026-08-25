"use client"

import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { normalizeObjectType } from "@/features/workspace/lib/document-facts"
import {
    findSourceSalesOrder,
    type SourceSalesOrderRef,
} from "@/features/workspace/lib/source-sales-order"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * 采购审核任务的来源销售单。简报已带身份时不再补拉对象中心。
 */
export function useWorkspaceSourceSalesOrder(item: WorkspaceWorkItem): {
    source: SourceSalesOrderRef | null
    isPending: boolean
} {
    const fromBrief = findSourceSalesOrder(item.summarySections)
    const isPurchase =
        normalizeObjectType(item.businessObjectType) === "purchase_order"
    const needsCenter =
        isPurchase &&
        !fromBrief?.objectId &&
        Boolean(item.businessObjectId.trim())
    const query = usePurchaseOrderCenterQuery(
        needsCenter ? item.businessObjectId : "",
    )
    const header = query.data?.header
    const orderNo = header?.salesOrderNo?.trim() || fromBrief?.orderNo
    const objectId = header?.salesOrderId?.trim() || fromBrief?.objectId
    if (!orderNo) {
        return {
            source: null,
            isPending: needsCenter && query.isPending,
        }
    }
    return {
        source: { orderNo, objectId: objectId || undefined },
        isPending: false,
    }
}
