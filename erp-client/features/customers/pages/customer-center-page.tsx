"use client"

import * as React from "react"
import { useCustomerDirectoryExport } from "../hooks/use-customer-directory-export"
import type { CustomerDirectoryQuery } from "../types"
import { useRouter } from "next/navigation"
import { PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { CustomerCreateDialog } from "@/features/customers/components/customer-create-dialog"
import {
    useCustomerCenterDirectoryState,
    useCustomerCenterScopeGuard,
    useCustomerCenterSearchShortcut,
} from "@/features/customers/hooks/use-customer-center-directory-state"
import { useCustomerDirectoryColumns } from "@/features/customers/hooks/use-directory-columns"
import { useCustomerDirectoryQuery } from "@/features/customers/hooks/queries"
import { SORT_COLUMN_TO_FIELD } from "@/features/customers/lib/directory-url"
import {
    SCOPE_LABELS,
    visibleCustomerScopes,
} from "@/features/customers/lib/filter-customers"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { CustomerCenterDirectoryTable } from "@/features/customers/pages/customer-center-directory-table"
import { CustomerCenterDirectoryToolbar } from "@/features/customers/pages/customer-center-directory-toolbar"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function CustomerCenterPage() {
    const router = useRouter()

    const directoryState = useCustomerCenterDirectoryState()
    const { scope, status, q, sort, dir, page, pushState } = directoryState
    const { canCreate, canReadAll } = useCustomerCenterScopeGuard()
    useCustomerCenterSearchShortcut()

    const [createOpen, setCreateOpen] = React.useState(false)

    const directoryInput: CustomerDirectoryQuery = {
        scope,
        status,
        query: q,
        ownerUserIds: directoryState.ownerUserIds,
        orgUnitIds: directoryState.orgUnitIds,
        includeDescendants: directoryState.includeDescendants,
        sort: (SORT_COLUMN_TO_FIELD[sort] as "updated_at") ?? "updated_at",
        sortDir: dir,
        page,
        pageSize: 20,
    }
    const directoryQuery = useCustomerDirectoryQuery(directoryInput, {
        enabled: scope !== "all_authorized" || canReadAll,
    })
    const exportMutation = useCustomerDirectoryExport(directoryInput)

    React.useEffect(() => {
        if (!isDataScopeChanged(directoryQuery.error) || page <= 1) return
        pushState({ page: 1 })
    }, [directoryQuery.error, page, pushState])

    const data = directoryQuery.data
    const items = React.useMemo(() => data?.items ?? [], [data?.items])
    const columns = useCustomerDirectoryColumns()

    // 分页从 URL 派生（P6），筛选/搜索变更写 URL 并回第 1 页。
    const pagination = React.useMemo(
        () => ({ pageIndex: Math.max(0, page - 1), pageSize: 20 }),
        [page],
    )

    // 查询失败只替换结果区，筛选区必须常驻（docs/ui-filter-design.md §11.2）。
    const tableSlot = directoryQuery.isError ? (
        <BusinessFailureState
            title="客户目录加载失败"
            error={directoryQuery.error}
            action={
                <Button
                    id="customers-directory-retry"
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => {
                        void directoryQuery.refetch()
                    }}
                >
                    重试
                </Button>
            }
        />
    ) : directoryQuery.isPending && !data ? (
        <div
            className="h-40 animate-pulse rounded-lg bg-muted"
            aria-busy="true"
            aria-label="正在加载客户目录"
        />
    ) : data && data.emptyReason === "no_scope" ? (
        <BusinessEmptyState
            kind="no-scope"
            className={listWorkspaceEmptyStateClassName}
            title="当前角色无客户范围"
            description="当前权限与数据范围内没有客户；不代表系统尚无客户。"
        />
    ) : data && items.length === 0 ? (
        data.totalInScope === 0 && !directoryState.hasActiveFilters ? (
            <BusinessEmptyState
                kind="no-data"
                className={listWorkspaceEmptyStateClassName}
                title="当前范围尚无客户"
                description={`${SCOPE_LABELS[scope]}下还没有客户。有权时可新建客户。`}
                action={
                    canCreate ? (
                        <Button
                            id="customers-directory-empty-create"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => setCreateOpen(true)}
                        >
                            新建客户
                        </Button>
                    ) : null
                }
            />
        ) : (
            <BusinessEmptyState
                kind="filter"
                className={listWorkspaceEmptyStateClassName}
                title="当前筛选无结果"
                description={`范围“${SCOPE_LABELS[scope]}”${status !== "active" ? ` · 状态 ${status}` : ""}${q ? ` · 关键词“${q}”` : ""} 下没有匹配客户。`}
                action={
                    directoryState.hasActiveFilters ? (
                        <Button
                            id="customers-directory-empty-clear-filters"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={directoryState.clearAllFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null
                }
            />
        )
    ) : data ? (
        <CustomerCenterDirectoryTable
            items={items}
            totalInScope={data.totalInScope}
            columns={columns}
            scope={scope}
            status={status}
            q={q}
            canCreate={canCreate}
            hasActiveFilters={directoryState.hasActiveFilters}
            sorting={directoryState.sorting}
            onSortingChange={directoryState.handleSortingChange}
            pagination={pagination}
            onPaginationChange={directoryState.handlePaginationChange}
            onClearFilters={directoryState.clearAllFilters}
            onCreate={() => setCreateOpen(true)}
            onOpenRow={(row) => router.push(`/sales/customers/${row.id}`)}
        />
    ) : null

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="销售"
                title="客户中心"
                description={
                    <>
                        查看客户资料与合作范围。
                        <span className="ml-3 text-xs" role="status">
                            {directoryQuery.isError ? (
                                "查询失败"
                            ) : directoryQuery.isFetching ? (
                                "正在更新…"
                            ) : data?.asOf || data?.queriedAt ? (
                                <time dateTime={data.asOf ?? data.queriedAt}>
                                    更新于{" "}
                                    {formatAuthorizationTime(
                                        data.asOf ?? data.queriedAt,
                                    )}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            >
                <Button
                    id="customers-directory-export"
                    variant="outline"
                    disabled={exportMutation.isPending || !data?.totalInScope}
                    onClick={() => exportMutation.mutate()}
                >
                    {exportMutation.isPending ? "正在导出…" : "导出"}
                </Button>
                {canCreate ? (
                    <Button
                        id="customers-directory-create"
                        type="button"
                        onClick={() => setCreateOpen(true)}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden="true" />
                        新建客户
                    </Button>
                ) : null}
            </ListWorkspaceHeader>

            <ListWorkSurface
                ariaLabel="客户目录"
                views={
                    <ListWorkspaceViews
                        ariaLabel="客户范围"
                        hint="选择客户查看详情"
                        items={visibleCustomerScopes(canReadAll).map((key) => ({
                            id: `customers-directory-scope-${toAutomationIdSegment(key)}`,
                            label: SCOPE_LABELS[key],
                            count:
                                data && scope === key
                                    ? data.totalInScope
                                    : undefined,
                            active: scope === key,
                            onClick: () => directoryState.applyScope(key),
                        }))}
                    />
                }
                toolbar={
                    <CustomerCenterDirectoryToolbar
                        ownerDraft={directoryState.ownerDraft}
                        setOwnerDraft={directoryState.setOwnerDraft}
                        ownerOptions={data?.ownerOptions ?? []}
                        orgDraft={directoryState.orgDraft}
                        setOrgDraft={directoryState.setOrgDraft}
                        descendantsDraft={directoryState.descendantsDraft}
                        setDescendantsDraft={directoryState.setDescendantsDraft}
                        searchInputRef={directoryState.searchInputRef}
                        searchDraft={directoryState.searchDraft}
                        setSearchDraft={directoryState.setSearchDraft}
                        statusDraft={directoryState.statusDraft}
                        setStatusDraft={directoryState.setStatusDraft}
                        appliedChips={directoryState.appliedChips}
                        removeFilter={directoryState.removeFilter}
                        applyFilters={directoryState.applyFilters}
                        panelOpen={directoryState.panelOpen}
                        setPanelOpen={directoryState.setPanelOpen}
                        resetMoreFilters={directoryState.resetMoreFilters}
                        cancelMoreFilters={directoryState.cancelMoreFilters}
                        clearAllFilters={directoryState.clearAllFilters}
                        hasPendingChanges={directoryState.hasPendingChanges}
                        resultCount={data?.totalInScope}
                        loading={directoryQuery.isPending && !data}
                        failed={directoryQuery.isError}
                    />
                }
                table={tableSlot}
            />

            <CustomerCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
            />
        </PageScaffold>
    )
}

/** 展示服务端授权时点，不得改用本地时钟冒充查询时刻。 */
function formatAuthorizationTime(asOf: string): string {
    const date = new Date(asOf)
    if (Number.isNaN(date.getTime())) return asOf
    return date.toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    })
}
