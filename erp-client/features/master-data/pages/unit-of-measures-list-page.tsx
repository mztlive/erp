"use client"

import { DictionaryCompactWorkspace } from "@/features/master-data/components/list/dictionary-compact-workspace"
import { MasterDataEmptyCreateButton } from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { UnitOfMeasurePreviewSheet } from "@/features/master-data/components/unit-of-measure/unit-of-measure-preview-sheet"
import {
    UnitOfMeasureCreateDialog,
    UnitOfMeasureReviseDialog,
} from "@/features/master-data/components/unit-of-measure/unit-of-measure-form-dialogs"
import { UnitOfMeasureDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"

export function UnitOfMeasuresListPage() {
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "unit-of-measures",
        createPermission: "unit_of_measure:create",
        searchInputRef,
    })
    const exportPending = state.exportPending

    return (
        <ListPageFrame
            title="计量单位"
            description="查看计量单位，供商品选择基础单位。"
            exportMeta={state.exportMeta}
            actions={[
                masterDataExportAction({
                    id: "master-data-unit-of-measures-list-export",
                    pending: exportPending,
                    rowCount: state.rows.length,
                    onClick: state.onExport,
                }),
                masterDataCreateAction({
                    id: "master-data-unit-of-measures-list-create",
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: () => state.setCreateOpen(true),
                }),
            ]}
            resultsLabel={`计量单位 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <DictionaryCompactWorkspace
                listId="master-data-unit-of-measures-list"
                ariaLabel="计量单位列表"
                fullWidth
                metricsAriaLabel="计量单位指标筛选"
                allLabel="全部单位"
                hint="选择单位查看详情"
                countLabel="计量单位"
                searchPlaceholder={masterDataSearchPlaceholder(
                    "unit-of-measures",
                )}
                codeLabel={masterDataCopy.fUnitCode}
                emptyTitle="还没有计量单位资料"
                emptyDescription="点击「新建」创建第一份资料；历史记录会随资料保留。"
                emptyAction={
                    state.canCreate ? (
                        <MasterDataEmptyCreateButton
                            id="master-data-unit-of-measures-list-empty-create"
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
            <UnitOfMeasurePreviewSheet
                row={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
            />
            <UnitOfMeasureCreateDialog
                idPrefix="master-data-unit-of-measures-list-create-dialog"
                open={state.createOpen}
                onOpenChange={state.setCreateOpen}
            />
            <UnitOfMeasureReviseDialog
                idPrefix="master-data-unit-of-measures-list-revise-dialog"
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
            <UnitOfMeasureDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
