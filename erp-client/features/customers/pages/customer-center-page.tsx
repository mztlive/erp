"use client"

import * as React from "react"
import { DownloadIcon, PlusIcon, UploadIcon } from "lucide-react"

import {
    BusinessFailureState,
    ListWorkspacePage,
    listWorkspaceFilterStatusText,
    type ListWorkspacePageAction,
} from "@/components/business"
import { toast } from "@/components/ui/toast"
import { CustomerImportDialog } from "@/features/customers/components/customer-import-dialog"
import { CustomerCreateDialog } from "@/features/customers/components/customer-create-dialog"
import { useCustomerDirectoryQuery } from "@/features/customers/hooks/queries"
import { useCustomerCenterDirectoryState } from "@/features/customers/hooks/use-customer-center-directory-state"
import { useCustomerDirectoryExport } from "@/features/customers/hooks/use-customer-directory-export"
import {
    SCOPE_LABELS,
    visibleCustomerScopes,
} from "@/features/customers/lib/filter-customers"
import { CustomerCenterDirectoryTable } from "@/features/customers/pages/customer-center-directory-table"
import { CustomerCenterDirectoryToolbar } from "@/features/customers/pages/customer-center-directory-toolbar"
import type {
    CustomerDirectoryQuery,
    CustomerScope,
} from "@/features/customers/types"
import { isDataScopeChanged } from "@/features/data-scope/cache"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { useRouter } from "@/lib/navigation"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

