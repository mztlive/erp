"use client"

import { useState } from "react"
import { SupplierImportDialog } from "@/features/master-data/components/supplier/supplier-import-dialog"
import { useRouter } from "next/navigation"
import { DownloadIcon, PlusIcon, UploadIcon } from "lucide-react"
import { useIsMutating } from "@tanstack/react-query"

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
import { ListPageFrame } from "@/features/master-data/components/list/list-page-frame"
import { SupplierListToolbar } from "@/features/master-data/components/list/supplier-list-toolbar"
import { suppliersListStyles } from "./suppliers-list-styles"
import { useListPageChrome } from "@/features/master-data/hooks/use-list-page-chrome"
import { useSupplierListColumns } from "@/features/master-data/hooks/use-supplier-list-columns"
import { useSupplierListState } from "@/features/master-data/hooks/use-supplier-list-state"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { BackgroundJobView } from "@/features/background-jobs/api"
import { launchNavDelivery } from "@/lib/nav-delivery"

/** 导入动作按钮与投递落点：动作 id 同时是动画起点。 */
const IMPORT_ACTION_ID = "master-data-suppliers-list-import"
const BACKGROUND_TASKS_HREF = "/governance/background-jobs"

export function SuppliersListPage() {
    const router = useRouter()
    const [importOpen, setImportOpen] = useState(false)
    const { searchInputRef, resultsHeadingRef, lastFocusedRowId } =
        useListPageChrome()
    const state = useSupplierListState(searchInputRef)
    const exportPending =
        useIsMutating({
            predicate: (mutation) => {
                const variables = mutation.state.variables
                return (
                    typeof variables === "object" &&
                    variables !== null &&
                    "resource" in variables &&
                    variables.resource === "suppliers" &&
                    !("idempotencyKey" in variables)
                )
            },
        }) > 0
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
            title="供应商与资质"
            description="查看供应商资料、资质与供货能力。"
            exportMeta={state.exportMeta}
            actions={[
                {
                    id: IMPORT_ACTION_ID,
                    actionKey: "import",
                    label: "导入",
                    icon: UploadIcon,
                    variant: "outline",
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => setImportOpen(true),
                },
                {
                    id: "master-data-suppliers-list-export",
                    actionKey: "export",
                    label: exportPending
                        ? "导出中…"
                        : masterDataCopy.actionExport,
                    icon: DownloadIcon,
                    variant: "outline",
                    disabled: exportPending || state.rows.length === 0,
                    onClick: state.onExport,
                },
                {
                    id: "master-data-suppliers-list-create",
                    actionKey: "create",
                    label: masterDataCopy.actionCreate,
                    icon: PlusIcon,
                    disabled: !state.canCreate,
                    title: !state.canCreate
                        ? state.createBlockedReason
                        : undefined,
                    onClick: () => router.push("/master-data/suppliers/new"),
                },
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
                        hint="选择供应商查看详情"
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
                        ownerOptions={state.listQuery.data?.ownerOptions ?? []}
                        capabilityOwnerOptions={
                            state.listQuery.data?.capabilityOwnerOptions ?? []
                        }
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
                                            ? "当前角色无供应商范围"
                                            : hasActiveFilters
                                              ? "当前筛选无结果"
                                              : "还没有供应商与资质资料"
                                    }
                                    description={
                                        noScope
                                            ? "当前权限与数据范围内没有供应商；不代表系统尚无供应商。"
                                            : hasActiveFilters
                                              ? "没有记录符合当前筛选条件，可清除筛选后重试。"
                                              : "点击「新建」创建第一份资料；历史记录会随资料保留。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id="master-data-suppliers-list-empty-clear-filters"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={
                                                    filters.clearAllFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        ) : state.canCreate ? (
                                            <Button
                                                id="master-data-suppliers-list-empty-create"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={() =>
                                                    router.push(
                                                        "/master-data/suppliers/new",
                                                    )
                                                }
                                            >
                                                {masterDataCopy.actionCreate}
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                        onRowPreview={(row) => openDetail(row.stableId)}
                        onRowOpen={(row) => openDetail(row.stableId)}
                    />
                }
            />
            {importOpen && (
                <SupplierImportDialog
                    onClose={() => setImportOpen(false)}
                    onSubmitted={onImportSubmitted}
                />
            )}
        </ListPageFrame>
    )
}
