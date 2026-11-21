"use client"

import * as React from "react"
import Link from "next/link"
import { DownloadIcon, PlusIcon } from "lucide-react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useRouter } from "@/lib/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { BusinessFailureState } from "@/components/business"
import { ListWorkspacePage } from "@/components/business/list-workspace"
import { salesOrdersListStyles as salesStyles } from "./sales-orders-list-styles"
import { toast } from "@/components/ui/toast"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadSalesOrderContractPdf } from "@/features/sales-orders/api/sales-orders"
import { SalesOrdersListFilterBar } from "@/features/sales-orders/components/sales-orders-list-filter-bar"
import { SalesOrdersListExportResult } from "@/features/sales-orders/components/sales-orders-list-header"
import { SalesOrdersListTable } from "@/features/sales-orders/components/sales-orders-list-table"
import type { SalesOrderSummaryFilter } from "@/features/sales-orders/lib/filter-orders"
import { useSalesOrdersListChips } from "@/features/sales-orders/hooks/use-sales-orders-list-chips"
import { useSalesOrdersListExport } from "@/features/sales-orders/hooks/use-sales-orders-list-export"
import { useSalesOrdersListFilters } from "@/features/sales-orders/hooks/use-sales-orders-list-filters"
import { useSalesOrdersListQuery } from "@/features/sales-orders/hooks/use-sales-orders-list-query"
import { useSalesOrdersListUrlState } from "@/features/sales-orders/hooks/use-sales-orders-list-url-state"
import { salesOrdersListFiltersActive } from "@/features/sales-orders/lib/sales-orders-list-filters"
import { SORT_COLUMN_TO_FIELD } from "@/features/sales-orders/lib/sales-orders-list-query"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

const CREATE_HREF = "/sales/orders?mode=create"

const SUMMARY_FILTER_OPTIONS: ReadonlyArray<{
    value: SalesOrderSummaryFilter
    label: string
}> = [
    { value: "all", label: "全部" },
    { value: "mine", label: "待我处理" },
    { value: "createdByMe", label: "我创建的" },
    { value: "exception", label: "异常" },
]

/**
 * 销售单列表。实物/卡券销售单走各自审批入口。
 * SalesReturnCase 为 NO_APPROVAL，列表不展示销售退货审批区或审批动作。
 */
