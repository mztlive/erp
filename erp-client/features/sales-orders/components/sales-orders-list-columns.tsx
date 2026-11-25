"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    StatusTrackSummary,
    TableAsyncCell,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    isPendingReviewStage,
    NATURE_LABEL,
    stageDueDisplay,
    stageOwnerDisplay,
} from "@/features/sales-orders/lib/labels"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export type SalesOrdersListColumnsContext = {
    canOpen: boolean
    canDownloadContract: boolean
    downloadingContractId: string | null
    downloadContract: (order: SalesOrderListItem) => void
}

export function buildSalesOrdersListColumns(
    context: SalesOrdersListColumnsContext,
): ColumnDef<SalesOrderListItem>[] {
    const {
        canOpen,
        canDownloadContract,
        downloadingContractId,
        downloadContract,
    } = context

    return [
        {
            id: "document",
            accessorKey: "documentNumber",
            header: "销售单",
            meta: { label: "销售单", width: "flex", numeric: true },
            enableHiding: false,
            cell: ({ row }) => {
                const order = row.original
                const summary = `${order.customerName} · ${NATURE_LABEL[order.nature]}`
                return (
                    <TableIdentityCell
                        primary={
                            canOpen ? (
                                <Link
                                    id={`sales-orders-list-row-${toAutomationIdSegment(order.id)}-open`}
                                    href={`/sales/orders/${order.id}`}
                                    aria-label={`查看销售单 ${order.documentNumber}`}
                                    className="num rounded-sm hover:text-primary hover:underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary"
                                    onClick={(event) => event.stopPropagation()}
                                >
                                    {order.documentNumber}
                                </Link>
                            ) : (
                                <span className="num">
                                    {order.documentNumber}
                                </span>
                            )
                        }
                        primaryTitle={order.documentNumber}
                        secondary={summary}
                        secondaryTitle={summary}
                    />
                )
            },
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "销售单状态", width: "default" },
            cell: ({ row }) => {
                const { tone, label } = row.original.primaryStatus
                return (
                    <BusinessStatusBadge
                        label={label}
                        tone={tone}
                        context="list"
                        appearance={tone === "success" ? "quiet" : "badge"}
                    />
                )
            },
        },
        {
            id: "nature",
            header: "业务性质",
            meta: { label: "业务性质", width: "status" },
            cell: ({ row }) => (
                <TableTextCell value={NATURE_LABEL[row.original.nature]} />
            ),
        },
        ...(canDownloadContract
            ? [
                  {
                      id: "contract",
                      header: "合同",
                      meta: { label: "合同", width: "reference" },
                      cell: ({ row }) => {
                          const order = row.original
                          const contractNo = order.contractNumber.trim()
                          const companyName = order.contractCompanyName.trim()
                          return (
                              <TableAsyncCell
                                  failed={Boolean(order.contractLoadError)}
                                  failedLabel={
                                      <span title={order.contractLoadError}>
                                          合同资料读取失败
                                      </span>
                                  }
                              >
                                  {contractNo ? (
                                      <TableIdentityCell
                                          primary={
                                              <span className="num">
                                                  {contractNo}
                                              </span>
                                          }
                                          primaryTitle={contractNo}
                                          secondary={companyName}
                                          secondaryTitle={companyName}
                                      />
                                  ) : order.contractId ? (
                                      <TableEmptyCell
                                          variant="label"
                                          label="合同编号未填写"
                                      />
                                  ) : (
                                      <TableEmptyCell />
                                  )}
                              </TableAsyncCell>
                          )
                      },
                  } satisfies ColumnDef<SalesOrderListItem>,
              ]
            : []),
        {
            id: "tracks",
            header: "进度",
            meta: { label: "多轨进度", width: "tracks" },
            enableSorting: false,
            cell: ({ row }) => (
                <StatusTrackSummary
                    variant="inline"
                    className="gap-x-3 gap-y-0 [&>div]:gap-1"
                    tracks={[
                        {
                            id: "fulfillment",
                            label: "履约",
                            status: row.original.fulfillment,
                        },
                        {
                            id: "collection",
                            label: "回款",
                            status: row.original.collection,
                        },
                        {
                            id: "invoicing",
                            label: "开票",
                            status: row.original.invoicing,
                        },
                    ]}
                />
            ),
        },
        {
            id: "amount",
            accessorKey: "amountGross",
            enableSorting: false,
            header: "成交金额",
            meta: {
                label: "成交金额",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => (
                <MoneyValue
                    display="compact"
                    value={row.original.amountGross}
                    taxBasis="gross"
                />
            ),
        },
        {
            id: "owner",
            accessorKey: "ownerName",
            enableSorting: false,
            header: "负责人",
            meta: { label: "负责人", width: "default" },
            cell: ({ row }) => (
                <TableTextCell value={row.original.ownerName} clamp={1} />
            ),
        },
        {
            id: "currentOwner",
            header: "当前责任 / 时限",
            meta: { label: "当前责任 / 时限", width: "default" },
            enableSorting: false,
            cell: ({ row }) => {
                const order = row.original
                if (!isPendingReviewStage(order.primaryStatus.code)) {
                    return <TableEmptyCell />
                }
                const due = stageDueDisplay(order)
                return (
                    <TableIdentityCell
                        primary={stageOwnerDisplay(order)}
                        secondary={
                            due ? (
                                <time dateTime={due.dateTime}>{due.label}</time>
                            ) : (
                                "时限未设置"
                            )
                        }
                        secondaryTitle={due?.label}
                        numericSecondary={Boolean(due)}
                    />
                )
            },
        },
        {
            id: "submittedAt",
            accessorKey: "submittedAt",
            header: "创建时间",
            meta: { label: "创建时间", width: "default", numeric: true },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.submittedAt}
                    numeric
                    clamp={1}
                />
            ),
        },
        ...(canDownloadContract
            ? [
                  {
                      id: "actions",
                      header: "操作",
                      meta: { label: "操作", width: "status", align: "end" },
                      cell: ({ row }) => {
                          const order = row.original
                          if (!order.contractId) return <TableEmptyCell />
                          const rowPrefix = `sales-orders-list-row-${toAutomationIdSegment(order.id)}`
                          const downloading =
                              downloadingContractId === order.contractId
                          return (
                              <TableRowActions
                                  moreId={`${rowPrefix}-more`}
                                  moreLabel="销售单更多操作"
                                  maxInline={1}
                                  actions={[
                                      {
                                          id: `${rowPrefix}-download-contract`,
                                          label: downloading ? (
                                              "下载中"
                                          ) : (
                                              <>
                                                  下载合同
                                                  {order.contractNumber ? (
                                                      <span className="sr-only">
                                                          {" "}
                                                          {order.contractNumber}
                                                      </span>
                                                  ) : null}
                                              </>
                                          ),
                                          loading: downloading,
                                          disabled: Boolean(
                                              downloadingContractId,
                                          ),
                                          disabledReason:
                                              downloadingContractId &&
                                              !downloading
                                                  ? "请等待当前合同下载完成"
                                                  : undefined,
                                          onClick: () =>
                                              downloadContract(order),
                                      },
                                  ]}
                              />
                          )
                      },
                  } satisfies ColumnDef<SalesOrderListItem>,
              ]
            : []),
    ]
}
