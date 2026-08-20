"use client"

import { BusinessStatusBadge, DocumentSection } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import {
    FACT_TYPE_LABEL,
    FACT_TYPE_TONE,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"

export function AftersalesSection({
    view,
    onOpenSupplier,
}: {
    view: MallConsumptionOrderView
    onOpenSupplier: () => void
}) {
    return (
        <DocumentSection
            title="售后结果分轨"
            description="商城退款仅冲减消费，卡券余额恢复仅记回补，供应商退款另行分列，不替代商城退款。"
        >
            <div className="grid gap-3 md:grid-cols-3">
                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="text-base">商城退款</CardTitle>
                        <CardDescription>冲减消费</CardDescription>
                    </CardHeader>
                    <CardContent className="text-sm">
                        {
                            view.facts.filter(
                                (f) => f.factType === "REFUND_SUCCEEDED",
                            ).length
                        }{" "}
                        笔记录（逐笔展示）
                    </CardContent>
                </Card>
                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="text-base">
                            卡券余额恢复
                        </CardTitle>
                        <CardDescription>只记余额回补</CardDescription>
                    </CardHeader>
                    <CardContent className="text-sm">
                        {
                            view.facts.filter(
                                (f) => f.factType === "CARD_BALANCE_RESTORED",
                            ).length
                        }{" "}
                        笔记录（与退款分轨）
                    </CardContent>
                </Card>
                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="text-base">供应商退款</CardTitle>
                        <CardDescription>成本/应付/现金分列</CardDescription>
                    </CardHeader>
                    <CardContent className="text-sm">
                        {view.supplierOrders.filter(
                            (s) => s.supplierRefundSummary,
                        ).length > 0 ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                onClick={onOpenSupplier}
                            >
                                查看履约区供应商退款摘要
                            </Button>
                        ) : (
                            "无"
                        )}
                    </CardContent>
                </Card>
            </div>
            <Separator className="my-4" />
            <ul className="space-y-2 text-sm">
                {view.facts
                    .filter(
                        (f) =>
                            f.factType === "REFUND_SUCCEEDED" ||
                            f.factType === "CARD_BALANCE_RESTORED" ||
                            f.factType === "ORDER_CANCELED",
                    )
                    .map((f) => (
                        <li
                            key={f.factId}
                            className="rounded-lg bg-muted/40 p-3"
                        >
                            <BusinessStatusBadge
                                context="list"
                                label={FACT_TYPE_LABEL[f.factType]}
                                tone={FACT_TYPE_TONE[f.factType]}
                            />
                            <span className="num ml-2 text-xs text-muted-foreground">
                                {formatDateTime(f.occurredAt, "default")}
                            </span>
                            <div className="mt-1 text-muted-foreground">
                                {Object.entries(f.resultDetails)
                                    .map(
                                        ([k, v]) =>
                                            `${k}=${String(v ?? "—")}`,
                                    )
                                    .join(" · ")}
                            </div>
                        </li>
                    ))}
            </ul>
        </DocumentSection>
    )
}
