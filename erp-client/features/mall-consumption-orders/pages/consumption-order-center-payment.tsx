"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { DocumentSection, MoneyValue } from "@/components/business"
import type {
    MallConsumptionOrderView,
    PaymentSourceView,
} from "@/features/mall-consumption-orders/types"
import { cn } from "@/lib/utils"

function sourceColumnTitle(source: PaymentSourceView) {
    if (source.sourceType === "CARD") {
        return (
            <span>
                卡券 {source.sourceReference}
                <Badge variant="outline" className="ml-1">
                    非卡号
                </Badge>
            </span>
        )
    }
    return <span>微信 {source.sourceReference}</span>
}

function allocationAmount(
    view: MallConsumptionOrderView,
    itemId: string,
    sourceId: string,
): string {
    const hit = view.fundingAllocations.find(
        (a) => a.mallOrderItemId === itemId && a.paymentSourceId === sourceId,
    )
    return hit?.allocatedPaymentAmount ?? "0.00"
}

function PaymentMatrix({ view }: { view: MallConsumptionOrderView }) {
    const sources = view.paymentSources
    const items = view.items
    const anyInvalid =
        !view.conservation.orderTotal.valid ||
        view.conservation.itemRowResults.some((r) => !r.valid) ||
        view.conservation.sourceColumnResults.some((r) => !r.valid)

    return (
        <div className="space-y-3 overflow-x-auto">
            {anyInvalid ? (
                <Alert variant="destructive" role="alert">
                    <AlertTitle>分摊不守恒</AlertTitle>
                    <AlertDescription>
                        系统校验与页面存在差异时高亮无效单元格；页面不推算优惠、运费或分摊。
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert variant="success">
                    <AlertTitle>行列守恒有效</AlertTitle>
                    <AlertDescription>
                        行合计、列合计与订单实付均由系统给出：
                        <span className="num mx-1">
                            {view.conservation.orderTotal.actual}
                        </span>
                        （含税实付）。
                    </AlertDescription>
                </Alert>
            )}

            <table className="w-full min-w-[40rem] border-collapse text-sm">
                <caption className="sr-only">
                    商品 × 支付来源分摊矩阵（仅卡券 / 微信）
                </caption>
                <thead>
                    <tr className="border-b border-border/30 text-left">
                        <th className="sticky left-0 bg-card p-2 font-medium">
                            商品明细
                        </th>
                        {sources.map((s) => (
                            <th
                                key={s.paymentSourceId}
                                className="p-2 font-medium"
                            >
                                {sourceColumnTitle(s)}
                            </th>
                        ))}
                        <th className="p-2 text-right font-medium">明细实付</th>
                    </tr>
                </thead>
                <tbody>
                    {items.map((item) => {
                        const rowResult = view.conservation.itemRowResults.find(
                            (r) => r.mallOrderItemId === item.mallOrderItemId,
                        )
                        return (
                            <tr
                                key={item.mallOrderItemId}
                                className="border-b border-border/70"
                            >
                                <th
                                    scope="row"
                                    className="sticky left-0 bg-card p-2 text-left font-normal"
                                >
                                    <div className="font-medium">
                                        {item.nameSnapshot}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        {item.specSnapshot}
                                        <span className="mx-1">·</span>
                                        <span className="num">
                                            {item.externalItemId}
                                        </span>
                                    </div>
                                </th>
                                {sources.map((s) => {
                                    const amount = allocationAmount(
                                        view,
                                        item.mallOrderItemId,
                                        s.paymentSourceId,
                                    )
                                    return (
                                        <td
                                            key={s.paymentSourceId}
                                            className="p-2"
                                        >
                                            <MoneyValue value={amount} />
                                        </td>
                                    )
                                })}
                                <td
                                    className={cn(
                                        "p-2 text-right",
                                        rowResult &&
                                            !rowResult.valid &&
                                            "bg-destructive/10",
                                    )}
                                >
                                    <MoneyValue
                                        value={item.paidAmount}
                                        taxBasis="gross"
                                    />
                                    {rowResult && !rowResult.valid ? (
                                        <div className="text-xs text-destructive">
                                            期望 {rowResult.expected} / 实际{" "}
                                            {rowResult.actual}
                                        </div>
                                    ) : null}
                                </td>
                            </tr>
                        )
                    })}
                </tbody>
                <tfoot>
                    <tr className="border-t border-border/30 font-medium">
                        <th
                            scope="row"
                            className="sticky left-0 bg-card p-2 text-left"
                        >
                            来源合计
                        </th>
                        {sources.map((s) => {
                            const col =
                                view.conservation.sourceColumnResults.find(
                                    (r) =>
                                        r.paymentSourceId === s.paymentSourceId,
                                )
                            return (
                                <td
                                    key={s.paymentSourceId}
                                    className={cn(
                                        "p-2",
                                        col &&
                                            !col.valid &&
                                            "bg-destructive/10",
                                    )}
                                >
                                    <MoneyValue value={s.amount} />
                                    {col && !col.valid ? (
                                        <div className="text-xs text-destructive">
                                            期望 {col.expected}
                                        </div>
                                    ) : null}
                                </td>
                            )
                        })}
                        <td className="p-2 text-right">
                            <MoneyValue
                                value={view.conservation.orderTotal.actual}
                                taxBasis="gross"
                            />
                        </td>
                    </tr>
                </tfoot>
            </table>
            <p className="text-xs text-muted-foreground">
                支付来源仅卡券与微信；不存在福利账户分支。成本不进入本矩阵。
            </p>
        </div>
    )
}

export function PaymentSection({
    view,
}: {
    view: MallConsumptionOrderView
}) {
    return (
        <DocumentSection
            title="支付与分摊"
            description="商品与支付来源的守恒校验；合计与状态以系统结果为准。"
        >
            <div className="mb-4 flex flex-wrap gap-2">
                {view.paymentSources.map((s) => (
                    <Badge key={s.paymentSourceId} variant="secondary">
                        {s.sourceType === "CARD"
                            ? "卡券"
                            : "微信"}{" "}
                        {s.sourceReference}
                        {s.sourceType === "CARD"
                            ? " · 非卡号"
                            : ""}{" "}
                        · ¥{s.amount}
                    </Badge>
                ))}
            </div>
            <PaymentMatrix view={view} />
        </DocumentSection>
    )
}
