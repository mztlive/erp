"use client"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { salesChangeUnitName } from "../lib/sales-change-draft"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { useSalesChangeDraftQuery } from "../hooks/use-sales-change-draft"
import { multiplyFixed, sumFixed } from "@/lib/fixed-decimal"

/** 只展示已保存的改单目标；编辑弹窗中的未保存输入不进入详情摘要。 */
export function SalesChangeDraftSummary({
    changeOrderId,
}: {
    changeOrderId: string
}) {
    const query = useSalesChangeDraftQuery(changeOrderId, true)
    const { names } = useProductUnitNames(
        query.data?.lines.map((line) => line.goods?.sku_id),
        Boolean(query.data?.lines.length),
    )
    if (query.isPending)
        return (
            <p role="status" className="text-xs text-muted-foreground">
                正在读取改单草稿…
            </p>
        )
    if (query.isError)
        return (
            <div
                role="alert"
                className="flex flex-wrap items-center gap-2 text-body-compact"
            >
                <span>改单摘要读取失败，请重试。</span>
                <Button
                    id="sales-change-summary-retry"
                    variant="outline"
                    size="sm"
                    onClick={() => void query.refetch()}
                >
                    重新读取
                </Button>
            </div>
        )
    const draft = query.data
    // 首次发起的变更及工作副本均为版本 1。撤回后另建的副本也可能为 1，
    // 必须同时核对变更版本，避免把已经保存的恢复内容误报为未修改。
    if (draft.version === 1 && draft.working_copy_version === 1)
        return (
            <p className="text-xs text-muted-foreground">
                尚未保存修改，草稿沿用原销售单内容。
            </p>
        )
    const lines = draft.lines.map((line) => {
        const quantity =
            line.goods?.quantity ?? String(line.voucher?.card_count ?? "0")
        const amount = multiplyFixed(
            quantity,
            line.goods?.unit_price_gross ??
                line.voucher?.unit_price_gross ??
                "0",
            {
                leftMaxScale: 6,
                rightMaxScale: 4,
                outputScale: 2,
            },
        )
        return { line, quantity, amount }
    })
    const total = sumFixed(
        lines.map((line) => line.amount),
        { maxScale: 2, outputScale: 2 },
    )
    return (
        <div className="space-y-3 border-t border-warning-border/60 pt-3 text-body-compact">
            <div className="flex flex-wrap items-baseline gap-x-5 gap-y-2">
                <span className="font-medium">已保存的改单内容</span>
                <span>
                    原单 <MoneyValue value={draft.base_gross_amount} />
                </span>
                <span>
                    改单后 <MoneyValue value={total} />
                </span>
                <span className="text-muted-foreground">审批通过后生效</span>
            </div>
            {draft.reason && draft.reason !== "销售发起变更" ? (
                <p className="break-words">变更原因：{draft.reason}</p>
            ) : null}
            <ul className="space-y-2">
                {lines.slice(0, 3).map(({ line, quantity, amount }) => (
                    <li
                        key={line.sales_order_line_id}
                        className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1"
                    >
                        <span className="min-w-0 break-words">
                            {line.item_name_snapshot}
                        </span>
                        <span className="flex items-baseline gap-4">
                            <QuantityValue
                                value={quantity}
                                unit={salesChangeUnitName(line, names)}
                            />
                            <MoneyValue value={amount} />
                        </span>
                    </li>
                ))}
            </ul>
            {lines.length > 3 ? (
                <p className="text-xs text-muted-foreground">
                    共 {lines.length} 项，继续编辑可查看全部商品和交付安排。
                </p>
            ) : null}
        </div>
    )
}