export function SalesOrdersListPage() {
    const router = useRouter()
    const account = useAccountProfileQuery()
    const canCreate = hasPermission(
        account.data?.permissions,
        "sales_order:create",
    )
    const canList = hasPermission(account.data?.permissions, "sales_order:list")
    const canReadContract = hasPermission(
        account.data?.permissions,
        "contract:detail",
    )
    const { url, pushUrl } = useSalesOrdersListUrlState()
    const { ordersQuery, query, identityReady } = useSalesOrdersListQuery(url, {
        canList,
        canReadContract,
    })
    const queryFailed = ordersQuery.isError || !identityReady
    const [listSettled, setListSettled] = React.useState(false)
    React.useEffect(() => {
        if (
            !listSettled &&
            canList &&
            (!ordersQuery.isPending || queryFailed)
        ) {
            setListSettled(true)
        }
    }, [canList, listSettled, ordersQuery.isPending, queryFailed])
    const listLoading =
        !queryFailed && (ordersQuery.isPending || ordersQuery.isFetching)
    const filters = useSalesOrdersListFilters(url, pushUrl)
    const { removeFilter, clearFilters } = filters

    const items = React.useMemo(
        () => ordersQuery.data?.items ?? [],
        [ordersQuery.data?.items],
    )
    const total = ordersQuery.data?.total ?? 0

    const { exportJob, exportCsv, isExporting } = useSalesOrdersListExport(
        query,
        total,
    )
    const chips = useSalesOrdersListChips(url, items, removeFilter, canList)

    const [downloadingContractId, setDownloadingContractId] = React.useState<
        string | null
    >(null)

    const handleRowNavigate = React.useCallback(
        (id: string) => {
            router.push(`/sales/orders/${id}`)
        },
        [router],
    )

    const downloadContract = React.useCallback(
        async (order: SalesOrderListItem) => {
            const contractId = order.contractId.trim()
            if (!contractId || downloadingContractId) return
            setDownloadingContractId(contractId)
            try {
                await downloadSalesOrderContractPdf(contractId)
            } catch (error) {
                toast.add({
                    title: "合同下载失败",
                    description: getErrorMessage(error, "请稍后重试"),
                    type: "error",
                    timeout: 4000,
                })
            } finally {
                setDownloadingContractId(null)
            }
        },
        [downloadingContractId],
    )

    const filtersActive = salesOrdersListFiltersActive(url)

    const pagination = React.useMemo<PaginationState>(
        () => ({
            pageIndex: Math.max(0, url.page - 1),
            pageSize: url.pageSize,
        }),
        [url.page, url.pageSize],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
        },
        [pushUrl],
    )

    const sorting = React.useMemo<SortingState>(
        () =>
            url.sort && SORT_COLUMN_TO_FIELD[url.sort]
                ? [{ id: url.sort, desc: url.dir === "desc" }]
                : [],
        [url.dir, url.sort],
    )

    const handleSortingChange = React.useCallback(
        (next: SortingState) => {
            const head = next[0]
            pushUrl({
                sort:
                    head && SORT_COLUMN_TO_FIELD[head.id] ? head.id : undefined,
                dir: head ? (head.desc ? "desc" : "asc") : undefined,
                page: 1,
            })
        },
        [pushUrl],
    )

    return (
        <ListWorkspacePage
            title="销售单"
            loading={
                account.isPending ||
                (canList &&
                    !listSettled &&
                    ordersQuery.isPending &&
                    !queryFailed)
            }
            views={
                canList
                    ? {
                          ariaLabel: "销售单工作视图",
                          items: SUMMARY_FILTER_OPTIONS.map((option) => ({
                              id: `sales-orders-list-filter-summary-${toAutomationIdSegment(option.value)}`,
                              label: option.label,
                              count:
                                  url.summary === option.value &&
                                  ordersQuery.data &&
                                  !queryFailed
                                      ? total
                                      : undefined,
                              active: url.summary === option.value,
                              onClick: () => {
                                  // 工作视图与创建人、商业状态、审核状态筛选互斥。
                                  filters.changeSummary(option.value)
                              },
                          })),
                      }
                    : undefined
            }
            actions={[
                {
                    hidden: !canList,
                    actionKey: "export",
                    id: "sales-orders-list-header-export",
                    label: isExporting ? "导出中…" : "导出",
                    icon: DownloadIcon,
                    tier: "quiet",
                    loading: isExporting,
                    disabled: total === 0 || queryFailed || isExporting,
                    title: queryFailed
                        ? "请重新查询后导出"
                        : !isExporting && total === 0
                          ? "当前没有可导出的销售单"
                          : undefined,
                    onClick: exportCsv,
                },
                {
                    hidden: !canCreate,
                    actionKey: "create",
                    id: "sales-orders-list-header-create",
                    label: "新建销售单",
                    icon: PlusIcon,
                    tier: "primary",
                    render: <Link href={CREATE_HREF} />,
                },
            ]}
            alerts={
                account.isError && !account.data ? (
                    <BusinessFailureState
                        id="sales-orders-list-retry"
                        title="账号资料加载失败"
                        error={account.error}
                        onRetry={() => void account.refetch()}
                    />
                ) : !canList ? (
                    <BusinessFailureState
                        kind="permission"
                        title="暂无查看销售单的权限"
                    />
                ) : undefined
            }
            notices={
                canList && exportJob ? (
                    <SalesOrdersListExportResult exportJob={exportJob} />
                ) : undefined
            }
            resultsLabel={
                canList && ordersQuery.data && !queryFailed
                    ? `销售单列表，共 ${total} 条结果`
                    : undefined
            }
            surface={
                canList
                    ? {
                          ariaLabel: "销售单列表",
                          toolbar: (
                              <SalesOrdersListFilterBar
                                  filters={filters}
                                  chips={chips}
                                  loading={ordersQuery.isFetching}
                                  failed={queryFailed}
                              />
                          ),
                          tableClassName: salesStyles.table,
                          table: (
                              <SalesOrdersListTable
                                  noScope={
                                      ordersQuery.data?.emptyReason ===
                                      "no_scope"
                                  }
                                  items={items}
                                  total={total}
                                  loading={listLoading}
                                  isError={queryFailed}
                                  error={ordersQuery.error}
                                  onRetry={() => {
                                      if (identityReady)
                                          void ordersQuery.refetch()
                                      else void account.refetch()
                                  }}
                                  filtersActive={filtersActive}
                                  onClearFilters={clearFilters}
                                  sorting={sorting}
                                  onSortingChange={handleSortingChange}
                                  pagination={pagination}
                                  onPaginationChange={handlePaginationChange}
                                  canCreate={canCreate}
                                  createHref={CREATE_HREF}
                                  canOpen={hasPermission(
                                      account.data?.permissions,
                                      "sales_order:detail",
                                  )}
                                  canDownloadContract={canReadContract}
                                  onRowNavigate={handleRowNavigate}
                                  downloadingContractId={downloadingContractId}
                                  downloadContract={downloadContract}
                              />
                          ),
                      }
                    : undefined
            }
        />
    )
}
