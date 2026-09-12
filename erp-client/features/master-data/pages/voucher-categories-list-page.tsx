"use client"

import { useState } from "react"
import type { MasterDataListItem } from "@/features/master-data/types"
import { VoucherCategoryStatusDialog } from "@/features/master-data/components/list/voucher-category-status-dialog"

import { DownloadIcon, PlusIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

import { Button } from "@/components/ui/button"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { DictionaryCompactList } from "@/features/master-data/components/list/dictionary-compact-list"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { VoucherCategoryFormDialog } from "@/features/master-data/components/list/voucher-category-form-dialog"
import { VoucherCategoryPreviewSheet } from "@/features/master-data/components/list/voucher-category-preview-sheet"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"

export function VoucherCategoriesListPage() {
    const [statusTarget, setStatusTarget] = useState<MasterDataListItem | null>(
        null,
    )
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "voucher-categories",
        createPermission: "voucher_category_profile:create",
        enablePreview: true,
        enableRevisionFilter: false,
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
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.lifecycleStatus !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title="卡券类目"
            description="查看卡券类目资料。"
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
            <ListWorkSurface
                ariaLabel="卡券类目列表"
                views={
                    <LifecycleMetricStrip
                        idPrefix="master-data-voucher-categories-list-metrics"
                        metrics={state.syncedMetrics}
                        metricKey={filters.metricKey}
                        ariaLabel="卡券类目指标筛选"
                        allLabel="全部类目"
                        hint="选择类目查看详情"
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                }
                toolbar={
                    <DictionaryListToolbar
                        idPrefix="master-data-voucher-categories-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        showRevisionFilter={false}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "voucher-categories",
                        )}
                        countLabel="卡券类目"
                        resultCount={
                            state.listQuery.data ? state.rows.length : undefined
                        }
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                    />
                }
                table={
                    <DictionaryCompactList
                        id="master-data-voucher-categories-list"
                        rows={state.rows}
                        codeLabel="类目编号"
                        selectedId={state.previewId}
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
                        onPreview={(row) => {
                            lastFocusedRowId.current = row.stableId
                            state.setPreviewId(row.stableId)
                        }}
                    />
                }
            />
            <VoucherCategoryPreviewSheet
                row={state.previewRow}
                detailQuery={state.previewDetailQuery}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onStatusChange={setStatusTarget}
            />
            <VoucherCategoryStatusDialog
                target={statusTarget}
                onClose={() => setStatusTarget(null)}
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
