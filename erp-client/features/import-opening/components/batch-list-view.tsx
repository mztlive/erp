"use client"

import { ShieldAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    MetricItem,
    MetricStrip,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    ListWorkspaceViews,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { BatchListToolbar } from "@/features/import-opening/components/batch-list-toolbar"
import { useImportBatchListQuery } from "@/features/import-opening/hooks/queries"
import { useBatchListColumns } from "@/features/import-opening/hooks/use-batch-list-columns"
import { useBatchListFilters } from "@/features/import-opening/hooks/use-batch-list-filters"
import { useBatchPagination } from "@/features/import-opening/hooks/use-batch-pagination"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type { ImportEnvironment } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

const ENVIRONMENT_VIEWS: ReadonlyArray<{
    value: ImportEnvironment
    label: string
    id: string
}> = [
    {
        value: "VALIDATION",
        label: "验证环境",
        id: "operations-import-batches-filter-environment-validation-trigger",
    },
    {
        value: "PRODUCTION",
        label: "生产环境",
        id: "operations-import-batches-filter-environment-production-trigger",
    },
]

export function BatchListView({
    urlState,
    patchUrl,
}: {
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
}) {
    const filters = useBatchListFilters({ urlState, patchUrl })
    const listQuery = useImportBatchListQuery({
        environment: urlState.environment,
        status: filters.appliedStatus,
        objectType: filters.appliedObjectType ?? "all",
        q: urlState.q,
        page: urlState.page,
        pageSize: 20,
    })

    const columns = useBatchListColumns({
        onOpenBatch: (batchId) =>
            patchUrl({ batchId, section: "overview", page: 1 }),
    })

    const data = listQuery.data
    const { pagination, setPagination } = useBatchPagination(urlState.page)
    const listLoadFailed = listQuery.isError || !listQuery.data

    return (
        <PageScaffold density="compact" className={listWorkspaceStyles.page}>
            <ListWorkspaceHeader
                className="pb-6 md:pb-6"
                eyebrow="治理"
                title="导入与期初"
                description={
                    <>
                        查看导入批次与期初处理进度。
                        <span className="ml-3 text-xs" role="status">
                            {listQuery.isError ? (
                                "查询失败"
                            ) : listQuery.isFetching ? (
                                "正在更新…"
                            ) : data?.queriedAt ? (
                                <time dateTime={data.queriedAt}>
                                    更新于{" "}
                                    {formatDateTime(
                                        data.queriedAt,
                                        "dateStyle",
                                        "passthrough",
                                    )}
                                </time>
                            ) : (
                                "正在查询"
                            )}
                        </span>
                    </>
                }
            />

            <MetricStrip className="mb-4" columns={4} aria-label="导入批次指标">
                <MetricItem
                    label="待校验"
                    value={data?.metrics.pendingValidate ?? "—"}
                />
                <MetricItem
                    label="待业务确认"
                    value={data?.metrics.pendingConfirm ?? "—"}
                />
                <MetricItem
                    label="执行中"
                    value={data?.metrics.applying ?? "—"}
                />
                <MetricItem
                    label="失败/部分失败"
                    value={data?.metrics.failedOrPartial ?? "—"}
                />
            </MetricStrip>

            <Alert>
                <ShieldAlertIcon />
                <AlertTitle>安全边界</AlertTitle>
                <AlertDescription>
                    本页不展示原始
                    SQL、数据库连接头、禁止字段或存储对象键。不合规导出只能在受控临时区清洗后，以白名单合规包进入安全接收。
                </AlertDescription>
            </Alert>

            <ListWorkSurface
                toolbarClassName="pt-3 pb-2"
                ariaLabel="导入批次列表"
                views={
                    <ListWorkspaceViews
                        ariaLabel="导入环境"
                        hint={
                            urlState.environment === "PRODUCTION"
                                ? "生产环境 · 操作需显著确认"
                                : "验证环境"
                        }
                        items={ENVIRONMENT_VIEWS.map((item) => ({
                            id: item.id,
                            label: item.label,
                            active: item.value === urlState.environment,
                            onClick: () =>
                                patchUrl({
                                    environment: item.value,
                                    page: 1,
                                    batchId: undefined,
                                }),
                        }))}
                    />
                }
                toolbar={
                    <BatchListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.qDraft}
                        setSearchDraft={filters.setQDraft}
                        clearAllFilters={filters.clearAllBatchFilters}
                        appliedChips={filters.appliedChips}
                        removeFilter={filters.removeBatchFilter}
                        batchFilterPanelOpen={filters.batchFilterPanelOpen}
                        setBatchFilterPanelOpen={
                            filters.setBatchFilterPanelOpen
                        }
                        applyBatchFilters={filters.applyBatchFilters}
                        resetMoreFilters={filters.resetMoreBatchFilters}
                        objectTypeDraft={filters.objectTypeDraft}
                        setObjectTypeDraft={filters.setObjectTypeDraft}
                        statusDraft={filters.statusDraft}
                        setStatusDraft={filters.setStatusDraft}
                        hasPendingChanges={filters.hasPendingChanges}
                        resultCount={data?.totalCount}
                        loading={listQuery.isPending || listQuery.isFetching}
                        failed={listQuery.isError}
                    />
                }
                table={
                    <DataTable
                        id="operations-import-batches-table"
                        data={[...(data?.rows ?? [])]}
                        columns={columns}
                        defaultColumnVisibility={{
                            environment: false,
                            rule: false,
                            stage: false,
                        }}
                        defaultColumnPinning={{ left: ["batchNo"] }}
                        getRowId={(row) => row.batchId}
                        rowCount={data?.totalCount ?? 0}
                        pagination={pagination}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            patchUrl({ page: next.pageIndex + 1 })
                        }}
                        layout="flush"
                        loading={listQuery.isPending}
                        errorState={
                            listQuery.isError ? (
                                <BusinessFailureState
                                    title="批次列表加载失败"
                                    error={listQuery.error}
                                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                                    action={
                                        <Button
                                            id="operations-import-batches-table-retry"
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={() =>
                                                void listQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !listLoadFailed &&
                            (data?.rows.length ?? 0) === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        filters.hasAppliedBatchFilters
                                            ? "filter"
                                            : "no-data"
                                    }
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        filters.hasAppliedBatchFilters
                                            ? "当前筛选无结果"
                                            : "还没有导入批次"
                                    }
                                    description={
                                        filters.hasAppliedBatchFilters
                                            ? "没有批次符合当前筛选条件，可清除筛选后重试。"
                                            : "当前环境还没有导入批次。"
                                    }
                                    action={
                                        filters.hasAppliedBatchFilters ? (
                                            <Button
                                                id="operations-import-batches-table-clear-filters"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={
                                                    filters.clearAllBatchFilters
                                                }
                                            >
                                                清除筛选
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />
        </PageScaffold>
    )
}
