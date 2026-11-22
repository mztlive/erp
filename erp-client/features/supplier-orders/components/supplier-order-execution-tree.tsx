"use client"

import {
    BusinessStatusBadge,
    DetailWorkspaceSection,
    QuantityValue,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    CANCEL_STATUS_LABEL,
    FULFILLMENT_STATUS_LABEL,
    REFUND_STATUS_LABEL,
} from "../types"
import type {
    SupplierExecutionOrderNodeRole,
    SupplierExecutionOrderTree,
} from "../api/execution-setup"

const CONFIRMATION_LABEL = {
    PENDING: "等待确认",
    CONFIRMED: "已确认",
    REJECTED: "确认被拒绝",
} as const

const NODE_ROLE_LABEL: Record<SupplierExecutionOrderNodeRole, string> = {
    PARENT: "历史父单",
    RESERVATION_ROOT: "待确认锁库单",
    LEAF: "拆分后子订单",
    UNRESOLVED: "关系待核实",
}

function statusLabel(table: Record<string, string>, value: string | null) {
    return value ? (table[value.toUpperCase()] ?? "状态待核验") : "尚未取得"
}

/** 当前子单与历史父单分开展示，不在客户端累加父子金额或推算发货数量。 */
export function SupplierOrderExecutionTree({
    tree,
    orderId,
    confirmationUnknown,
}: {
    tree: SupplierExecutionOrderTree
    orderId: string
    confirmationUnknown: boolean
}) {
    const leaves = tree.nodes.filter((node) => node.role === "LEAF")
    const reservations = tree.nodes.filter(
        (node) => node.role === "RESERVATION_ROOT",
    )
    const current = tree.nodes.filter(
        (node) => node.role === "LEAF" || node.role === "RESERVATION_ROOT",
    )
    const confirmed = leaves.filter(
        (node) => node.confirmation === "CONFIRMED",
    ).length
    const canceled = leaves.filter(
        (node) => node.cancellation?.toUpperCase() === "CANCELED",
    ).length
    const history = tree.nodes.filter(
        (node) => node.role === "PARENT" || node.role === "UNRESOLVED",
    )
    const prefix = `supplier-order-tree-${toAutomationIdSegment(orderId)}`

    return (
        <DetailWorkspaceSection
            title="供应商父子订单"
            width="full"
            subtitle={
                <span>
                    主订单号：<span className="num">{tree.root_order_no}</span>
                </span>
            }
            actions={
                <div className="flex flex-wrap gap-1.5">
                    {leaves.length > 0 || !reservations.length ? (
                        <BusinessStatusBadge
                            context="list"
                            appearance="quiet"
                            tone="neutral"
                            label={`${leaves.length} 个拆分后子订单`}
                        />
                    ) : null}
                    {reservations.length > 0 ? (
                        <BusinessStatusBadge
                            context="list"
                            tone="warning"
                            label={`${reservations.length} 个待确认锁库单`}
                        />
                    ) : null}
                    {confirmed > 0 ? (
                        <BusinessStatusBadge
                            context="list"
                            tone={
                                confirmed === leaves.length ? "success" : "info"
                            }
                            label={
                                confirmed === leaves.length
                                    ? "全部已确认"
                                    : "部分确认"
                            }
                        />
                    ) : null}
                    {canceled > 0 ? (
                        <BusinessStatusBadge
                            context="list"
                            tone="warning"
                            label={
                                canceled === leaves.length
                                    ? "全部已取消"
                                    : "部分取消"
                            }
                        />
                    ) : null}
                </div>
            }
        >
            {tree.pending ? (
                <Alert variant="warning">
                    <AlertDescription>
                        供应商拆单结果核验中，请稍后刷新查看；请勿重复下单或确认。
                    </AlertDescription>
                </Alert>
            ) : null}
            {confirmationUnknown ? (
                <p className="text-xs text-warning-soft-foreground">
                    存在确认结果待核实的子订单，请先核验执行结果，请勿重复确认。
                </p>
            ) : null}
            {reservations.length > 0 ? (
                <p className="text-xs text-muted-foreground">
                    待确认锁库单需确认或取消后才会生效；锁库数量不计入实际已发货数量。
                </p>
            ) : null}
            {current.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    暂未获取到供应商拆分后的子订单明细。
                </p>
            ) : (
                <div className="divide-y divide-border/60 border-y border-border/60">
                    {current.map((node) => (
                        <article
                            key={node.external_order_no}
                            className="space-y-3 py-4"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <h3 className="text-body-compact font-medium">
                                    {node.role === "RESERVATION_ROOT"
                                        ? "锁库单"
                                        : "子订单"}{" "}
                                    <span className="num">
                                        {node.external_order_no}
                                    </span>
                                </h3>
                                <div className="flex flex-wrap gap-1.5">
                                    {node.role === "RESERVATION_ROOT" ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            tone="warning"
                                            label={NODE_ROLE_LABEL[node.role]}
                                        />
                                    ) : null}
                                    <BusinessStatusBadge
                                        context="list"
                                        tone={
                                            node.confirmation === "CONFIRMED"
                                                ? "success"
                                                : node.confirmation ===
                                                    "REJECTED"
                                                  ? "destructive"
                                                  : "warning"
                                        }
                                        label={
                                            node.confirmation
                                                ? CONFIRMATION_LABEL[
                                                      node.confirmation
                                                  ]
                                                : "确认状态待核实"
                                        }
                                    />
                                    {node.role === "LEAF" &&
                                    node.fulfillment ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            appearance="quiet"
                                            tone="info"
                                            label={statusLabel(
                                                FULFILLMENT_STATUS_LABEL,
                                                node.fulfillment,
                                            )}
                                        />
                                    ) : null}
                                    {node.cancellation &&
                                    node.cancellation.toUpperCase() !==
                                        "NONE" ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            tone="warning"
                                            label={`取消：${statusLabel(CANCEL_STATUS_LABEL, node.cancellation)}`}
                                        />
                                    ) : null}
                                    {node.refund &&
                                    node.refund.toUpperCase() !== "NONE" ? (
                                        <BusinessStatusBadge
                                            context="list"
                                            appearance="quiet"
                                            tone="neutral"
                                            label={`返款：${statusLabel(REFUND_STATUS_LABEL, node.refund)}`}
                                        />
                                    ) : null}
                                </div>
                            </div>
                            {node.parent_order_no ? (
                                <p className="text-xs text-muted-foreground">
                                    来源父单{" "}
                                    <span className="num">
                                        {node.parent_order_no}
                                    </span>
                                </p>
                            ) : null}
                            <ul className="space-y-2 text-body-compact">
                                {node.lines.map((line, index) => (
                                    <li
                                        key={`${line.sku_code}-${index}`}
                                        className="flex flex-wrap justify-between gap-x-5 gap-y-1"
                                    >
                                        <span>
                                            {line.product_name ||
                                                "商品名称待核验"}
                                            <span className="num ml-2 text-xs text-muted-foreground">
                                                供应商 SKU：{line.sku_code}
                                            </span>
                                        </span>
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit ?? ""}
                                        />
                                    </li>
                                ))}
                            </ul>
                        </article>
                    ))}
                </div>
            )}
            {history.length > 0 ? (
                <details className="pt-2">
                    <summary
                        id={`${prefix}-history`}
                        className="cursor-pointer text-xs text-muted-foreground"
                    >
                        根单与历次拆分记录（{history.length}）
                    </summary>
                    <ul className="mt-3 space-y-2 text-xs text-muted-foreground">
                        {history.map((node) => (
                            <li key={node.external_order_no}>
                                <span className="num">
                                    {node.external_order_no}
                                </span>{" "}
                                · {NODE_ROLE_LABEL[node.role]}
                                {node.parent_order_no ? (
                                    <>
                                        {" "}
                                        · 来源{" "}
                                        <span className="num">
                                            {node.parent_order_no}
                                        </span>
                                    </>
                                ) : null}
                            </li>
                        ))}
                    </ul>
                </details>
            ) : null}
            <p className="text-xs text-muted-foreground">
                历史主单仅作关联追溯；实际发货数量以系统确认的交付记录为准。
            </p>
        </DetailWorkspaceSection>
    )
}
