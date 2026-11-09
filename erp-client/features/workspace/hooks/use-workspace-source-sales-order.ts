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
    error: Error | null
    refetch: () => unknown
} {
    const fromBrief = findSourceSalesOrder(item.summarySections)
    const isPurchase =
        normalizeObjectType(item.businessObjectType) === "purchase_order"
    const needsCenter =
        item.workItemType !== "DOCUMENT_APPROVAL" &&
        item.workItemType !== "APPROVAL_INSTANCE" &&
        item.workItemType !== "APPROVAL_REWORK" &&
        isPurchase &&
        !fromBrief?.objectId &&
        Boolean(item.businessObjectId.trim())
    const query = usePurchaseOrderCenterQuery(
        needsCenter ? item.businessObjectId : "",
    )
    const center = needsCenter && !query.isError ? query.data : undefined
    const matching =
        !center ||
        (center.identity.purchaseOrderId === item.businessObjectId &&
            [
                center.identity.currentRevisionId,
                center.identity.currentSubmissionId,
                String(center.currentContent.version),
            ].includes(item.subjectVersion))
    const error =
        query.error ??
        (!matching
            ? new Error("关联采购资料版本已变化，请刷新任务后重试")
            : null)
    const header = matching ? center?.header : undefined
    const orderNo = header?.salesOrderNo?.trim() || fromBrief?.orderNo
    const objectId = header?.salesOrderId?.trim() || fromBrief?.objectId
    if (!orderNo) {
        return {
            source: null,
            error,
            refetch: query.refetch,
            isPending: needsCenter && query.isPending,
        }
    }
    return {
        source: { orderNo, objectId: objectId || undefined },
        error,
        refetch: query.refetch,
        isPending: false,
    }
}
