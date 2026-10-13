"use client"

import Link from "next/link"

import { Badge } from "@/components/ui/badge"
import type { SalesChangeProcurementProgress } from "@/features/sales-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { purchaseSourceUpdateText } from "@/lib/ui-text"

/**
 * 销售变更生效后受影响采购的处理进度（第一期 §6.3 第 11 条）。
 * 全部处理完成由系统按事实判定，不设销售确认。
 */
export function SalesChangeProcurementProgressCard({
    id,
    procurement,
}: {
    id: string
    procurement: SalesChangeProcurementProgress
}) {
    if (procurement.total === 0) return null

    return (
        <section
            id={id}
            aria-labelledby={`${id}-title`}
            className="space-y-3 rounded-xl border border-border/70 px-4 py-4 md:px-5"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 id={`${id}-title`} className="text-sm font-semibold">
                    {procurement.predicted ? "预计影响的采购" : "采购处理"}
                </h3>
                <Badge
                    variant={
                        procurement.completed
                            ? "success"
                            : procurement.predicted
                              ? "secondary"
                              : "warning"
                    }
                >
                    {procurement.completed
                        ? purchaseSourceUpdateText.completed
                        : procurement.predicted
                          ? `预计 ${procurement.total} 张`
                          : `待处理 ${procurement.open} / ${procurement.total} 张`}
                </Badge>
            </div>
            {procurement.predicted ? (
                <p className="text-xs text-muted-foreground">
                    按本次提交内容预判，生效时按当时事实重新判定；生效后审批中的受影响采购会自动退回草稿，并派给采购负责人处理。
                </p>
            ) : null}
            {procurement.items.length ? (
                <ul className="divide-y divide-border/60">
                    {procurement.items.map((item) => (
                        <li
                            key={item.purchaseOrderId}
                            className="space-y-1 py-2 first:pt-0 last:pb-0 text-sm"
                        >
                            <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                <Link
                                    id={`${id}-${toAutomationIdSegment(item.purchaseOrderId)}-link`}
                                    href={`/procurement/orders/${encodeURIComponent(item.purchaseOrderId)}`}
                                    className="num font-medium underline underline-offset-2"
                                >
                                    {item.purchaseNo ?? "采购单"}
                                </Link>
                                <span className="text-muted-foreground">
                                    {itemStatus(procurement.predicted, item)}
                                </span>
                                {item.maintainNote ? (
                                    <span className="text-muted-foreground">
                                        （{item.maintainNote}）
                                    </span>
                                ) : null}
                            </div>
                            {item.supplierName || item.ownerName ? (
                                <p className="text-xs text-muted-foreground">
                                    {[
                                        item.supplierName,
                                        item.ownerName
                                            ? `负责人 ${item.ownerName}`
                                            : undefined,
                                    ]
                                        .filter(Boolean)
                                        .join(" · ")}
                                </p>
                            ) : null}
                            <ul className="space-y-0.5 text-xs text-muted-foreground">
                                {item.reasons.map((reason) => (
                                    <li
                                        key={`${reason.lineNo}-${reason.description}`}
                                    >
                                        第 {reason.lineNo} 行 {reason.itemName}
                                        ：{reason.description}
                                    </li>
                                ))}
                            </ul>
                        </li>
                    ))}
                </ul>
            ) : (
                <p className="text-xs text-muted-foreground">
                    {purchaseSourceUpdateText.noPurchasePermission}
                </p>
            )}
        </section>
    )
}

/** 单张采购的处理状态文案。 */
function itemStatus(
    predicted: boolean,
    item: SalesChangeProcurementProgress["items"][number],
) {
    if (predicted) return "预计受影响"
    if (item.awaitingSubmission)
        return purchaseSourceUpdateText.awaitingSubmission
    if (item.open) return "待采购处理"
    return item.resolutionLabel ?? "已处理"
}
