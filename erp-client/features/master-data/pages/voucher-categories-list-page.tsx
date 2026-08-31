"use client"

import { DownloadIcon, PlusIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

import { BusinessTableFrame } from "@/components/business"
import { Button } from "@/components/ui/button"
import { DictionaryListTable } from "@/features/master-data/components/list/dictionary-list-table"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { VoucherCategoryFormDialog } from "@/features/master-data/components/list/voucher-category-form-dialog"
import { useVoucherCategoryListColumns } from "@/features/master-data/hooks/use-dictionary-list-columns"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"

export function VoucherCategoriesListPage() {
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "voucher-categories",
        createPermission: "voucher_category_profile:create",
        searchInputRef,
    })
    const exportPending =
        useIsMutating({
            predicate: (mutation) => {
                const variables = mutation.state.variables
                return (
                    typeof variables === "object" &&
                    variables !== null &&
                    "resource" in variables &&
                    variables.resource === "voucher-categories" &&
                    !("idempotencyKey" in variables)
                )
            },
        }) > 0
    const columns = useVoucherCategoryListColumns({
        lastFocusedRowId,
        rows: state.rows,
        onReviseTarget: state.setReviseTarget,
    })
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title={masterDataCopy.pageTitle("卡券类目")}
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: "master-data-voucher-categories-list-export",
                    actionKey: "export",
                    label: exportPending
                        ? "导出中…"
                        : masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    mobileVisibility: "hide",
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-voucher-categories-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    mobileVisibility: "hide",
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => state.setCreateOpen(true),
                },
            ]}
            resultsLabel={`卡券类目 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        卡券类目列表
                        <span
                            className="font-normal text-muted-foreground"
                            aria-live="polite"
                        >
                            {state.rows.length} 条
                        </span>
                    </span>
                }
                description={state.listTableDescription}
                toolbar={
                    <DictionaryListToolbar
                        idPrefix="master-data-voucher-categories-list-toolbar"
                        searchInputRef={searchInputRef}
                        searchDraft={filters.searchDraft}
                        setSearchDraft={filters.setSearchDraft}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "voucher-categories",
                        )}
                        countLabel="卡券类目"
                        hasActiveFilters={hasActiveFilters}
                        clearAllFilters={filters.clearAllFilters}
                        appliedChips={filters.appliedChips}
                        removeFilter={filters.removeFilter}
                        filterPanelOpen={filters.filterPanelOpen}
                        setFilterPanelOpen={filters.setFilterPanelOpen}
                        hasStructuredListFilters={
                            filters.hasStructuredListFilters
                        }
                        applyListFilters={filters.applyListFilters}
                        resetMoreFilters={filters.resetMoreFilters}
                        lifecycleStatusDraft={filters.lifecycleStatusDraft}
                        setLifecycleStatusDraft={
                            filters.setLifecycleStatusDraft
                        }
                        revisionTimingDraft={filters.revisionTimingDraft}
                        setRevisionTimingDraft={filters.setRevisionTimingDraft}
                    />
                }
                table={
                    <DictionaryListTable
                        id="master-data-voucher-categories-list-table"
                        rows={state.rows}
                        pageRows={state.pageRows}
                        columns={columns}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        loading={state.listQuery.isFetching}
                        listLoadFailed={listLoadFailed}
                        error={state.listQuery.error}
                        onRetry={() => void state.listQuery.refetch()}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={filters.clearAllFilters}
                        emptyTitle="还没有卡券类目资料"
                        emptyDescription="点击「新建」创建第一份资料；历史记录会随资料保留。"
                        emptyAction={
                            state.canCreate ? (
                                <Button
                                    id="master-data-voucher-categories-list-empty-create"
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={() => state.setCreateOpen(true)}
                                >
                                    {masterDataCopy.actionCreate}
                                </Button>
                            ) : undefined
                        }
                        onRowPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setReviseTarget(row)
                        }}
                        onRowOpen={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setReviseTarget(row)
                        }}
                    />
                }
            />
            <VoucherCategoryFormDialog
                idPrefix="master-data-voucher-categories-list-create-dialog"
                open={state.createOpen}
                onOpenChange={state.setCreateOpen}
            />
            <VoucherCategoryFormDialog
                idPrefix="master-data-voucher-categories-list-revise-dialog"
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
        </ListPageFrame>
    )
}
