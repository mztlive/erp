"use client"

import * as React from "react"
import { ChevronRightIcon, TriangleAlertIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { deliveryAttention } from "../lib/delivery-progress"
import { inquiryDate } from "../lib/format"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedback,
    type InquiryRequirement,
    type InquiryLine,
    type InquiryFeedbackLine,
} from "../types"
import { InquiryRequirementDetail } from "./inquiry-requirement-detail"

export function InquiryProductsTable({
    requirement,
    feedback,
    waiting,
}: {
    requirement: InquiryRequirement
    feedback?: InquiryFeedback
    waiting: boolean
}) {
    const [activeLineId, setActiveLineId] = React.useState<string | null>(null)
    const returnFocus = React.useRef<HTMLButtonElement | null>(null)
    const details = useInquirySkuDetailsQuery(
        requirement.lines.map((line) => line.sku_id),
    )
    const activeLine = requirement.lines.find(
        (line) => line.line_id === activeLineId,
    )
    const results = new Map(
        feedback?.data.lines.map((line) => [line.line_id, line]),
    )
    const unitLabel = (code: string) =>
        details.units.get(code) ??
        (details.isPending ? "单位加载中" : "单位未匹配")
    return (
        <>
            {details.error && (
                <BusinessFailureState
                    id="sales-inquiry-products-units-retry"
                    title="商品单位资料读取失败"
                    error={details.error}
                    onRetry={() => void details.refetch()}
                />
            )}
            <div className="min-h-0 flex-1 overflow-hidden rounded-lg border [&>[data-slot=table-container]]:h-full [&>[data-slot=table-container]]:overflow-auto">
                <Table className="min-w-[1120px] table-fixed">
                    <TableHeader className="sticky top-0 z-10">
                        <TableRow>
                            <TableHead className="w-[28%]">
                                商品 / 规格
                            </TableHead>
                            <TableHead className="w-[15%] text-right">
                                需求 / 可供
                            </TableHead>
                            <TableHead className="w-[16%] text-right">
                                含税供给单价
                            </TableHead>
                            <TableHead className="w-[9%] text-right">
                                税率
                            </TableHead>
                            <TableHead className="w-[12%]">
                                要求交付日
                            </TableHead>
                            <TableHead className="w-[12%]">
                                报价有效至
                            </TableHead>
                            <TableHead className="w-[8%] text-right">
                                操作
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {requirement.lines.map((line) => (
                            <ProductRow
                                key={line.line_id}
                                line={line}
                                result={results.get(line.line_id)}
                                unit={unitLabel(line.unit_code)}
                                waiting={waiting}
                                hasFeedback={Boolean(feedback)}
                                active={activeLineId === line.line_id}
                                onDetails={(event) => {
                                    returnFocus.current = event.currentTarget
                                    setActiveLineId(line.line_id)
                                }}
                            />
                        ))}
                        {requirement.lines.length === 0 && (
                            <TableRow>
                                <TableCell
                                    colSpan={7}
                                    className="h-24 text-center text-muted-foreground"
                                >
                                    暂无商品需求
                                </TableCell>
                            </TableRow>
                        )}
                    </TableBody>
                </Table>
            </div>
            <QuickPreviewSheet
                idPrefix="sales-inquiry-product-detail"
                open={Boolean(activeLine)}
                onOpenChange={(open) => {
                    if (!open) setActiveLineId(null)
                }}
                finalFocus={returnFocus}
                size="detail"
                title={activeLine?.product_name ?? "需求详情"}
                identity={
                    activeLine ? `SKU 编号：${activeLine.sku_code}` : undefined
                }
                description={
                    activeLine?.specification &&
                    activeLine.specification !== "无规格"
                        ? activeLine.specification
                        : undefined
                }
                summary={
                    <p className="text-sm text-muted-foreground">
                        {feedback
                            ? `第 ${feedback.round} 轮反馈对应需求第 ${feedback.requirement_version} 版`
                            : "当前商品需求"}
                    </p>
                }
                footer={
                    <Button
                        id="sales-inquiry-product-detail-done"
                        variant="outline"
                        onClick={() => setActiveLineId(null)}
                    >
                        关闭
                    </Button>
                }
            >
                <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
                    {activeLine && (
                        <>
                            {results.get(activeLine.line_id)?.public_note && (
                                <p className="whitespace-pre-wrap rounded-lg bg-muted/40 p-3 text-sm">
                                    采购说明：
                                    {
                                        results.get(activeLine.line_id)
                                            ?.public_note
                                    }
                                </p>
                            )}
                            <InquiryRequirementDetail
                                line={activeLine}
                                unit={unitLabel(activeLine.unit_code)}
                                procurement={false}
                                image={null}
                            />
                        </>
                    )}
                </div>
            </QuickPreviewSheet>
        </>
    )
}

