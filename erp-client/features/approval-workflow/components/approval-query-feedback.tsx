"use client"

import { BusinessFailureState } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"

type FailedQuery = {
    isError: boolean
    error: unknown
    refetch: () => Promise<unknown>
    isFetchNextPageError?: boolean
    fetchNextPage?: () => Promise<unknown>
}

/** 审批辅助读取失败保留已有内容，并明确区分历史与恢复操作的重试入口。 */
export function ApprovalQueryFeedback({
    surface,
    instanceId,
    historyQuery,
    recoveryQuery,
}: {
    surface: string
    instanceId?: string
    historyQuery?: FailedQuery
    recoveryQuery?: FailedQuery
}) {
    if (!instanceId) return null
    const prefix = `${surface}-${toAutomationIdSegment(instanceId)}`
    return (
        <>
            {historyQuery?.isError ? (
                <BusinessFailureState
                    id={`${prefix}-history-failure`}
                    title="审批历史读取失败，已显示的记录可能不完整"
                    error={historyQuery.error}
                    onRetry={() => {
                        if (
                            historyQuery.isFetchNextPageError &&
                            historyQuery.fetchNextPage
                        ) {
                            void historyQuery.fetchNextPage()
                        } else {
                            void historyQuery.refetch()
                        }
                    }}
                />
            ) : null}
            {recoveryQuery?.isError ? (
                <BusinessFailureState
                    id={`${prefix}-recovery-failure`}
                    title="审批恢复操作读取失败，暂无法确认可用操作"
                    error={recoveryQuery.error}
                    onRetry={() => void recoveryQuery.refetch()}
                />
            ) : null}
        </>
    )
}
