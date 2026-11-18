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
import { BrandCardContent } from "@/features/master-data/components/brand/brand-card-content"
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

export function BrandsListPage() {
    const router = useRouter()
    const onImportSubmitted = (job: BackgroundJobView, fileName: string) => {
        launchNavDelivery(
            "background-task",
            document.getElementById("master-data-brands-list-import"),
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
    const exportPending = state.exportPending

    return (
        <ListPageFrame
            resource="brands"
            title="品牌"
            description="查看品牌字典，供商品与 SKU 选用。"
            exportMeta={state.exportMeta}
            actions={[
                masterDataExportAction({
                    id: "master-data-brands-list-export",
                    pending: exportPending,
                    rowCount: state.rows.length,
                    onClick: state.onExport,
                }),
                {
                    id: "master-data-brands-list-import",
                    actionKey: "import",
                    label: "批量导入",
                    icon: UploadIcon,
                    variant: "outline",
                    hidden: !state.canCreate,
                    onClick: () => setImportOpen(true),
                },
                masterDataCreateAction({
                    id: "master-data-brands-list-create",
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
                listId="master-data-brands-list"
                ariaLabel="品牌列表"
                metricsAriaLabel="品牌指标筛选"
                allLabel="全部品牌"
                hint="选择品牌查看详情"
                countLabel="品牌"
                searchPlaceholder={masterDataSearchPlaceholder("brands")}
                codeLabel="品牌代码"
                renderCard={(row) => <BrandCardContent row={row} />}
                emptyTitle="还没有品牌资料"
                emptyDescription={
                    state.canCreate
                        ? "点击「新建」创建品牌，或使用「批量导入」一次建立多个品牌。"
                        : undefined
                }
                emptyAction={
                    state.canCreate ? (
                        <MasterDataEmptyCreateButton
                            id="master-data-brands-list-empty-create"
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
