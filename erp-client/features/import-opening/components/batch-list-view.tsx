"use client"

import * as React from "react"
import { ShieldAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    MetricItem,
    MetricStrip,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { BatchListToolbar } from "@/features/import-opening/components/batch-list-toolbar"
import { useImportBatchListQuery } from "@/features/import-opening/hooks/queries"
import { useBatchListColumns } from "@/features/import-opening/hooks/use-batch-list-columns"
import { useBatchListFilters } from "@/features/import-opening/hooks/use-batch-list-filters"
import { useBatchPagination } from "@/features/import-opening/hooks/use-batch-pagination"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type { ImportEnvironment } from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    ENVIRONMENT_LABEL,
    OBJECT_CODE_LABEL,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

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

    /** 表头说明：有筛选时写人读摘要，否则默认操作说明（§2.1）。 */
    const tableDescription = React.useMemo(() => {
        if (listQuery.isError) {
            return "列表加载失败，可调整筛选后重试"
        }
        const active: string[] = []
        if (urlState.q?.trim()) active.push(`搜索「${urlState.q.trim()}」`)
        if (filters.appliedObjectType) {
            active.push(
                `对象 ${OBJECT_CODE_LABEL[filters.appliedObjectType]}`,
            )
        }
        if (filters.appliedStatus) {
            active.push(`状态 ${BATCH_STATUS_LABEL[filters.appliedStatus]}`)
        }
        if (active.length === 0) {
            return `${ENVIRONMENT_LABEL[urlState.environment]} · 展示全部批次，点击批次号查看详情。`
        }
        return `${ENVIRONMENT_LABEL[urlState.environment]} · 当前筛选：${active.join(" · ")}`
    }, [
        filters.appliedObjectType,
        filters.appliedStatus,
        listQuery.isError,
        urlState.environment,
        urlState.q,
    ])

    const listLoadFailed = listQuery.isError || !listQuery.data

    return (
        <PageScaffold>
            <PageHeader
                title="导入与期初"
                metadata={
                    <DataFreshness
                        updatedAt={
                            data?.queriedAt
                                ? formatDateTime(
                                      data.queriedAt,
                                      "dateStyle",
                                      "passthrough",
                                  )
                                : "刚刚"
                        }
                        dateTime={data?.queriedAt}
                        state={listQuery.isFetching ? "stale" : "fresh"}
                        label="导入批次"
                    />
                }
            />

            <div className="flex flex-wrap items-center gap-3">
                <Label className="text-sm text-muted-foreground">环境</Label>
                <Tabs
                    value={urlState.environment}
                    onValueChange={(v) => {
                        if (v == null) return
                        patchUrl({
                            environment: v as ImportEnvironment,
                            page: 1,
                            batchId: undefined,
                        })
                    }}
                >
                    <TabsList>
                        <TabsTrigger value="VALIDATION">验证环境</TabsTrigger>
                        <TabsTrigger value="PRODUCTION">生产环境</TabsTrigger>
                    </TabsList>
                </Tabs>
                {urlState.environment === "PRODUCTION" ? (
                    <Badge variant="destructive">
                        生产环境 · 操作需显著确认
                    </Badge>
                ) : (
                    <Badge variant="secondary">验证环境</Badge>
                )}
            </div>

            <MetricStrip columns={4} aria-label="导入批次指标">
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

            <BusinessTableFrame
                showHeader
                title={
                    <span className="inline-flex items-baseline gap-2">
                        导入批次
                        <span
                            aria-live="polite"
                            className="font-normal text-muted-foreground"
                        >
                            {(data?.totalCount ?? 0).toLocaleString("zh-CN")}{" "}
                            批
                        </span>
                    </span>
                }
                description={tableDescription}
                toolbar={
                    <BatchListToolbar
                        searchInputRef={filters.searchInputRef}
                        searchDraft={filters.qDraft}
                        setSearchDraft={filters.setQDraft}
                        hasActiveFilters={filters.hasAppliedBatchFilters}
                        clearAllFilters={filters.clearAllBatchFilters}
                        appliedChips={filters.appliedChips}
                        removeFilter={filters.removeBatchFilter}
                        batchFilterPanelOpen={filters.batchFilterPanelOpen}
                        setBatchFilterPanelOpen={
                            filters.setBatchFilterPanelOpen
                        }
                        hasStructuredBatchFilters={
                            filters.hasStructuredBatchFilters
                        }
                        applyBatchFilters={filters.applyBatchFilters}
                        resetMoreFilters={filters.resetMoreBatchFilters}
                        objectTypeDraft={filters.objectTypeDraft}
                        setObjectTypeDraft={filters.setObjectTypeDraft}
                        statusDraft={filters.statusDraft}
                        setStatusDraft={filters.setStatusDraft}
                    />
                }
                table={
                    <DataTable
                        data={[...(data?.rows ?? [])]}
                        columns={columns}
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
                                    className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
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
