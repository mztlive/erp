"use client"

import { useState } from "react"
import { DownloadIcon, UploadIcon } from "lucide-react"

import { ListDataTable } from "@/components/business/list-workspace"
import { toast } from "@/components/ui/toast"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { SupplierListToolbar } from "@/features/master-data/components/list/supplier-list-toolbar"
import { SupplierImportDialog } from "@/features/master-data/components/supplier/supplier-import-dialog"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSupplierListColumns } from "@/features/master-data/hooks/use-supplier-list-columns"
import { useSupplierListState } from "@/features/master-data/hooks/use-supplier-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"
import { downloadSupplierTemplate } from "@/features/master-data/lib/supplier-import"
import type { BackgroundJobView } from "@/lib/background-jobs"
import { launchNavDelivery } from "@/lib/nav-delivery"
import { useRouter } from "@/lib/navigation"

import { suppliersListStyles } from "./suppliers-list-styles"

/** 导入动作按钮与投递落点：动作 id 同时是动画起点。 */
const IMPORT_ACTION_ID = "master-data-suppliers-list-import"
const BACKGROUND_TASKS_HREF = "/governance/background-jobs"

export function SuppliersListPage() {
    const router = useRouter()
    const [importOpen, setImportOpen] = useState(false)
    const [templatePending, setTemplatePending] = useState(false)
    const onDownloadTemplate = async () => {
        setTemplatePending(true)
        try {
            await downloadSupplierTemplate()
        } catch {
            toast.add({ title: "模板下载失败，请重试", type: "error" })
        } finally {
            setTemplatePending(false)
        }
    }
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSupplierListState(searchInputRef)
    const exportPending = state.exportPending
    const { filters } = state
    const openDetail = (stableId: string) => {
        lastFocusedRowId.current = stableId
        router.push(`/master-data/suppliers/${stableId}?section=overview`)
    }
    const openCreate = () => router.push("/master-data/suppliers/new")
    const columns = useSupplierListColumns()
    const hasActiveFilters =
        filters.q.trim() !== "" || filters.hasStructuredSupplierFilters
    const noScope = state.listQuery.data?.emptyReason === "no_scope"
    const listLoadFailed = state.listQuery.isError || !state.listQuery.data
    // 导入只做任务投递：本页不留进度，用曲线把任务飞到侧栏「后台任务」，结果由 toast 交代。
    const onImportSubmitted = (job: BackgroundJobView, fileName: string) => {
        launchNavDelivery(
            "background-task",
            document.getElementById(IMPORT_ACTION_ID),
        )
        toast.add({
            title: "导入任务已提交",
            description: `「${fileName || "所选文件"}」共 ${job.total_count} 行，正在后台逐行导入，进度与结果在「后台任务」查看。`,
            type: "success",
            timeout: 6000,
            actionProps: {
                children: "查看任务",
                onClick: () => router.push(BACKGROUND_TASKS_HREF),
            },
        })
    }

    return (
        <ListPageFrame
            resource="suppliers"
            eyebrow=""
            title="供应商与资质"
            views={
                state.syncedMetrics.length > 1 ? (
                    <LifecycleMetricStrip
                        idPrefix="master-data-suppliers-list-metrics"
                        metrics={state.syncedMetrics}
                        metricKey={filters.metricKey}
                        ariaLabel="供应商与资质指标"
                        allLabel="全部供应商"
                        variant="inline"
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                ) : undefined
            }
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: "master-data-suppliers-list-import-template",
                    actionKey: "import-template",
                    label: "下载导入模板",
                    icon: DownloadIcon,
                    tier: "quiet",
                    hidden: !state.canCreate,
                    disabled: templatePending,
                    loading: templatePending,
                    onClick: () => void onDownloadTemplate(),
                },
                {
                    id: IMPORT_ACTION_ID,
                    actionKey: "import",
                    label: "导入",
                    icon: UploadIcon,
                    tier: "quiet",
                    hidden: !state.canCreate,
                    onClick: () => setImportOpen(true),
                },
                {
                    ...masterDataExportAction({
                        id: "master-data-suppliers-list-export",
                        pending: exportPending,
                        rowCount: state.rows.length,
                        onClick: state.onExport,
                    }),
                    tier: "quiet",
                },
                masterDataCreateAction({
                    id: "master-data-suppliers-list-create",
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: openCreate,
                }),
            ]}
            resultsLabel={`供应商与资质 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
            surface={{
                ariaLabel: "供应商与资质列表",
                toolbar: (
                    <SupplierListToolbar
                        idPrefix="master-data-suppliers-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        appliedChips={state.appliedChips}
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                    />
                ),
                tableClassName: suppliersListStyles.table,
                table: (
                    <ListDataTable
                        id="master-data-suppliers-list-table"
                        entityName="供应商"
                        caption="供应商与资质"
                        data={state.pageRows}
                        columns={columns}
                        defaultColumnVisibility={{
                            revisionNo: false,
                            invoice: false,
                            businessCategory: false,
                        }}
                        defaultColumnOrder={[
                            "name",
                            "lifecycle",
                            "maintainer",
                            "capability",
                            "qualification",
                            "settlement",
                            "entities",
                            "invoice",
                            "businessCategory",
                            "revisionNo",
                        ]}
                        defaultColumnPinning={{ left: ["name"] }}
                        getRowId={(row) => row.stableId}
                        rowLabel={(row) => `${row.name}（${row.stableNo}）`}
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        loading={state.listQuery.isFetching}
                        showToolbarSummary={false}
                        isError={listLoadFailed}
                        error={state.listQuery.error}
                        onRetry={() => void state.listQuery.refetch()}
                        retryButtonId="master-data-suppliers-list-retry"
                        noScope={noScope}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={
                            noScope ? undefined : filters.clearAllFilters
                        }
                        clearFiltersButtonId="master-data-suppliers-list-empty-clear-filters"
                        canCreate={state.canCreate}
                        onCreate={openCreate}
                        createButtonId="master-data-suppliers-list-empty-create"
                        createLabel={masterDataCopy.actionCreate}
                        emptyCopy={{
                            noScopeDescription:
                                "您当前尚未分配供应商的数据查看权限，请联系管理员开通。",
                            filterDescription:
                                masterDataCopy.filterEmptyDescription,
                            emptyTitle: "还没有供应商与资质资料",
                        }}
                        onRowPreview={
                            state.canReadDetail
                                ? (row) => openDetail(row.stableId)
                                : undefined
                        }
                        onRowOpen={
                            state.canReadDetail
                                ? (row) => openDetail(row.stableId)
                                : undefined
                        }
                    />
                ),
            }}
        >
            {state.canCreate && importOpen && (
                <SupplierImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            )}
        </ListPageFrame>
    )
}
