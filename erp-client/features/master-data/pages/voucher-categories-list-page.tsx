"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { useState } from "react"
import type { MasterDataListItem } from "@/features/master-data/types"
import { VoucherCategoryStatusDialog } from "@/features/master-data/components/list/voucher-category-status-dialog"

import { DictionaryCompactWorkspace } from "@/features/master-data/components/list/dictionary-compact-workspace"
import { MasterDataEmptyCreateButton } from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { VoucherCategoryFormDialog } from "@/features/master-data/components/list/voucher-category-form-dialog"
import { VoucherCategoryPreviewSheet } from "@/features/master-data/components/list/voucher-category-preview-sheet"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"

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
    const exportPending = state.exportPending

    return (
        <ListPageFrame
            resource="voucher-categories"
            title="卡券类目"
            description="查看卡券类目资料。"
            exportMeta={state.exportMeta}
            actions={[
                masterDataExportAction({
                    id: "master-data-voucher-categories-list-export",
                    pending: exportPending,
                    rowCount: state.rows.length,
                    onClick: state.onExport,
                }),
                masterDataCreateAction({
                    id: "master-data-voucher-categories-list-create",
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: () => state.setCreateOpen(true),
                }),
            ]}
            resultsLabel={`卡券类目 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <DictionaryCompactWorkspace
                listId="master-data-voucher-categories-list"
                ariaLabel="卡券类目列表"
                metricsAriaLabel="卡券类目指标筛选"
                allLabel="全部类目"
                hint="选择类目查看详情"
                countLabel="卡券类目"
                showRevisionFilter={false}
                searchPlaceholder={masterDataSearchPlaceholder(
                    "voucher-categories",
                )}
                codeLabel="类目编号"
                emptyTitle="还没有卡券类目资料"
                emptyDescription={
                    state.canCreate
                        ? "点击「新建」创建第一份资料；历史记录会随资料保留。"
                        : undefined
                }
                emptyAction={
                    state.canCreate ? (
                        <MasterDataEmptyCreateButton
                            id="master-data-voucher-categories-list-empty-create"
                            onClick={() => state.setCreateOpen(true)}
                        />
                    ) : undefined
                }
                searchInputRef={searchInputRef}
                filters={state.filters}
                syncedMetrics={state.syncedMetrics}
                rows={state.rows}
                previewId={state.previewId}
                fetching={state.listQuery.isFetching}
                failed={state.listQuery.isError}
                hasData={state.listQuery.data != null}
                error={state.listQuery.error}
                onRetry={() => void state.listQuery.refetch()}
                onPreview={(row) => {
                    lastFocusedRowId.current = row.stableId
                    state.setPreviewId(row.stableId)
                }}
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
            <PermissionGuard permission="voucher_category_profile:create">
                <VoucherCategoryFormDialog
                    idPrefix="master-data-voucher-categories-list-create-dialog"
                    open={state.createOpen}
                    onOpenChange={state.setCreateOpen}
                />
            </PermissionGuard>
            <PermissionGuard permission="voucher_category_profile:update">
                <VoucherCategoryFormDialog
                    idPrefix="master-data-voucher-categories-list-revise-dialog"
                    open={state.reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setReviseTarget(null)
                    }}
                    target={state.reviseTarget}
                />
            </PermissionGuard>
        </ListPageFrame>
    )
}
