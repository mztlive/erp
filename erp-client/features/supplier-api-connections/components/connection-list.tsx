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
                                active: !filters.applied.status,
                                onClick: () =>
                                    patchUrl({ status: undefined, page: 1 }),
                            },
                            ...(
                                [
                                    ["ENABLED", "enabled", "已启用"],
                                    ["DISABLED", "disabled", "已停用"],
                                    ["FAULTED", "faulted", "故障"],
                                    [
                                        "PENDING_CONFIG",
                                        "pending-config",
                                        "待配置",
                                    ],
                                ] as const
                            ).map(([status, id, label]) => ({
                                id: `supplier-api-connections-view-${id}`,
                                label,
                                active: filters.applied.status === status,
                                onClick: () => patchUrl({ status, page: 1 }),
                            })),
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
                        appliedChips={appliedChips}
                        removeFilter={filters.removeFilter}
                        onApplyFilters={filters.applyFilters}
                        onClearFilters={filters.clearFilters}
                        onResetMoreFilters={filters.resetMoreFilters}
                        onCancelMoreFilters={filters.cancelMoreFilters}
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
                        hasPendingChanges={filters.hasPendingChanges}
                        resultCount={data?.total}
                        loading={listQuery.isFetching}
                        failed={listQuery.isError}
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
