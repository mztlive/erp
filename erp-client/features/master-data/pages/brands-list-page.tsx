"use client"

import { PermissionGuard } from "@/features/auth/components/permission-guard"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { toast } from "@/components/ui/toast"
import { launchNavDelivery } from "@/lib/nav-delivery"
import type { BackgroundJobView } from "@/lib/background-jobs"
import { BrandImportDialog } from "@/features/master-data/components/brand/brand-import-dialog"
import { UploadIcon } from "lucide-react"

import { DictionaryCompactWorkspace } from "@/features/master-data/components/list/dictionary-compact-workspace"
import { MasterDataEmptyCreateButton } from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { BrandPreviewDialog } from "@/features/master-data/components/brand/brand-preview-dialog"
import { BrandListIdentity } from "@/features/master-data/components/brand/brand-card-content"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import {
    BrandCreateDialog,
    BrandReviseDialog,
} from "@/features/master-data/components/brand/brand-form-dialogs"
import { BrandDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { masterDataSearchPlaceholder } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"

const LIST_ID = "master-data-brands-list"
const IMPORT_ACTION_ID = `${LIST_ID}-import`

export function BrandsListPage() {
    const router = useRouter()
    const onImportSubmitted = (job: BackgroundJobView, fileName: string) => {
        launchNavDelivery(
            "background-task",
            document.getElementById(IMPORT_ACTION_ID),
        )
        toast.add({
            title: "品牌导入任务已提交",
            description: `「${fileName}」共 ${job.total_count} 行，可关闭页面，进度与结果在「后台任务」查看。`,
            type: "success",
            timeout: 6000,
            actionProps: {
                children: "查看任务",
                onClick: () => router.push("/governance/background-jobs"),
            },
        })
    }
    const [importOpen, setImportOpen] = useState(false)
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useDictionaryListState({
        resource: "brands",
        createPermission: "product_brand:create",
        searchInputRef,
    })

    return (
        <ListPageFrame
            resource="brands"
            eyebrow=""
            title="品牌"
            views={
                <LifecycleMetricStrip
                    idPrefix={`${LIST_ID}-metrics`}
                    metrics={state.syncedMetrics}
                    metricKey={state.filters.metricKey}
                    ariaLabel="品牌指标筛选"
                    allLabel="全部品牌"
                    variant="inline"
                    onChangeLifecycle={state.filters.changeLifecycle}
                />
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: IMPORT_ACTION_ID,
                    actionKey: "import",
                    label: "批量导入",
                    icon: UploadIcon,
                    tier: "quiet",
                    hidden: !state.canCreate,
                    onClick: () => setImportOpen(true),
                },
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
                    onClick: () => state.setCreateOpen(true),
                }),
            ]}
            resultsLabel={`品牌 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <DictionaryCompactWorkspace
                listId={LIST_ID}
                ariaLabel="品牌列表"
                countLabel="品牌"
                searchPlaceholder={masterDataSearchPlaceholder("brands")}
                codeLabel="品牌代码"
                renderIdentity={(row) => <BrandListIdentity row={row} />}
                emptyTitle="还没有品牌资料"
                emptyDescription={
                    state.canCreate
                        ? "点击「新建」创建品牌，或使用「批量导入」一次建立多个品牌。"
                        : undefined
                }
                emptyAction={
                    state.canCreate ? (
                        <MasterDataEmptyCreateButton
                            id={`${LIST_ID}-empty-create`}
                            onClick={() => state.setCreateOpen(true)}
                        />
                    ) : undefined
                }
                searchInputRef={searchInputRef}
                filters={state.filters}
                rows={state.rows}
                previewId={state.previewId}
                fetching={state.listQuery.isFetching}
                failed={state.listQuery.isError}
                hasData={Boolean(state.listQuery.data)}
                error={state.listQuery.error}
                onRetry={() => void state.listQuery.refetch()}
                onPreview={(row) => {
                    lastFocusedRowId.current = row.stableId
                    state.setPreviewId(row.stableId)
                }}
            />
            <BrandPreviewDialog
                row={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
            />
            {importOpen && (
                <PermissionGuard permission="product_brand:create">
                    <BrandImportDialog
                        onClose={() => setImportOpen(false)}
                        onSubmitted={onImportSubmitted}
                    />
                </PermissionGuard>
            )}
            <PermissionGuard permission="product_brand:create">
                <BrandCreateDialog
                    open={state.createOpen}
                    onOpenChange={state.setCreateOpen}
                />
            </PermissionGuard>
            <PermissionGuard permission="product_brand:update">
                <BrandReviseDialog
                    open={state.reviseTarget != null}
                    onOpenChange={(open) => {
                        if (!open) state.setReviseTarget(null)
                    }}
                    target={state.reviseTarget}
                />
            </PermissionGuard>
            <PermissionGuard permission="product_brand:update">
                <BrandDisableDialog
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
