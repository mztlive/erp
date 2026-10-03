"use client"

import { useQuery, useQueryClient } from "@tanstack/react-query"
import { approvalKeys } from "@/features/approval-workflow/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchPurchaseChangeDraft,
    fetchPurchaseChangeSubmissionState,
} from "../api/purchase-change-draft"

/** 编辑只读取原变更单的完整冻结内容。 */
export function usePurchaseChangeDraftQuery(
    changeOrderId: string,
    enabled: boolean,
) {
    return useQuery({
        queryKey: ["purchase-orders", "change-draft", changeOrderId],
        queryFn: () => fetchPurchaseChangeDraft(changeOrderId),
        enabled: enabled && Boolean(changeOrderId),
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
}

/** 按需核对原变更单，确认提交后刷新审批及业务缓存。 */
export function usePurchaseChangeSubmissionQuery(changeOrderId: string) {
    const client = useQueryClient()
    const query = useQuery({
        queryKey: ["purchase-orders", "change-submission-state", changeOrderId],
        queryFn: () => fetchPurchaseChangeSubmissionState(changeOrderId),
        enabled: false,
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
    const confirm = async () => {
        await Promise.all([
            client.invalidateQueries({ queryKey: ["purchase-orders"] }),
            client.invalidateQueries({ queryKey: approvalKeys.all }),
            client.invalidateQueries({ queryKey: workItemKeys.all }),
            client.invalidateQueries({ queryKey: queryKeyRoots.workspaceHome }),
            client.invalidateQueries({ queryKey: queryKeyRoots.salesOrders }),
        ])
    }
    return { query, confirm }
}
