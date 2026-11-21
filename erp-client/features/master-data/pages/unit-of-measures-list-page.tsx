"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { DictionaryCompactWorkspace } from "@/features/master-data/components/list/dictionary-compact-workspace"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
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

const LIST_ID = "master-data-unit-of-measures-list"

export function UnitOfMeasuresListPage() {
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "unit-of-measures",
        createPermission: "unit_of_measure:create",
        searchInputRef,
    })
    const { filters } = state
    const openCreate = () => state.setCreateOpen(true)

    return (
        <ListPageFrame
            resource="unit-of-measures"
            eyebrow=""
            title="计量单位"
            views={
                <LifecycleMetricStrip
                    idPrefix={`${LIST_ID}-metrics`}
                    metrics={state.syncedMetrics}
                    metricKey={filters.metricKey}
                    ariaLabel="计量单位指标筛选"
                    allLabel="全部单位"
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
            resultsLabel={`计量单位 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <DictionaryCompactWorkspace
                listId={LIST_ID}
                ariaLabel="计量单位列表"
                countLabel="计量单位"
                searchPlaceholder={masterDataSearchPlaceholder(
                    "unit-of-measures",
                )}
                codeLabel={masterDataCopy.fUnitCode}
                emptyTitle="还没有计量单位资料"
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
            <UnitOfMeasurePreviewSheet
                row={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
            />
            <PermissionGuard permission="unit_of_measure:create">
                <UnitOfMeasureCreateDialog
                    idPrefix="master-data-unit-of-measures-list-create-dialog"
                    open={state.createOpen}
                    onOpenChange={state.setCreateOpen}
                />
            </PermissionGuard>
            <PermissionGuard permission="unit_of_measure:update">
                <UnitOfMeasureReviseDialog
                    idPrefix="master-data-unit-of-measures-list-revise-dialog"
                    open={state.reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setReviseTarget(null)
                    }}
                    target={state.reviseTarget}
                />
            </PermissionGuard>
            <PermissionGuard permission="unit_of_measure:update">
                <UnitOfMeasureDisableDialog
                    open={state.disableTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setDisableTarget(null)
                    }}
                    target={state.disableTarget}
                />
            </PermissionGuard>
        </ListPageFrame>
    )
}