export function CustomerCenterPage() {
    const router = useRouter()
    const directoryState = useCustomerCenterDirectoryState()
    const {
        accountProfile,
        scope,
        status,
        q,
        dir,
        page,
        pushState,
        canCreate,
        canReadDirectory,
        canOpenDetail,
        canReadAll,
    } = directoryState
    useSearchHotkey(directoryState.searchInputRef)

    const [createOpen, setCreateOpen] = React.useState(false)
    const [importOpen, setImportOpen] = React.useState(false)
    const [lastResolvedScope, setLastResolvedScope] =
        React.useState<CustomerScope | null>(null)

    const directoryInput: CustomerDirectoryQuery = {
        scope,
        status,
        query: q,
        ownerUserIds: directoryState.ownerUserIds,
        orgUnitIds: directoryState.orgUnitIds,
        includeDescendants: directoryState.includeDescendants,
        sort: "updated_at",
        sortDir: dir,
        page,
        pageSize: 20,
    }
    const directoryQuery = useCustomerDirectoryQuery(directoryInput, {
        enabled: canReadDirectory && (scope !== "all_authorized" || canReadAll),
    })
    const exportMutation = useCustomerDirectoryExport(directoryInput)

    const resolved =
        canReadDirectory &&
        directoryQuery.isSuccess &&
        !directoryQuery.isPlaceholderData
    React.useEffect(() => {
        if (resolved) setLastResolvedScope(scope)
    }, [scope, resolved])

    React.useEffect(() => {
        if (!isDataScopeChanged(directoryQuery.error) || page <= 1) return
        pushState({ page: 1 })
    }, [directoryQuery.error, page, pushState])

    // 不在另一个客户范围下展示占位数据。
    const data =
        canReadDirectory &&
        (!directoryQuery.isPlaceholderData || lastResolvedScope === scope)
            ? directoryQuery.data
            : undefined
    const listLoadFailed =
        directoryQuery.isError || (!directoryQuery.isFetching && !data)
    const listLoading = directoryQuery.isFetching && !listLoadFailed
    const initialLoading =
        listLoading &&
        directoryQuery.isPending &&
        !directoryQuery.isFetched &&
        lastResolvedScope === null
    const resultsStatus =
        data && !listLoadFailed && !directoryQuery.isPlaceholderData
            ? listWorkspaceFilterStatusText({
                  resultCount: data.totalInScope,
                  noun: "个客户",
              })
            : null
    // 授权时点以服务端响应为准。
    const authorizationTime = data?.asOf ?? data?.queriedAt
    const exportDisabledReason = listLoadFailed
        ? "请重新查询后再导出"
        : !data || directoryQuery.isPlaceholderData
          ? "请等待查询完成"
          : data.totalInScope === 0
            ? "没有可导出的客户"
            : undefined
    const openCreate = () => setCreateOpen(true)
    const createAction: ListWorkspacePageAction = {
        id: "customers-directory-create",
        actionKey: "create",
        label: "新建客户",
        icon: PlusIcon,
        tier: "primary",
        hidden: !canCreate,
        onClick: openCreate,
    }
    const importAction: ListWorkspacePageAction = {
        id: "customers-directory-import",
        actionKey: "import",
        label: "批量导入",
        icon: UploadIcon,
        tier: "quiet",
        hidden: !canCreate,
        onClick: () => setImportOpen(true),
    }
    const dialogs = canCreate ? (
        <>
            <CustomerCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
            />
            {importOpen && (
                <CustomerImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={(job, fileName) => {
                        toast.add({
                            title: "客户导入任务已提交",
                            description: `「${fileName}」共 ${job.total_count} 行`,
                            type: "success",
                            actionProps: hasPermission(
                                accountProfile.data?.permissions,
                                "background_job:list",
                            )
                                ? {
                                      children: "查看任务",
                                      onClick: () =>
                                          router.push(
                                              "/governance/background-jobs",
                                          ),
                                  }
                                : undefined,
                        })
                    }}
                />
            )}
        </>
    ) : null

    if (!accountProfile.data && accountProfile.isError) {
        return (
            <ListWorkspacePage
                title="客户中心"
                alerts={
                    <BusinessFailureState
                        idPrefix="customers-directory"
                        title="无法读取客户查看权限"
                        error={accountProfile.error}
                        onRetry={() => void accountProfile.refetch()}
                    />
                }
            />
        )
    }
    if (!accountProfile.data && accountProfile.isPending) {
        return <ListWorkspacePage title="客户中心" loading />
    }
    if (!canReadDirectory) {
        return (
            <ListWorkspacePage
                title="客户中心"
                actions={canCreate ? [importAction, createAction] : undefined}
                alerts={
                    <BusinessFailureState
                        kind="permission"
                        title="暂无客户目录查看权限"
                    />
                }
            >
                {dialogs}
            </ListWorkspacePage>
        )
    }

    return (
        <ListWorkspacePage
            title="客户中心"
            views={{
                ariaLabel: "客户范围",
                items: visibleCustomerScopes(canReadAll).map((key) => ({
                    id: `customers-directory-scope-${toAutomationIdSegment(key)}`,
                    label: SCOPE_LABELS[key],
                    count:
                        !listLoadFailed && scope === key
                            ? data?.totalInScope
                            : undefined,
                    active: scope === key,
                    onClick: () => directoryState.applyScope(key),
                })),
            }}
            headerActions={
                !listLoadFailed && authorizationTime ? (
                    <time
                        dateTime={authorizationTime}
                        className="num sr-only whitespace-nowrap text-xs text-muted-foreground xl:not-sr-only"
                    >
                        更新于 {formatAuthorizationTime(authorizationTime)}
                    </time>
                ) : undefined
            }
            actions={[
                {
                    id: "customers-directory-export",
                    actionKey: "export",
                    label: exportMutation.isPending ? "正在导出…" : "导出",
                    icon: DownloadIcon,
                    tier: "quiet",
                    disabled: exportDisabledReason !== undefined,
                    title: exportDisabledReason,
                    loading: exportMutation.isPending,
                    onClick: () => exportMutation.mutate(),
                },
                importAction,
                createAction,
            ]}
            notices={
                <span className="sr-only" role="status">
                    {resultsStatus}
                </span>
            }
            loading={initialLoading}
            surface={{
                ariaLabel: "客户目录",
                toolbar: (
                    <CustomerCenterDirectoryToolbar
                        canFilterOwner={directoryState.canFilterOwner}
                        canFilterOrg={directoryState.canFilterOrg}
                        ownerDraft={directoryState.ownerDraft}
                        setOwnerDraft={directoryState.setOwnerDraft}
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
                        removeOrgFilter={directoryState.removeOrgFilter}
                        applyFilters={directoryState.applyFilters}
                        panelOpen={directoryState.panelOpen}
                        setPanelOpen={directoryState.setPanelOpen}
                        resetMoreFilters={directoryState.resetMoreFilters}
                        cancelMoreFilters={directoryState.cancelMoreFilters}
                        clearAllFilters={directoryState.clearAllFilters}
                        hasActiveFilters={directoryState.hasActiveFilters}
                        hasPendingChanges={directoryState.hasPendingChanges}
                        resultStatus={
                            listLoadFailed
                                ? "查询未完成"
                                : listLoading && !data?.items.length
                                  ? "查询中…"
                                  : null
                        }
                    />
                ),
                table: (
                    <CustomerCenterDirectoryTable
                        data={data}
                        scope={scope}
                        canCreate={canCreate}
                        canOpenDetail={canOpenDetail}
                        hasActiveFilters={directoryState.hasActiveFilters}
                        sorting={directoryState.sorting}
                        onSortingChange={directoryState.handleSortingChange}
                        page={
                            directoryQuery.isPlaceholderData
                                ? (data?.page ?? page)
                                : page
                        }
                        onPaginationChange={
                            directoryState.handlePaginationChange
                        }
                        loading={listLoading}
                        failed={listLoadFailed}
                        error={directoryQuery.error}
                        onRetry={() => void directoryQuery.refetch()}
                        onClearFilters={directoryState.clearAllFilters}
                        onCreate={openCreate}
                        onOpenRow={(row) =>
                            router.push(`/sales/customers/${row.id}`)
                        }
                    />
                ),
            }}
        >
            {dialogs}
        </ListWorkspacePage>
    )
}

function formatAuthorizationTime(asOf: string): string {
    const date = new Date(asOf)
    if (Number.isNaN(date.getTime())) return asOf
    return date.toLocaleTimeString("zh-CN", {
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    })
}
