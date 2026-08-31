"use client"

import { RefreshCwIcon } from "lucide-react"

import { DataFreshness, PageActions, PageHeader } from "@/components/business"
import { Spinner } from "@/components/ui/spinner"
import { formatDateTime } from "@/lib/datetime"

export function ExecutionProjectionPageHeader({
    queriedAt,
    isFetching,
    onRefresh,
    selectedCount,
    bulkOverLimit,
    bulkPending,
    onBulkQuery,
    onBulkRetry,
}: {
    queriedAt: string | undefined
    isFetching: boolean
    onRefresh: () => void
    selectedCount: number
    bulkOverLimit: boolean
    bulkPending: boolean
    onBulkQuery: () => void
    onBulkRetry: () => void
}) {
    return (
        <PageHeader
            title="执行信息"
            metadata={
                <DataFreshness
                    updatedAt={
                        queriedAt
                            ? formatDateTime(
                                  queriedAt,
                                  "monthDay",
                                  "passthrough",
                              )
                            : "—"
                    }
                    dateTime={queriedAt}
                    state={isFetching ? "syncing" : "fresh"}
                    label="发送状态更新于"
                />
            }
            actions={
                <PageActions
                    actions={[
                        {
                            actionKey: "refresh",
                            id: "execution-projections-page-header-refresh",
                            label: "刷新",
                            icon: RefreshCwIcon,
                            variant: "ghost",
                            onClick: onRefresh,
                        },
                        {
                            actionKey: "bulk-query",
                            id: "execution-projections-page-header-bulk-query",
                            label: bulkPending ? (
                                <>
                                    <Spinner
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    处理中…
                                </>
                            ) : (
                                "批量查询"
                            ),
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled:
                                selectedCount === 0 ||
                                bulkOverLimit ||
                                bulkPending,
                            onClick: onBulkQuery,
                        },
                        {
                            actionKey: "bulk-retry",
                            id: "execution-projections-page-header-bulk-retry",
                            label: bulkPending ? (
                                <>
                                    <Spinner
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    处理中…
                                </>
                            ) : (
                                "批量重试"
                            ),
                            mobileVisibility: "hide",
                            disabled:
                                selectedCount === 0 ||
                                bulkOverLimit ||
                                bulkPending,
                            onClick: onBulkRetry,
                        },
                    ]}
                />
            }
        />
    )
}
