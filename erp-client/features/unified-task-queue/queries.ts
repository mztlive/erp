"use client"

/**
 * W02 统一待办队列 · Query/Mutation hooks 稳定入口。
 * 实现见 hooks/queries；本文件只做再导出。
 */

export {
    unifiedQueueKeys,
    useBatchClaimWorkItemMutation,
    useClaimWorkItemMutation,
    useCloseWorkItemMutation,
    useCompleteWorkItemMutation,
    useTransferWorkItemMutation,
    useUnifiedTaskCountQuery,
    useUnifiedTaskQueueQuery,
    useWorkItemActionMutation,
    WorkItemApiError,
} from "@/features/unified-task-queue/hooks/queries"
