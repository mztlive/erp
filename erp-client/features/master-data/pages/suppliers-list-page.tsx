"use client"

import { useState } from "react"
import { SupplierImportDialog } from "@/features/master-data/components/supplier/supplier-import-dialog"
import { useRouter } from "@/lib/navigation"
import { DownloadIcon, UploadIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
} from "@/components/business"
import {
    ListWorkSurface,
    listWorkspaceEmptyStateClassName,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { toast } from "@/components/ui/toast"
import { LifecycleMetricStrip } from "@/features/master-data/components/list/lifecycle-metric-strip"
import {
    MasterDataClearFiltersButton,
    MasterDataEmptyCreateButton,
} from "@/features/master-data/components/list/list-empty-actions"
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { SupplierListToolbar } from "@/features/master-data/components/list/supplier-list-toolbar"
import { suppliersListStyles } from "./suppliers-list-styles"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSupplierListColumns } from "@/features/master-data/hooks/use-supplier-list-columns"
import { useSupplierListState } from "@/features/master-data/hooks/use-supplier-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    masterDataCreateAction,
    masterDataExportAction,
} from "@/features/master-data/lib/list-page-actions"
import type { BackgroundJobView } from "@/lib/background-jobs"
import { launchNavDelivery } from "@/lib/nav-delivery"
import { downloadSupplierTemplate } from "@/features/master-data/lib/supplier-import"

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
            title="供应商与资质"
            description="查看供应商资料、资质与供货能力。"
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: "master-data-suppliers-list-import-template",
                    actionKey: "import-template",
                    label: "下载导入模板",
                    icon: DownloadIcon,
                    variant: "outline",
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
                    variant: "outline",
                    hidden: !state.canCreate,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => setImportOpen(true),
                },
                masterDataExportAction({
                    id: "master-data-suppliers-list-export",
                    pending: exportPending,
                    rowCount: state.rows.length,
                    onClick: state.onExport,
                }),
                masterDataCreateAction({
                    id: "master-data-suppliers-list-create",
                    canCreate: state.canCreate,
                    blockedReason: state.createBlockedReason,
                    onClick: () => router.push("/master-data/suppliers/new"),
                }),
            ]}
            resultsLabel={`供应商与资质 · ${state.rows.length} 条结果`}
            resultsHeadingRef={resultsHeadingRef}
            loading={state.listQuery.isPending}
        >
            <ListWorkSurface
                ariaLabel="供应商与资质列表"
                views={
                    <LifecycleMetricStrip
                        idPrefix="master-data-suppliers-list-metrics"
                        metrics={state.syncedMetrics}
                        metricKey={filters.metricKey}
                        ariaLabel="供应商与资质指标"
                        allLabel="全部供应商"
                        hint={
                            state.canReadDetail
                                ? "选择供应商查看详情"
                                : undefined
                        }
                        onChangeLifecycle={filters.changeLifecycle}
                    />
                }
                toolbar={
                    <SupplierListToolbar
                        idPrefix="master-data-suppliers-list-toolbar"
                        searchInputRef={searchInputRef}
                        filters={filters}
                        appliedChips={state.appliedChips}
                        resultCount={
                            state.listQuery.data ? state.rows.length : undefined
                        }
                        loading={state.listQuery.isFetching}
                        failed={state.listQuery.isError}
                    />
                }
                tableClassName={suppliersListStyles.table}
                table={
                    <DataTable
                        id="master-data-suppliers-list-table"
                        data={state.pageRows}
                        columns={columns}
                        defaultColumnVisibility={{
                            revisionNo: false,
                            invoice: false,
                            businessCategory: false,
                        }}
                        getRowId={(row) => row.stableId}
                        rowCount={state.rows.length}
                        pagination={filters.pagination}
                        onPaginationChange={filters.changePagination}
                        loading={state.listQuery.isFetching}
                        layout="flush"
                        defaultColumnPinning={{
                            left: ["name"],
                        }}
                        errorState={
                            listLoadFailed ? (
                                <BusinessFailureState
                                    error={state.listQuery.error}
                                    action={
                                        <Button
                                            id="master-data-suppliers-list-retry"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                void state.listQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed && state.rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        noScope
                                            ? "no-scope"
                                            : hasActiveFilters
                                              ? "filter"
                                              : "no-data"
                                    }
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        noScope
                                            ? "暂无可查看的供应商"
                                            : hasActiveFilters
                                              ? "当前筛选无结果"
                                              : "还没有供应商与资质资料"
                                    }
                                    description={
                                        noScope
                                            ? "您当前尚未分配供应商的数据查看权限，请联系管理员开通。"
                                            : hasActiveFilters
                                              ? masterDataCopy.filterEmptyDescription
                                              : state.canCreate
                                                ? "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                                : undefined
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <MasterDataClearFiltersButton
                                                id="master-data-suppliers-list-empty-clear-filters"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            />
                                        ) : state.canCreate ? (
                                            <MasterDataEmptyCreateButton
                                                id="master-data-suppliers-list-empty-create"
                                                onClick={() =>
                                                    router.push(
                                                        "/master-data/suppliers/new",
                                                    )
                                                }
                                            />
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
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
                }
            />
            {state.canCreate && importOpen && (
                <SupplierImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            )}
        </ListPageFrame>
    )
}
