"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { PlusIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { CustomerCreateDialog } from "@/features/customers/components/customer-create-dialog"
import { SORT_COLUMN_TO_FIELD } from "@/features/customers/lib/directory-url"
import { describeCustomerDirectoryTable } from "@/features/customers/lib/customer-center-description"
import { useCustomerDirectoryColumns } from "@/features/customers/hooks/use-directory-columns"
import { useCustomerDirectoryQuery } from "@/features/customers/hooks/queries"
import {
    useCustomerCenterDirectoryState,
    useCustomerCenterScopeGuard,
    useCustomerCenterSearchShortcut,
} from "@/features/customers/hooks/use-customer-center-directory-state"
import { CustomerCenterDirectoryTable } from "@/features/customers/pages/customer-center-directory-table"
import { CustomerCenterDirectoryToolbar } from "@/features/customers/pages/customer-center-directory-toolbar"

export function CustomerCenterPage() {
    const router = useRouter()

    const directoryState = useCustomerCenterDirectoryState()
    const { scope, status, q, sort, dir, page } = directoryState
    const { canCreate, canReadAll } = useCustomerCenterScopeGuard({
        scope,
        status,
        q,
        sort,
        dir,
        page,
    })
    useCustomerCenterSearchShortcut()

    const [createOpen, setCreateOpen] = React.useState(false)

    const directoryQuery = useCustomerDirectoryQuery({
        scope,
        status,
        query: q,
        sort: (SORT_COLUMN_TO_FIELD[sort] as "updated_at") ?? "updated_at",
        sortDir: dir,
        page,
        pageSize: 20,
    })

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
            onRetry={() => {
                void directoryQuery.refetch()
            }}
        />
    ) : directoryQuery.isPending && !data ? (
        <div
            className="h-40 animate-pulse rounded-lg bg-muted"
            aria-busy="true"
            aria-label="正在加载客户目录"
        />
    ) : data && !data.hasCustomerScope ? (
        <BusinessEmptyState
            kind="no-scope"
            title="当前角色无客户范围"
            description="当前权限与数据范围内没有客户；不代表系统尚无客户。"
        />
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
        <PageScaffold>
            <PageHeader
                title="客户中心"
                metadata={
                    <DataFreshness
                        updatedAt={data?.queriedAt?.slice(11, 16) ?? "—"}
                        dateTime={data?.queriedAt}
                        state={directoryQuery.isError ? "failed" : "fresh"}
                        label="客户目录"
                    />
                }
                actions={
                    <PageActions
                        actions={
                            canCreate
                                ? [
                                      {
                                          actionKey: "create",
                                          label: "新建客户",
                                          icon: PlusIcon,
                                          onClick: () => setCreateOpen(true),
                                      },
                                  ]
                                : []
                        }
                    />
                }
            />

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        客户结果
                        {data ? (
                            <span
                                className="font-normal text-muted-foreground"
                                aria-live="polite"
                            >
                                {data.totalInScope.toLocaleString("zh-CN")} 条
                            </span>
                        ) : null}
                    </span>
                }
                description={
                    data
                        ? describeCustomerDirectoryTable({
                              scope,
                              status,
                              q,
                              totalInScope: data.totalInScope,
                              itemsLength: items.length,
                          })
                        : undefined
                }
                toolbar={
                    <CustomerCenterDirectoryToolbar
                        searchInputRef={directoryState.searchInputRef}
                        searchDraft={directoryState.searchDraft}
                        setSearchDraft={directoryState.setSearchDraft}
                        scope={scope}
                        onScopeChange={directoryState.applyScope}
                        statusDraft={directoryState.statusDraft}
                        setStatusDraft={directoryState.setStatusDraft}
                        canReadAll={canReadAll}
                        hasActiveFilters={directoryState.hasActiveFilters}
                        appliedChips={directoryState.appliedChips}
                        removeFilter={directoryState.removeFilter}
                        panelOpen={directoryState.panelOpen}
                        setPanelOpen={directoryState.setPanelOpen}
                        hasStructuredFilters={
                            directoryState.hasStructuredFilters
                        }
                        applyFilters={directoryState.applyFilters}
                        resetMoreFilters={directoryState.resetMoreFilters}
                        clearAllFilters={directoryState.clearAllFilters}
                    />
                }
                table={tableSlot}
            />

            <CustomerCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                onSucceeded={() => {
                    // 结果卡由用户选择打开客户或上传合同；不自动跳走。
                }}
            />
        </PageScaffold>
    )
}