function ProductRow({
    line,
    result,
    unit,
    waiting,
    hasFeedback,
    active,
    onDetails,
}: {
    line: InquiryLine
    result?: InquiryFeedbackLine
    unit: string
    waiting: boolean
    hasFeedback: boolean
    active: boolean
    onDetails: (event: React.MouseEvent<HTMLButtonElement>) => void
}) {
    const prefix = `sales-inquiry-product-${toAutomationIdSegment(line.line_id)}`
    const prices = result?.prices.length ? result.prices : [null]
    const attention = deliveryAttention({ ...line, unit_code: unit })
    const shortage =
        result &&
        compareDecimal(
            line.quantity,
            result.available_quantity,
            line.quantity_scale,
        ) > 0
            ? subtractFixed(line.quantity, result.available_quantity, {
                  maxScale: line.quantity_scale,
                  outputScale: line.quantity_scale,
              })
            : null
    return (
        <>
            {prices.map((price) => (
                <TableRow
                    key={price?.price_id ?? line.line_id}
                    data-state={active ? "selected" : undefined}
                >
                    {price === prices[0] && (
                        <>
                            <TableCell
                                rowSpan={prices.length}
                                className="whitespace-normal py-2.5"
                            >
                                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                    <span className="font-medium leading-5 wrap-anywhere">
                                        {line.product_name}
                                    </span>
                                    {result ? (
                                        <StatusBadge
                                            label={
                                                INQUIRY_CONCLUSION_LABEL[
                                                    result.conclusion
                                                ]
                                            }
                                            tone={
                                                result.conclusion ===
                                                "available"
                                                    ? "success"
                                                    : result.conclusion ===
                                                        "pending"
                                                      ? "neutral"
                                                      : "warning"
                                            }
                                        />
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            {hasFeedback
                                                ? "反馈缺失"
                                                : waiting
                                                  ? "待采购反馈"
                                                  : "暂无报价"}
                                        </span>
                                    )}
                                </div>
                                <p className="mt-1 text-xs leading-4 text-muted-foreground wrap-anywhere">
                                    {line.specification &&
                                    line.specification !== "无规格"
                                        ? `${line.specification} · `
                                        : ""}
                                    SKU {line.sku_code}
                                </p>
                            </TableCell>
                            <TableCell
                                rowSpan={prices.length}
                                className="py-2.5 text-right"
                            >
                                <div>
                                    <QuantityValue
                                        value={line.quantity}
                                        unit=""
                                    />{" "}
                                    /{" "}
                                    {result ? (
                                        <QuantityValue
                                            value={result.available_quantity}
                                            unit={unit}
                                        />
                                    ) : (
                                        <>
                                            —{" "}
                                            <span className="text-xs text-muted-foreground">
                                                {unit}
                                            </span>
                                        </>
                                    )}
                                </div>
                                {shortage && (
                                    <p className="mt-1 text-xs text-warning-soft-foreground">
                                        缺{" "}
                                        <QuantityValue
                                            value={shortage}
                                            unit={unit}
                                        />
                                    </p>
                                )}
                                {attention && (
                                    <Button
                                        id={`${prefix}-delivery-attention`}
                                        variant="link"
                                        size="sm"
                                        className="mt-1 h-auto max-w-full items-start justify-end gap-1 p-0 text-right text-xs leading-5 whitespace-normal text-warning-soft-foreground"
                                        aria-label={`${line.product_name}：${attention}，查看需求详情`}
                                        onClick={onDetails}
                                    >
                                        <TriangleAlertIcon
                                            className="mt-0.5 size-3.5"
                                            aria-hidden="true"
                                        />
                                        <span className="min-w-0 wrap-anywhere">
                                            {attention}
                                        </span>
                                    </Button>
                                )}
                                {result?.public_note && (
                                    <Button
                                        id={`${prefix}-note`}
                                        variant="link"
                                        size="sm"
                                        className="h-auto p-0 text-xs"
                                        onClick={onDetails}
                                    >
                                        查看
                                        {result.conclusion === "available"
                                            ? "采购"
                                            : "缺货"}
                                        说明
                                    </Button>
                                )}
                            </TableCell>
                        </>
                    )}
                    <TableCell className="py-2.5 text-right">
                        <div>
                            {price?.unit_price_gross != null ? (
                                <MoneyValue value={price.unit_price_gross} />
                            ) : (
                                "—"
                            )}
                        </div>
                        {line.sales_reference_price_gross != null && (
                            <div className="mt-1 text-xs text-muted-foreground">
                                参考{" "}
                                <MoneyValue
                                    value={line.sales_reference_price_gross}
                                />
                            </div>
                        )}
                    </TableCell>
                    <TableCell className="py-2.5 text-right">
                        {price?.tax_rate != null ? (
                            <RateValue
                                value={multiplyFixed(price.tax_rate, "100", {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 2,
                                })}
                                precision={2}
                            />
                        ) : (
                            "—"
                        )}
                    </TableCell>
                    <TableCell className="py-2.5">
                        <CompactDate value={line.delivery_due_at} />
                    </TableCell>
                    <TableCell className="py-2.5">
                        {price ? (
                            <>
                                <CompactDate value={price.valid_until} />
                                {price.valid_until > 0 &&
                                    price.valid_until * 1000 <= Date.now() && (
                                        <span className="text-xs text-warning-soft-foreground">
                                            已过期
                                        </span>
                                    )}
                            </>
                        ) : (
                            "—"
                        )}
                    </TableCell>
                    {price === prices[0] && (
                        <TableCell
                            rowSpan={prices.length}
                            className="py-2.5 text-right"
                        >
                            <Button
                                id={`${prefix}-detail`}
                                variant="link"
                                size="sm"
                                className="px-0"
                                onClick={onDetails}
                            >
                                需求详情
                                <ChevronRightIcon aria-hidden="true" />
                            </Button>
                        </TableCell>
                    )}
                </TableRow>
            ))}
        </>
    )
}

function CompactDate({ value }: { value: number }) {
    const [date, time] = inquiryDate(value).split(" ")
    return (
        <div className="num">
            <p>{date}</p>
            {time && (
                <p className="mt-1 text-xs text-muted-foreground">{time}</p>
            )}
        </div>
    )
}
