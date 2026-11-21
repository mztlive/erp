"use client"

import { useState } from "react"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { DictionaryCompactWorkspace } from "@/features/master-data/components/list/dictionary-compact-workspace"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { MasterDataEmptyCreateButton } from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { VoucherCategoryFormDialog } from "@/features/master-data/components/list/voucher-category-form-dialog"
import { VoucherCategoryPreviewSheet } from "@/features/master-data/components/list/voucher-category-preview-sheet"
import { VoucherCategoryStatusDialog } from "@/features/master-data/components/list/voucher-category-status-dialog"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"
import type { MasterDataListItem } from "@/features/master-data/types"

const LIST_ID = "master-data-voucher-categories-list"

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
    const { filters } = state
    const openCreate = () => state.setCreateOpen(true)

    return (
        <ListPageFrame
            resource="voucher-categories"
            eyebrow=""
            title="卡券类目"
            views={
                <LifecycleMetricStrip
                    idPrefix={`${LIST_ID}-metrics`}
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="卡券类目指标筛选"
                    allLabel="全部类目"
                    variant="inline"
                    onChangeLifecycle={filters.changeLifecycle}
                />
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    ...masterDataExportAction({
                        id: `${LIST_ID}-export`,
                        pending: state.exportPending,
                        rowCount: state.rows.length,
                        onClick: state.onExport,
                    }),
                    tier: "quiet",
                },
                masterDataCreateAction({
                    id: `${LIST_ID}-create`,
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: openCreate,
                }),
            ]}
            resultsLabel={`卡券类目 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <DictionaryCompactWorkspace
                listId={LIST_ID}
                ariaLabel="卡券类目列表"
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
                            id={`${LIST_ID}-empty-create`}
                            onClick={openCreate}
                        />
                    ) : undefined
                }
                searchInputRef={searchInputRef}
                filters={filters}
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
