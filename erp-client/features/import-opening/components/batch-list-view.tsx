"use client"

import * as React from "react"
import { ShieldAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    ListDataTable,
    ListWorkspacePage,
    listWorkspaceEmptyStateClassName,
} from "@/components/business"
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
import { useRouter } from "@/lib/navigation"

const TABLE_ID = "operations-import-batches-table"
const CLEAR_FILTERS_ID = `${TABLE_ID}-clear-filters`
const PAGE_SIZE = 20
const EMPTY_COPY = {
    emptyDescription: "当前环境还没有导入批次。",
    filterDescription: "没有批次符合当前筛选条件，可清除筛选后重试。",
}

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
    const router = useRouter()
    const filters = useBatchListFilters({ urlState, patchUrl })
    const listQuery = useImportBatchListQuery({
        environment: urlState.environment,
        objectType: filters.appliedObjectType ?? "all",
        q: urlState.q,
        page: urlState.page,
        pageSize: PAGE_SIZE,
    })

    const openBatch = React.useCallback(
        (batchId: string) => {
            const params = new URLSearchParams()
            if (urlState.environment === "PRODUCTION") {
                params.set("environment", "PRODUCTION")
            }
            const qs = params.toString()
            const base = `/governance/imports/${encodeURIComponent(batchId)}`
            router.push(qs ? `${base}?${qs}` : base)
        },
        [router, urlState.environment],
    )
    const columns = useBatchListColumns({ onOpenBatch: openBatch })
    const data = listQuery.data
    const rows = React.useMemo(() => [...(data?.rows ?? [])], [data?.rows])
    const { pagination, setPagination } = useBatchPagination(urlState.page)
    const listLoadFailed =
        listQuery.isError || (!listQuery.isPending && data === undefined)
    const hasAppliedFilters = filters.hasAppliedBatchFilters
    // 数据类型在分页后筛选，当前页可能为空，但仍需保留服务端分页总数。
    const isEmptyPage = data != null && data.totalCount > 0 && rows.length === 0
    const isProduction = urlState.environment === "PRODUCTION"

    return (
        <ListWorkspacePage
            title="导入与期初"
            views={{
                ariaLabel: "导入环境",
                items: ENVIRONMENT_VIEWS.map((item) => ({
                    id: item.id,
                    label: item.label,
                    active: item.value === urlState.environment,
                    onClick: () =>
                        patchUrl({
                            environment: item.value,
                            page: 1,
                            batchId: undefined,
                        }),
                })),
            }}
            headerActions={
                !listLoadFailed && data?.queriedAt ? (
                    <time
                        className="num text-xs text-muted-foreground"
                        dateTime={data.queriedAt}
                    >
                        更新于{" "}
                        {formatDateTime(
                            data.queriedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                    </time>
                ) : undefined
            }
            alerts={
                <>
                    <section
                        data-slot="list-workspace-metrics"
                        aria-label="导入批次指标"
                        className="-mx-(--scaffold-px) border-b border-border/60 bg-card px-(--scaffold-px)"
                    >
                        <DetailWorkspaceMetrics
                            columns={4}
                            variant="bar"
                            ariaLabel="导入批次指标"
                        >
                            <DetailWorkspaceMetricItem
                                label="待校验"
                                value={data?.metrics.pendingValidate ?? "—"}
                            />
                            <DetailWorkspaceMetricItem
                                label="待业务确认"
                                value={data?.metrics.pendingConfirm ?? "—"}
                            />
                            <DetailWorkspaceMetricItem
                                label="执行中"
                                value={data?.metrics.applying ?? "—"}
                            />
                            <DetailWorkspaceMetricItem
                                label="失败/部分失败"
                                value={data?.metrics.failedOrPartial ?? "—"}
                            />
                        </DetailWorkspaceMetrics>
                    </section>
                    <Alert>
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>导入文件规范</AlertTitle>
                        <AlertDescription>
                            请上传按标准模板整理的期初数据文件，系统将在预检通过并经各部门核对后正式导入。
                            {isProduction
                                ? "生产环境 · 操作需显著确认。"
                                : null}
                        </AlertDescription>
                    </Alert>
                </>
            }
            surface={{
                ariaLabel: "导入批次列表",
                toolbar: (
                    <BatchListToolbar
                        filters={filters}
                        loading={listQuery.isFetching}
                        failed={listLoadFailed}
                    />
                ),
                table: (
                    <ListDataTable
                        id={TABLE_ID}
                        entityName="导入批次"
                        caption="导入批次列表"
                        data={rows}
                        columns={columns}
                        defaultColumnVisibility={{
                            environment: false,
                            rule: false,
                            stage: false,
                        }}
                        defaultColumnPinning={{ left: ["batchNo"] }}
                        getRowId={(row) => row.batchId}
                        rowLabel={(row) => row.batchNo}
                        onRowOpen={(row) => openBatch(row.batchId)}
                        rowCount={data?.totalCount ?? 0}
                        pagination={pagination}
                        pageSizeOptions={[PAGE_SIZE]}
                        onPaginationChange={(next) => {
                            setPagination(next)
                            patchUrl({ page: next.pageIndex + 1 })
                        }}
                        showToolbarSummary={false}
                        loading={listQuery.isPending || listQuery.isFetching}
                        isError={listLoadFailed}
                        error={listQuery.error}
                        onRetry={() => void listQuery.refetch()}
                        hasActiveFilters={hasAppliedFilters}
                        onClearFilters={filters.clearAllBatchFilters}
                        clearFiltersButtonId={CLEAR_FILTERS_ID}
                        emptyCopy={EMPTY_COPY}
                        emptyState={
                            isEmptyPage ? (
                                <BusinessEmptyState
                                    kind={
                                        hasAppliedFilters ? "filter" : "no-data"
                                    }
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        hasAppliedFilters
                                            ? "当前筛选无结果"
                                            : "当前页没有导入批次"
                                    }
                                    description={
                                        hasAppliedFilters
                                            ? EMPTY_COPY.filterDescription
                                            : "当前页没有可显示的批次。"
                                    }
                                    action={
                                        hasAppliedFilters ? (
                                            <Button
                                                id={CLEAR_FILTERS_ID}
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
                ),
            }}
        />
    )
}
