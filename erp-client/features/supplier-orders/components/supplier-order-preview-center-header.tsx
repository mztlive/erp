"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import {
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
    GuardedBusinessAction,
} from "@/components/business"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { Button } from "@/components/ui/button"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { formatDateTime } from "@/lib/datetime"

export function SupplierOrderCenterHeader({
    order,
    canQuery,
    canReplay,
    isResultUnknown,
    actionBlockers,
    allowedActions,
    queryPending,
    replayPending,
    onQueryResult,
    onReplayClick,
    embedded = false,
    tabs,
}: {
    order: SupplierOrderDetailView["order"]
    canQuery: boolean
    canReplay: boolean
    isResultUnknown: boolean
    actionBlockers: SupplierOrderDetailView["actionBlockers"]
    allowedActions: string[]
    queryPending: boolean
    replayPending: boolean
    onQueryResult: () => void
    onReplayClick: () => void
    embedded?: boolean
    tabs: ReactNode
}) {
    return (
        <DetailWorkspaceChrome
            tabs={tabs}
            back={
                embedded
                    ? undefined
                    : {
                          id: "supplier-order-center-header-back-list",
                          label: "返回供应商订单列表",
                          href: "/supplier-api/orders",
                      }
            }
            title={order.supplierName}
            documentNumber={order.orderNo}
            primaryStatus={{
                label: order.fulfillmentLabel,
                tone: order.fulfillmentTone,
            }}
            meta={
                <DetailWorkspaceMeta
                    items={[
                        <span key="external" className="num">
                            供应商单号 {order.externalOrderNo ?? "尚未返回"}
                        </span>,
                        order.connectionCode ? (
                            <span key="conn" className="num">
                                连接 {order.connectionCode}
                            </span>
                        ) : null,
                        order.createdAt ? (
                            <span key="created" className="num">
                                创建于{" "}
                                {formatDateTime(
                                    order.createdAt,
                                    "fullIntl",
                                    "passthrough",
                                )}
                            </span>
                        ) : null,
                    ]}
                />
            }
            statuses={[
                {
                    id: "fulfillment",
                    label: "履约",
                    status: {
                        label: order.fulfillmentLabel,
                        tone: order.fulfillmentTone,
                    },
                },
                {
                    id: "cancel",
                    label: "取消",
                    status: {
                        label: order.cancelLabel,
                        tone: order.cancelTone,
                    },
                },
                {
                    id: "refund",
                    label: "退款",
                    status: {
                        label: order.refundLabel,
                        tone: order.refundTone,
                    },
                },
            ]}
            primaryAction={
                isResultUnknown ? (
                    <PermissionGuard permission="supplier_fulfillment_order:investigate">
                        <GuardedBusinessAction
                            id="supplier-order-center-header-query-result"
                            loading={queryPending}
                            type="button"
                            size="sm"
                            disabled={!canQuery || queryPending}
                            reason={
                                !canQuery
                                    ? (actionBlockers.find(
                                          (b) => b.action === "QUERY_RESULT",
                                      )?.message ?? "当前不可查询")
                                    : undefined
                            }
                            onClick={onQueryResult}
                        >
                            {queryPending ? "查询中…" : "查询原下单结果"}
                        </GuardedBusinessAction>
                    </PermissionGuard>
                ) : undefined
            }
            secondaryActions={
                <>
                    {isResultUnknown ? (
                        <PermissionGuard permission="supplier_fulfillment_order:investigate">
                            <GuardedBusinessAction
                                id="supplier-order-center-header-replay"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={!canReplay || replayPending}
                                reason={
                                    canReplay
                                        ? "已确认无结果，可安全重发"
                                        : (actionBlockers.find(
                                              (b) => b.action === "REPLAY",
                                          )?.message ??
                                          "需先查询确认无结果后，方可重发")
                                }
                                onClick={onReplayClick}
                            >
                                安全重发
                            </GuardedBusinessAction>
                        </PermissionGuard>
                    ) : null}
                    {allowedActions.includes("ESCALATE_W29") ? (
                        <PermissionGuard permission="integration_error_task:list">
                            <Button
                                id="supplier-order-center-header-escalate"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.quietButton}
                                render={
                                    <Link href="/governance/integration-errors" />
                                }
                            >
                                转接口错误中心
                                <ExternalLinkIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        </PermissionGuard>
                    ) : null}
                </>
            }
        />
    )
}
