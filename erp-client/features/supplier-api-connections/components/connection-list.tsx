"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { PlusIcon, RefreshCwIcon } from "lucide-react"

import { FormalActionResult, ListWorkspacePage } from "@/components/business"
import type { ResultState } from "@/components/business/feedback"
import { useAccountProfileQuery } from "@/features/auth/queries"
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
import { hasPermission } from "@/lib/permissions"

export function ConnectionList({
    urlState,
    patchUrl,
    onOpen,
    onOpenCatalog,
}: {
    urlState: ConnectionsUrlState
    patchUrl: (patch: Partial<ConnectionsUrlState>) => void
    onOpen: (connectionId: string) => void
    onOpenCatalog: (connectionId: string) => void
}) {
    const profile = useAccountProfileQuery()
    const canCreate = hasPermission(
        profile.data?.permissions,
        "supplier_api_connection:create",
    )
    const canOpen = hasPermission(
        profile.data?.permissions,
        "supplier_api_connection:detail",
    )
    const filters = useConnectionListFilters(urlState, patchUrl)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [result, setResult] = React.useState<
        (ResultState & { actions?: React.ReactNode }) | null
    >(null)

    // 查询只消费 Applied（URL 派生且已降级非法枚举值），Draft 变化不请求（§5、§6.3）
    const listQuery = useConnectionListQuery({
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
    const canCreateConnection = canCreate && Boolean(data?.hasModulePermission)

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

    const columns = useConnectionListColumns(
        canOpen ? onOpen : undefined,
        canOpen ? onOpenCatalog : undefined,
    )

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
        <ListWorkspacePage
            title="平台连接"
            views={{
                ariaLabel: "连接状态与健康视图",
                items: [
                    {
                        id: "supplier-api-connections-view-all",
                        label: "全部连接",
                        active: !filters.applied.status,
                        onClick: () => patchUrl({ status: undefined, page: 1 }),
                    },
                    ...(
                        [
                            ["ENABLED", "enabled", "已启用"],
                            ["DISABLED", "disabled", "已停用"],
                            ["FAULTED", "faulted", "故障"],
                            ["PENDING_CONFIG", "pending-config", "待配置"],
                        ] as const
                    ).map(([status, id, label]) => ({
                        id: `supplier-api-connections-view-${id}`,
                        label,
                        active: filters.applied.status === status,
                        onClick: () => patchUrl({ status, page: 1 }),
                    })),
                ],
            }}
            actions={[
                {
                    id: "supplier-api-connections-list-refresh",
                    type: "button",
                    actionKey: "refresh",
                    label: "刷新",
                    icon: RefreshCwIcon,
                    tier: "quiet",
                    onClick: () => void listQuery.refetch(),
                },
                {
                    id: "supplier-api-connections-list-create",
                    type: "button",
                    actionKey: "create",
                    label: "新建连接",
                    icon: PlusIcon,
                    tier: "primary",
                    hidden: !profile.isSuccess || !canCreateConnection,
                    onClick: () => setCreateOpen(true),
                },
            ]}
            alerts={
                result ? (
                    <FormalActionResult
                        status={
                            result.status === "failed"
                                ? "rejected"
                                : result.status
                        }
                        title={result.title}
                        description={result.description}
                        reference={result.reference}
                        facts={result.facts}
                        actions={result.actions}
                    />
                ) : null
            }
            surface={{
                ariaLabel: "供应商平台连接列表",
                tableClassName:
                    "max-sm:[&_[data-column-pinned=left]]:left-auto! max-sm:[&_[data-pinned-edge=left]]:shadow-none!",
                toolbar: (
                    <ConnectionListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.searchDraft}
                        onSearchDraftChange={filters.setSearchDraft}
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
                        loading={listQuery.isFetching}
                        failed={listQuery.isError}
                    />
                ),
                table: (
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
                        onRowOpen={canOpen ? onOpen : undefined}
                        onClearFilters={filters.clearFilters}
                        onCreate={
                            canCreateConnection
                                ? () => setCreateOpen(true)
                                : undefined
                        }
                        loading={listQuery.isPending || listQuery.isFetching}
                        isError={listQuery.isError}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        hasFilters={filters.hasFilters}
                    />
                ),
            }}
        >
            {canCreate && (
                <ConnectionCreateDialog
                    open={createOpen}
                    onOpenChange={setCreateOpen}
                    onOpen={canOpen ? onOpen : undefined}
                    onResult={setResult}
                />
            )}
        </ListWorkspacePage>
    )
}
