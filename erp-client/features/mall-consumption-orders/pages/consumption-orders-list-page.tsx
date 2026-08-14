"use client"

import * as React from "react"
import type { ColumnPinningState } from "@tanstack/react-table"
import { DownloadIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessTableFrame,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { ConsumptionMetricStrip } from "@/features/mall-consumption-orders/components/list-page/metric-strip"
import { buildConsumptionOrderColumns } from "@/features/mall-consumption-orders/components/list-page/columns"
import { ConsumptionOrderFilterBar } from "@/features/mall-consumption-orders/components/list-page/filter-bar"
import {
    ExportPreviewPanel,
    ExportResultPanel,
} from "@/features/mall-consumption-orders/components/list-page/export-panels"
import { ConsumptionOrdersTable } from "@/features/mall-consumption-orders/components/list-page/orders-table"
import { ConsumptionOrderPreviewSheet } from "@/features/mall-consumption-orders/components/list-page/preview-sheet"
import { useConsumptionOrderListQuery } from "@/features/mall-consumption-orders/hooks/queries"
import { useConsumptionOrderExportFlow } from "@/features/mall-consumption-orders/hooks/use-consumption-order-export"
import { useSearchDraft } from "@/features/mall-consumption-orders/hooks/use-search-draft"
import { useConsumptionOrdersUrlState } from "@/features/mall-consumption-orders/hooks/use-consumption-orders-url-state"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { formatDateTime } from "@/lib/datetime"

export function ConsumptionOrdersListPage() {
    const {
        qParam,
        mallId,
        fulfillmentChain,
        attributionStatus,
        paymentSource,
        costBasis,
        occurredFrom,
        occurredTo,
        factTypes,
        supplierStatuses,
        dataSources,
        periodSelected,
        metric,
        previewId,
        pagination,
        listQueryInput,
        hasActiveFilters,
        listReturnHref,
        replaceParams,
        handlePaginationChange,
        openPreview,
        closePreview,
        clearFilters,
        toggleMetric,
    } = useConsumptionOrdersUrlState()

    const { searchInput, setSearchInput, searchInputRef, commitSearch } =
        useSearchDraft({ qParam, replaceParams })

    const [columnPinning] = React.useState<ColumnPinningState>({
        left: ["mallOrder"],
        right: ["actions"],
    })

    const listQuery = useConsumptionOrderListQuery(listQueryInput, {
        enabled: periodSelected,
    })
    const data = listQuery.data
    const rows = data?.rows ?? []
    const metrics = data?.metrics ?? []
    const empty = data?.emptyReason

    const {
        exportPreviewOpen,
        exportResult,
        exportMutation,
        confirmExport,
        openExportPreview,
        cancelExportPreview,
    } = useConsumptionOrderExportFlow(
        data?.pageInfo.total ?? 0,
        data?.filterSummary ?? "",
    )

    const columns = React.useMemo(
        () => buildConsumptionOrderColumns(listReturnHref),
        [listReturnHref],
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="商城消费订单"
                breadcrumbs={[
                    {
                        id: "com",
                        label: "商城与发布",
                        href: "/commerce/consumption-orders",
                    },
                    { id: "co", label: "商城消费订单", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt={
                            data
                                ? formatDateTime(
                                      data.factWatermark,
                                      "monthDay",
                                      "passthrough",
                                  )
                                : "—"
                        }
                        dateTime={data?.factWatermark}
                        state={listQuery.isFetching ? "syncing" : "fresh"}
                        label="记录更新"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "refresh",
                                label: "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                onClick: () => {
                                    void listQuery.refetch()
                                },
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled:
                                    !data ||
                                    data.pageInfo.total === 0 ||
                                    exportMutation.isPending ||
                                    empty === "NO_PERMISSION" ||
                                    empty === "NO_SCOPE",
                                onClick: openExportPreview,
                            },
                        ]}
                    />
                }
            />

            <Alert
                variant="info"
                className="gap-2 py-2 lg:grid-cols-[auto_minmax(0,1fr)_auto] lg:items-center lg:gap-3"
            >
                <AlertTitle className="whitespace-nowrap">
                    只读记录追溯
                </AlertTitle>
                <AlertDescription className="min-w-0 lg:truncate">
                    {data?.boundaryNotice ??
                        "本页只读：不修改支付状态、不编辑分摊、不重试供应商动作；导出与信息揭示均有审计。"}
                </AlertDescription>
            </Alert>

            {exportResult ? <ExportResultPanel result={exportResult} /> : null}

            {exportPreviewOpen ? (
                <ExportPreviewPanel
                    filterSummary={data?.filterSummary ?? "—"}
                    total={data?.pageInfo.total ?? 0}
                    isPending={exportMutation.isPending}
                    onConfirm={() => {
                        void confirmExport()
                    }}
                    onCancel={cancelExportPreview}
                />
            ) : null}

            {empty === "NO_PERMISSION" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无模块权限"
                    description="当前角色无权访问商城消费订单。不显示无权限范围的指标。"
                />
            ) : empty === "NO_SCOPE" ? (
                <BusinessEmptyState
                    kind="no-scope"
                    title="无数据范围"
                    description="你可进入此页面，但授权商城/客户范围内没有可查看消费订单。不显示无权限范围的指标。"
                />
            ) : (
                <>
                    {/* 指标与普通筛选 AND 共存：指标点击不清理其它筛选（避免隐藏行为）；
              矛盾组合无结果时由「当前筛选无结果」空态解释并引导清除。 */}
                    <ConsumptionMetricStrip
                        metrics={metrics}
                        activeMetric={metric}
                        periodSelected={periodSelected}
                        onToggleMetric={toggleMetric}
                    />
                    {!periodSelected ? (
                        <p className="text-xs text-muted-foreground">
                            选择记录发生起止时间后，可点击指标快捷筛选。
                        </p>
                    ) : null}

                    <BusinessTableFrame
                        title="消费订单列表"
                        description="商城订单与操作列固定；金额为人民币含税实付。Enter 打开预览抽屉。"
                        toolbar={
                            <ConsumptionOrderFilterBar
                                searchInput={searchInput}
                                searchInputRef={searchInputRef}
                                onSearchInputChange={setSearchInput}
                                onCommitSearch={commitSearch}
                                searchPending={searchInput.trim() !== qParam}
                                filterSummary={data?.filterSummary}
                                mallId={mallId}
                                attributionStatus={attributionStatus}
                                fulfillmentChain={fulfillmentChain}
                                paymentSource={paymentSource}
                                costBasis={costBasis}
                                occurredFrom={occurredFrom}
                                occurredTo={occurredTo}
                                factTypes={factTypes}
                                supplierStatuses={supplierStatuses}
                                dataSources={dataSources}
                                hasActiveFilters={hasActiveFilters}
                                onClearFilters={clearFilters}
                                onReplaceParams={replaceParams}
                            />
                        }
                        table={
                            <ConsumptionOrdersTable
                                periodSelected={periodSelected}
                                isPending={listQuery.isPending}
                                isError={listQuery.isError}
                                error={listQuery.error}
                                empty={empty}
                                rows={rows}
                                columns={columns}
                                columnPinning={columnPinning}
                                pagination={pagination}
                                rowCount={data?.pageInfo.total ?? 0}
                                isFetching={listQuery.isFetching}
                                onPaginationChange={handlePaginationChange}
                                onRowPreview={(row) =>
                                    openPreview(row.mallOrderId)
                                }
                                onClearFilters={clearFilters}
                                onRetry={() => {
                                    void listQuery.refetch()
                                }}
                            />
                        }
                    />

                    <div className="flex flex-wrap gap-2">
                        <Badge variant="secondary">
                            仅支持卡券与微信两种支付来源
                        </Badge>
                        <Badge variant="outline">无福利账户支付</Badge>
                        <Badge variant="outline">
                            列表 {data?.pageInfo.total ?? 0} 条 · 每页{" "}
                            {pagination.pageSize} 条
                        </Badge>
                    </div>
                </>
            )}

            <ConsumptionOrderPreviewSheet
                previewId={previewId}
                onClose={closePreview}
                listReturnHref={listReturnHref}
            />
        </PageScaffold>
    )
}
