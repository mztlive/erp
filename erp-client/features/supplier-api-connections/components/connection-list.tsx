"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { PlusIcon, RefreshCwIcon } from "lucide-react"

import {
    FormalActionResult,
    GuardedBusinessAction,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import type { ResultState } from "@/components/business/feedback"
import { Button } from "@/components/ui/button"
import { ConnectionCreateDialog } from "@/features/supplier-api-connections/components/connection-create-dialog"
import { ConnectionListTable } from "@/features/supplier-api-connections/components/connection-list-table"
import { ConnectionListToolbar } from "@/features/supplier-api-connections/components/connection-list-toolbar"
import {
    buildConnectionAppliedChips,
    useConnectionListFilters,
} from "@/features/supplier-api-connections/hooks/use-connection-list-filters"
import { useConnectionListQuery } from "@/features/supplier-api-connections/hooks/queries"
import { useConnectionListColumns } from "@/features/supplier-api-connections/hooks/use-connection-list-columns"
import type { ConnectionsUrlState } from "@/features/supplier-api-connections/lib/url-state"

export function ConnectionList({
    urlState,
    patchUrl,
    onOpen,
}: {
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    onOpen: (connectionId: string) => void
}) {
    const filters = useConnectionListFilters(urlState, patchUrl)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<
        (ResultState & { actions?: React.ReactNode }) | null
    >(null)

    // 查询只消费 Applied（URL 派生且已降级非法枚举值），Draft 变化不请求（§5、§6.3）
    const listQuery = useConnectionListQuery({
        environment: urlState.environment,
        status: filters.applied.status,
        health:
            filters.applied.health.length > 0
                ? filters.applied.health.join(",")
                : undefined,
        capability: filters.applied.capability,
        catalogFreshness:
            filters.applied.catalogFreshness.length > 0
                ? filters.applied.catalogFreshness.join(",")
                : undefined,
        supplierId: filters.applied.supplierId,
        q: filters.applied.q,
        page: urlState.page,
        pageSize: urlState.pageSize,
    })

    const data = listQuery.data

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

    // chip 展示供应商业务名称，不展示内部 ID（§4.5）
    const supplierNameLabel = React.useMemo(
        () =>
            data?.items.find(
                (item) => item.supplier.id === filters.applied.supplierId,
            )?.supplier.name,
        [data?.items, filters.applied.supplierId],
    )
    const appliedChips = React.useMemo(
        () => buildConnectionAppliedChips(urlState, supplierNameLabel),
        [supplierNameLabel, urlState],
    )
    const noStatusOrHealthView =
        !urlState.status && !urlState.health && !urlState.catalogFreshness

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="供应商"
                title="API 供应商连接"
                description="查看供应商接口连接与健康状态。"
            >
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="supplier-api-connections-list-refresh"
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
                            id="supplier-api-connections-list-create"
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
                            <PlusIcon className="size-3.5" aria-hidden="true" />
                            新建连接
                        </GuardedBusinessAction>
                    </div>
                </div>
            </ListWorkspaceHeader>

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

            <ListWorkSurface
                ariaLabel="API 供应商连接列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="连接状态与健康视图"
                        hint={
                            filters.appliedFilterLabels.length > 0
                                ? `筛选条件：${filters.appliedFilterLabels.join("、")}`
                                : "选择连接查看详情"
                        }
                        items={[
                            {
                                id: "supplier-api-connections-view-all",
                                label: "全部连接",
                                count: data?.total ?? 0,
                                active: noStatusOrHealthView,
                                onClick: () =>
                                    patchUrl({
                                        status: undefined,
                                        health: undefined,
                                        catalogFreshness: undefined,
                                        page: 1,
                                    }),
                            },
                            {
                                id: "supplier-api-connections-metric-enabled",
                                label: "已启用",
                                count: data?.metrics.enabled ?? 0,
                                active: urlState.status === "ENABLED",
                                onClick: () =>
                                    patchUrl({
                                        status:
                                            urlState.status === "ENABLED"
                                                ? undefined
                                                : "ENABLED",
                                        page: 1,
                                    }),
                            },
                            {
                                id: "supplier-api-connections-metric-faulted",
                                label: "故障",
                                count: data?.metrics.faulted ?? 0,
                                active: urlState.status === "FAULTED",
                                onClick: () =>
                                    patchUrl({
                                        status:
                                            urlState.status === "FAULTED"
                                                ? undefined
                                                : "FAULTED",
                                        page: 1,
                                    }),
                            },
                            {
                                id: "supplier-api-connections-metric-pending",
                                label: "待配置",
                                count: data?.metrics.pendingConfig ?? 0,
                                active: urlState.status === "PENDING_CONFIG",
                                onClick: () =>
                                    patchUrl({
                                        status:
                                            urlState.status === "PENDING_CONFIG"
                                                ? undefined
                                                : "PENDING_CONFIG",
                                        page: 1,
                                    }),
                            },
                            {
                                id: "supplier-api-connections-metric-health",
                                label: "健康异常",
                                count: data?.metrics.healthAbnormal ?? 0,
                                active: Boolean(urlState.health),
                                onClick: () =>
                                    patchUrl({
                                        health: urlState.health
                                            ? undefined
                                            : "FAILED,AUTH_FAILED,PARTIAL,UNKNOWN",
                                        page: 1,
                                    }),
                            },
                            {
                                id: "supplier-api-connections-metric-catalog",
                                label: "目录陈旧",
                                count: data?.metrics.catalogStale ?? 0,
                                active: Boolean(urlState.catalogFreshness),
                                onClick: () =>
                                    patchUrl({
                                        catalogFreshness:
                                            urlState.catalogFreshness
                                                ? undefined
                                                : "STALE,FAILED",
                                        page: 1,
                                    }),
                            },
                        ]}
                    />
                }
                toolbar={
                    <ConnectionListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.searchDraft}
                        onSearchDraftChange={filters.setSearchDraft}
                        environment={urlState.environment}
                        onEnvironmentChange={filters.applyEnvironment}
                        filterPanelOpen={filters.filterPanelOpen}
                        onFilterPanelOpenChange={filters.setFilterPanelOpen}
                        hasStructuredFilters={filters.hasStructuredFilters}
                        appliedChips={appliedChips}
                        removeFilter={filters.removeFilter}
                        onApplyFilters={filters.applyFilters}
                        onClearFilters={filters.clearFilters}
                        onResetMoreFilters={filters.resetMoreFilters}
                        statusDraft={filters.statusDraft}
                        onStatusDraftChange={filters.setStatusDraft}
                        healthDraft={filters.healthDraft}
                        onHealthDraftChange={filters.setHealthDraft}
                        capabilityDraft={filters.capabilityDraft}
                        onCapabilityDraftChange={filters.setCapabilityDraft}
                        catalogFreshnessDraft={filters.catalogFreshnessDraft}
                        onCatalogFreshnessDraftChange={
                            filters.setCatalogFreshnessDraft
                        }
                        supplierIdDraft={filters.supplierIdDraft}
                        onSupplierIdDraftChange={filters.setSupplierIdDraft}
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
                        onClearFilters={filters.clearFilters}
                        onCreate={() => setCreateOpen(true)}
                        loading={listQuery.isFetching}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        hasFilters={filters.hasFilters}
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
