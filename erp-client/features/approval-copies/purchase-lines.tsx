"use client"

import { useMemo } from "react"

import {
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    ApprovalPurchaseFulfillment,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import {
    countUniqueDeliveryPoints,
    formatPurchaseFulfillmentSummary,
} from "@/features/approval-workflow/purchase-fulfillment"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { multiplyFixed, sumFixed } from "@/lib/fixed-decimal"

/** 仅展示抄送接口已经授权的采购提交行及履约规模摘要（不展示详细配送地址）。 */
export function CopyPurchaseLines({
    lines,
    fulfillment,
}: {
    lines: ApprovalPurchaseLine[]
    fulfillment?: ApprovalPurchaseFulfillment | null
    idPrefix?: string
}) {
    const { names } = useProductUnitNames([], lines.length > 0)

    const uniquePointCount = useMemo(
        () =>
            countUniqueDeliveryPoints(
                lines.map((line) => line.delivery_allocations),
            ),
        [lines],
    )
    const fulfillmentSummary = useMemo(
        () => formatPurchaseFulfillmentSummary(fulfillment, uniquePointCount),
        [fulfillment, uniquePointCount],
    )

    const grossTotal = useMemo(() => {
        const amounts = lines
            .map((line) => line.gross_amount)
            .filter((value): value is string => value != null)
        return amounts.length
            ? sumFixed(amounts, { maxScale: 2, outputScale: 2 })
            : null
    }, [lines])

    if (!lines.length) return null

    const subtitle = fulfillmentSummary
        ? `共 ${lines.length} 行 · ${fulfillmentSummary}`
        : `共 ${lines.length} 行`

    return (
        <DetailWorkspaceSection title="本次采购提交明细" subtitle={subtitle}>
            <div
                className={detailWorkspaceStyles.inlineTableWrapper}
                data-slot="detail-inline-table"
            >
                <Table
                    data-density="comfortable"
                    className="w-full [&_:is(th,td):first-child]:ps-0"
                >
                    <TableHeader>
                        <TableRow>
                            <TableHead className="w-8">#</TableHead>
                            <TableHead className="min-w-44">
                                商品或费用 / 规格
                            </TableHead>
                            <TableHead data-align="end" className="w-24">
                                数量
                            </TableHead>
                            <TableHead data-align="end" className="w-28">
                                含税单价
                            </TableHead>
                            <TableHead data-align="end" className="w-28">
                                含税金额
                            </TableHead>
                            <TableHead data-align="end" className="w-28">
                                预计交付
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line) => {
                            const unit =
                                line.unitName ??
                                names.get(line.unit?.trim() ?? "")
                            const spec =
                                line.specification &&
                                line.specification !== "无规格"
                                    ? line.specification
                                    : null
                            const taxRate =
                                line.input_tax_rate != null
                                    ? multiplyFixed(
                                          line.input_tax_rate,
                                          "100",
                                          {
                                              leftMaxScale: 6,
                                              rightMaxScale: 0,
                                              outputScale: 2,
                                          },
                                      )
                                    : null
                            return (
                                <TableRow key={line.id}>
                                    <TableCell className="num py-3 align-top text-muted-foreground">
                                        {line.line_no}
                                    </TableCell>
                                    <TableCell className="py-3 align-top whitespace-normal wrap-anywhere">
                                        <p className="font-medium text-foreground">
                                            {line.title}
                                        </p>
                                        {spec ? (
                                            <p className="mt-0.5 text-xs text-muted-foreground">
                                                {spec}
                                            </p>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-3 align-top"
                                    >
                                        {line.quantity == null ? (
                                            "—"
                                        ) : (
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={unit}
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-3 align-top"
                                    >
                                        <MoneyValue
                                            display="exact"
                                            value={line.unit_cost_gross}
                                        />
                                        {taxRate != null ? (
                                            <p className="mt-0.5 text-xs text-muted-foreground">
                                                进项{" "}
                                                <RateValue
                                                    value={taxRate}
                                                    precision={2}
                                                />
                                            </p>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-3 align-top font-medium"
                                    >
                                        <MoneyValue
                                            display="exact"
                                            value={line.gross_amount}
                                        />
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="num py-3 align-top text-muted-foreground"
                                    >
                                        {line.expected_delivery_date || "—"}
                                    </TableCell>
                                </TableRow>
                            )
                        })}
                    </TableBody>
                    {grossTotal ? (
                        <TableFooter>
                            <TableRow className="hover:bg-transparent">
                                <TableCell
                                    colSpan={4}
                                    className="py-2.5 text-xs font-medium text-muted-foreground"
                                >
                                    合计 · 共{" "}
                                    <span className="num text-foreground">
                                        {lines.length}
                                    </span>{" "}
                                    项
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="py-2.5 font-semibold text-foreground"
                                >
                                    <MoneyValue
                                        display="exact"
                                        value={grossTotal}
                                    />
                                </TableCell>
                                <TableCell className="py-2.5" />
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            </div>
        </DetailWorkspaceSection>
    )
}
