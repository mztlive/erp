"use client"

import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
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
        <section
            className="space-y-4 rounded-xl border bg-card p-5"
            aria-label="供应商父子订单"
        >
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <h3 className="font-semibold">供应商订单</h3>
                    <p className="text-xs text-muted-foreground">
                        主订单号：
                        <span className="num">{tree.root_order_no}</span>
                    </p>
                </div>
                <div className="flex flex-wrap gap-2">
                    {leaves.length > 0 || !reservations.length ? (
                        <Badge variant="outline">
                            {leaves.length} 个拆分后子订单
                        </Badge>
                    ) : null}
                    {reservations.length > 0 ? (
                        <Badge variant="warning">
                            {reservations.length} 个待确认锁库单
                        </Badge>
                    ) : null}
                    {confirmed > 0 ? (
                        <Badge
                            variant={
                                confirmed === leaves.length ? "success" : "info"
                            }
                        >
                            {confirmed === leaves.length
                                ? "全部已确认"
                                : "部分确认"}
                        </Badge>
                    ) : null}
                    {canceled > 0 ? (
                        <Badge variant="warning">
                            {canceled === leaves.length
                                ? "全部已取消"
                                : "部分取消"}
                        </Badge>
                    ) : null}
                </div>
            </div>
            {tree.pending ? (
                <Alert variant="warning">
                    <AlertDescription>
                        供应商拆单结果核验中，请稍后刷新查看；请勿重复下单或确认。
                    </AlertDescription>
                </Alert>
            ) : null}
            {confirmationUnknown ? (
                <p className="text-xs text-warning-soft-foreground">
                    存在确认结果待核实的子订单，请点击「查询原结果」核对，请勿重复确认。
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
                <div className="divide-y">
                    {current.map((node) => (
                        <article
                            key={node.external_order_no}
                            className="space-y-3 py-4 first:pt-0 last:pb-0"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-3">
                                <h4 className="text-body-compact font-medium">
                                    {node.role === "RESERVATION_ROOT"
                                        ? "锁库单"
                                        : "子订单"}{" "}
                                    <span className="num">
                                        {node.external_order_no}
                                    </span>
                                </h4>
                                <div className="flex flex-wrap gap-1.5">
                                    {node.role === "RESERVATION_ROOT" ? (
                                        <Badge variant="warning">
                                            {NODE_ROLE_LABEL[node.role]}
                                        </Badge>
                                    ) : null}
                                    <Badge
                                        variant={
                                            node.confirmation === "CONFIRMED"
                                                ? "success"
                                                : "warning"
                                        }
                                    >
                                        {node.confirmation
                                            ? CONFIRMATION_LABEL[
                                                  node.confirmation
                                              ]
                                            : "确认状态待核实"}
                                    </Badge>
                                    {node.role === "LEAF" &&
                                    node.fulfillment ? (
                                        <Badge variant="outline">
                                            {statusLabel(
                                                FULFILLMENT_STATUS_LABEL,
                                                node.fulfillment,
                                            )}
                                        </Badge>
                                    ) : null}
                                    {node.cancellation &&
                                    node.cancellation.toUpperCase() !==
                                        "NONE" ? (
                                        <Badge variant="warning">
                                            取消：
                                            {statusLabel(
                                                CANCEL_STATUS_LABEL,
                                                node.cancellation,
                                            )}
                                        </Badge>
                                    ) : null}
                                    {node.refund &&
                                    node.refund.toUpperCase() !== "NONE" ? (
                                        <Badge variant="outline">
                                            返款：
                                            {statusLabel(
                                                REFUND_STATUS_LABEL,
                                                node.refund,
                                            )}
                                        </Badge>
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
                                            <span className="ml-2 text-xs text-muted-foreground num">
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
                <details className="border-t pt-3">
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
        </section>
    )
}
