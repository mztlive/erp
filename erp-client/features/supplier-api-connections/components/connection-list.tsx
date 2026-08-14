"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    FormalActionResult,
    GuardedBusinessAction,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { ConnectionCreateDialog } from "@/features/supplier-api-connections/components/connection-create-dialog"
import { ConnectionListTable } from "@/features/supplier-api-connections/components/connection-list-table"
import { ConnectionListToolbar } from "@/features/supplier-api-connections/components/connection-list-toolbar"
import { ConnectionMetricStrip } from "@/features/supplier-api-connections/components/connection-metric-strip"
import { useConnectionListQuery } from "@/features/supplier-api-connections/hooks/queries"
import { useConnectionListColumns } from "@/features/supplier-api-connections/hooks/use-connection-list-columns"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"
import { formatDateTime } from "@/lib/datetime"

export function ConnectionList({
    urlState,
    patchUrl,
    onOpen,
}: {
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    onOpen: (connectionId: string) => void
}) {
    const [searchDraft, setSearchDraft] = React.useState(urlState.q ?? "")
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<
        (ResultState & { actions?: React.ReactNode }) | null
    >(null)

    React.useEffect(() => {
        setSearchDraft(urlState.q ?? "")
    }, [urlState.q])

    const listQuery = useConnectionListQuery({
        environment: urlState.environment,
        status: urlState.status,
        health: urlState.health,
        capability: urlState.capability,
        catalogFreshness: urlState.catalogFreshness,
        supplierId: urlState.supplierId,
        q: urlState.q,
        page: urlState.page,
        pageSize: urlState.pageSize,
    })

    const data = listQuery.data

    // D7：常驻/空态清除 = 清全部筛选参数并回第 1 页；environment 属视图类参数按 P4 保留，
    // 语义通过按钮 title/aria 说明。status/health/catalogFreshness 为逗号分隔多值串
    // （codec array 语义自洽），保持不变。
    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        patchUrl({
            q: undefined,
            status: undefined,
            health: undefined,
            catalogFreshness: undefined,
            capability: undefined,
            supplierId: undefined,
            page: 1,
        })
    }, [patchUrl])

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: Math.max(0, urlState.page - 1),
        pageSize: urlState.pageSize,
    })

    React.useEffect(() => {
        setPagination((p) => ({
            ...p,
            pageIndex: Math.max(0, urlState.page - 1),
            pageSize: urlState.pageSize,
        }))
    }, [urlState.page, urlState.pageSize])

    const columns = useConnectionListColumns(onOpen)

    if (listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-72 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (listQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="API 供应商连接" description="加载失败" />
                <BusinessFailureState
                    title="连接列表加载失败"
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="API 供应商连接"
                breadcrumbs={[
                    {
                        id: "api",
                        label: "供应商 API",
                        href: "/supplier-api/connections",
                    },
                    { id: "conn", label: "API 连接", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.projectedAt
                                ? formatDateTime(data.projectedAt, "default")
                                : "—"
                        }
                        dateTime={data?.projectedAt}
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                        label="连接列表"
                    />
                }
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground hover:text-foreground"
                            onClick={() => void listQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            刷新
                        </Button>
                        <div className="max-sm:hidden">
                            <GuardedBusinessAction
                                type="button"
                                size="sm"
                                disabled={!data?.hasModulePermission}
                                reason={
                                    data?.hasModulePermission
                                        ? undefined
                                        : "当前账号无模块权限"
                                }
                                onClick={() => setCreateOpen(true)}
                            >
                                <PlusIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                新建连接
                            </GuardedBusinessAction>
                        </div>
                    </div>
                }
            />

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed"
                            ? "rejected"
                            : result.status === "processing"
                              ? "processing"
                              : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={result.actions}
                />
            ) : null}

            {/* D7：空态不再隐藏筛选区——MetricStrip 与 ListToolbar 常驻，仅表格区切换空态 */}
            <ConnectionMetricStrip
                data={data}
                urlState={urlState}
                patchUrl={patchUrl}
            />

            <BusinessTableFrame
                title="连接列表"
                description="一行展示代码、供应商、环境、状态、能力、健康与下一步；身份与操作列固定；默认仅展示生产环境连接，可在工具栏切换。"
                toolbar={
                    <ConnectionListToolbar
                        urlState={urlState}
                        patchUrl={patchUrl}
                        searchDraft={searchDraft}
                        onSearchDraftChange={setSearchDraft}
                        onClearFilters={clearFilters}
                    />
                }
                table={
                    <ConnectionListTable
                        data={data}
                        columns={columns}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            patchUrl({
                                page: next.pageIndex + 1,
                                pageSize: next.pageSize,
                            })
                        }}
                        onRowOpen={onOpen}
                        onClearFilters={clearFilters}
                        onCreate={() => setCreateOpen(true)}
                    />
                }
            />

            <ConnectionCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                onOpen={onOpen}
                onResult={setResult}
            />
        </PageScaffold>
    )
}
