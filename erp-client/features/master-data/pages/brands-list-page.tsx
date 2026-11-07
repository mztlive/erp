"use client"

import { useState } from "react"
import { useRouter } from "next/navigation"
import { toast } from "@/components/ui/toast"
import { launchNavDelivery } from "@/lib/nav-delivery"
import type { BackgroundJobView } from "@/lib/background-jobs"
import { BrandImportDialog } from "@/features/master-data/components/brand/brand-import-dialog"
import { DownloadIcon, PlusIcon, UploadIcon } from "lucide-react"

import { ListWorkSurface } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { DictionaryListToolbar } from "@/features/master-data/components/list/dictionary-list-toolbar"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { DictionaryCompactList } from "@/features/master-data/components/list/dictionary-compact-list"
import { BrandPreviewDialog } from "@/features/master-data/components/brand/brand-preview-dialog"
import { BrandCardContent } from "@/features/master-data/components/brand/brand-card-content"
import {
    BrandCreateDialog,
    BrandReviseDialog,
} from "@/features/master-data/components/brand/brand-form-dialogs"
import { BrandDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useDictionaryListState } from "@/features/master-data/hooks/use-dictionary-list-state"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import {
    masterDataCopy,
    masterDataSearchPlaceholder,
} from "@/features/master-data/lib/copy"

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
    const { filters } = state
    const hasActiveFilters =
        filters.q.trim() !== "" ||
        filters.lifecycleStatus !== "all" ||
        filters.revisionTiming !== "all"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data

    return (
        <ListPageFrame
            title="品牌"
            description="查看品牌字典，供商品与 SKU 选用。"
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: "master-data-brands-list-export",
                    actionKey: "export",
                    label: exportPending
                        ? "导出中…"
                        : masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    loading: exportPending,
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-brands-list-import",
                    actionKey: "import",
                    label: "批量导入",
                    icon: UploadIcon,
                    variant: "outline",
                    hidden: !state.canCreate,
                    onClick: () => setImportOpen(true),
                },
                {
                    id: "master-data-brands-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    icon: PlusIcon,
                    hidden: !state.canCreate,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => state.setCreateOpen(true),
                },
            ]}
            resultsLabel={`品牌 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <ListWorkSurface
                ariaLabel="品牌列表"
                views={
                    <LifecycleMetricStrip
                        idPrefix="master-data-brands-list-metrics"
                        metrics={state.syncedMetrics}
                        metricKey={filters.metricKey}
                        ariaLabel="品牌指标筛选"
                        allLabel="全部品牌"
                        hint="选择品牌查看详情"
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                }
                toolbar={
                    <DictionaryListToolbar
                        idPrefix="master-data-brands-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        searchPlaceholder={masterDataSearchPlaceholder(
                            "brands",
                        )}
                        countLabel="品牌"
                        resultCount={
                            state.listQuery.data ? state.rows.length : undefined
                        }
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                    />
                }
                table={
                    <DictionaryCompactList
                        id="master-data-brands-list"
                        rows={state.rows}
                        codeLabel="品牌代码"
                        renderCard={(row) => <BrandCardContent row={row} />}
                        selectedId={state.previewId}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        loading={state.listQuery.isFetching}
                        listLoadFailed={listLoadFailed}
                        error={state.listQuery.error}
                        onRetry={() => void state.listQuery.refetch()}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={filters.clearAllFilters}
                        emptyTitle="还没有品牌资料"
                        emptyDescription="点击「新建」创建品牌，或使用「批量导入」一次建立多个品牌。"
                        emptyAction={
                            state.canCreate ? (
                                <Button
                                    id="master-data-brands-list-empty-create"
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
            <BrandPreviewDialog
                row={state.previewRow}
                lastFocusedRowId={lastFocusedRowId}
                onClose={() => state.setPreviewId(null)}
                onRevise={state.setReviseTarget}
                onDisable={state.setDisableTarget}
            />
            {importOpen && (
                <BrandImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            )}
            <BrandCreateDialog
                open={state.createOpen}
                onOpenChange={state.setCreateOpen}
            />
            <BrandReviseDialog
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
            <BrandDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </ListPageFrame>
    )
}
