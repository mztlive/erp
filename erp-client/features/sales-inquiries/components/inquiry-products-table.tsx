"use client"

import * as React from "react"
import { MapPinIcon, TriangleAlertIcon, TruckIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    QuickPreviewSheet,
    RateValue,
    detailWorkspaceStyles,
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
import { inquiryDateInput } from "../lib/format"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedback,
    type InquiryFeedbackLine,
    type InquiryLine,
    type InquiryRequirement,
} from "../types"
import { InquiryProductImage } from "./inquiry-product-image"
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
        details.units.get(code) ?? (details.isPending ? "加载中" : "")

    return (
        <>
            {details.error && (
                <BusinessFailureState
                    id="sales-inquiry-products-units-retry"
                    title="商品图片与单位资料读取失败"
                    error={details.error}
                    onRetry={() => void details.refetch()}
                />
            )}
            <div
                data-slot="detail-inline-table"
                className="min-w-0 border-y border-border/70"
            >
                <Table className="min-w-[880px] table-fixed">
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品 / 规格</TableHead>
                            <TableHead
                                data-align="end"
                                className="w-40 text-right"
                            >
                                需求 / 可供
                            </TableHead>
                            <TableHead
                                data-align="end"
                                className="w-36 text-right"
                            >
                                含税供给单价
                            </TableHead>
                            <TableHead
                                data-align="end"
                                className="w-20 text-right"
                            >
                                税率
                            </TableHead>
                            <TableHead className="w-28">要求交付日</TableHead>
                            <TableHead className="w-28 text-right">
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
                                image={
                                    <InquiryProductImage
                                        idPrefix={`sales-inquiry-product-${toAutomationIdSegment(line.line_id)}-image`}
                                        size="sm"
                                        assetId={details.images.get(
                                            line.sku_id,
                                        )}
                                        label={line.product_name}
                                        pending={
                                            !details.images.has(line.sku_id) &&
                                            details.isPending
                                        }
                                        lookupFailed={
                                            !details.images.has(line.sku_id) &&
                                            Boolean(details.error)
                                        }
                                    />
                                }
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
                                    colSpan={6}
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
                size="preview"
                title={activeLine?.product_name ?? "配送地址"}
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
                    <p className="text-muted-foreground">
                        {feedback
                            ? `第 ${feedback.round} 轮反馈 · 需求第 ${feedback.requirement_version} 版`
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
                {activeLine && (
                    <>
                        {results.get(activeLine.line_id)?.public_note && (
                            <div className="mb-4 rounded-lg border border-border/60 bg-surface-sunken/70 p-3 text-body-compact">
                                <p className="text-xs font-medium text-muted-foreground">
                                    采购说明
                                </p>
                                <p className="mt-1 whitespace-pre-wrap text-foreground wrap-anywhere">
                                    {
                                        results.get(activeLine.line_id)
                                            ?.public_note
                                    }
                                </p>
                            </div>
                        )}
                        <InquiryRequirementDetail
                            line={activeLine}
                            unit={unitLabel(activeLine.unit_code)}
                            procurement
                            compact
                            image={null}
                        />
                    </>
                )}
            </QuickPreviewSheet>
        </>
    )
}

function ProductRow({
    line,
    result,
    unit,
    image,
    waiting,
    hasFeedback,
    active,
    onDetails,
}: {
    line: InquiryLine
    result?: InquiryFeedbackLine
    unit: string
    image: React.ReactNode
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
    const specLine = [
        line.specification && line.specification !== "无规格"
            ? line.specification
            : null,
        `SKU ${line.sku_code}`,
    ]
        .filter(Boolean)
        .join(" · ")
    const dueDate = inquiryDateInput(line.delivery_due_at)

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
                                className="whitespace-normal py-3 align-top"
                            >
                                <div className="flex items-start gap-2.5">
                                    {image}
                                    <div className="min-w-0 flex-1">
                                        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                            <span className="font-medium leading-5 text-foreground wrap-anywhere">
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
                                        <p
                                            className="num mt-0.5 truncate text-xs text-muted-foreground"
                                            title={specLine}
                                        >
                                            {specLine}
                                        </p>
                                    </div>
                                </div>
                            </TableCell>
                            <TableCell
                                rowSpan={prices.length}
                                data-align="end"
                                className="py-3 text-right align-top"
                            >
                                <div className="num">
                                    <QuantityValue
                                        value={line.quantity}
                                        unit=""
                                    />{" "}
                                    <span className="text-muted-foreground">
                                        /
                                    </span>{" "}
                                    {result ? (
                                        <QuantityValue
                                            value={result.available_quantity}
                                            unit={unit}
                                            className="font-medium text-foreground"
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            —{unit ? ` ${unit}` : ""}
                                        </span>
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
                                        className="mt-1 h-auto max-w-full items-start justify-end gap-1 p-0 text-right text-xs whitespace-normal text-warning-soft-foreground"
                                        aria-label={`${line.product_name}：${attention}，查看${line.fulfillment_kind === "physical" ? "配送地址" : "交付要求"}`}
                                        onClick={onDetails}
                                    >
                                        <TriangleAlertIcon
                                            className="mt-0.5 size-3.5 shrink-0"
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
                                        className="mt-0.5 h-auto p-0 text-xs"
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
                    <TableCell
                        data-align="end"
                        className="py-3 text-right align-top"
                    >
                        <div>
                            {price?.unit_price_gross != null ? (
                                <MoneyValue
                                    value={price.unit_price_gross}
                                    display="exact"
                                    showHint={false}
                                    className="font-medium text-foreground"
                                />
                            ) : (
                                <span className="text-muted-foreground">—</span>
                            )}
                        </div>
                        {line.sales_reference_price_gross != null && (
                            <div className="mt-1 text-xs text-muted-foreground">
                                销售价{" "}
                                <MoneyValue
                                    value={line.sales_reference_price_gross}
                                    display="exact"
                                    showHint={false}
                                />
                            </div>
                        )}
                    </TableCell>
                    <TableCell
                        data-align="end"
                        className="py-3 text-right align-top"
                    >
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
                            <span className="text-muted-foreground">—</span>
                        )}
                    </TableCell>
                    <TableCell className="num py-3 align-top">
                        {dueDate ? (
                            <span>{dueDate}</span>
                        ) : (
                            <span className="text-muted-foreground">
                                未设置
                            </span>
                        )}
                    </TableCell>
                    {price === prices[0] && (
                        <TableCell
                            rowSpan={prices.length}
                            className="py-3 text-right align-top"
                        >
                            <Button
                                id={`${prefix}-detail`}
                                aria-haspopup="dialog"
                                variant="ghost"
                                size="sm"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                onClick={onDetails}
                            >
                                {line.fulfillment_kind === "physical" ? (
                                    <MapPinIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                        className="size-3.5"
                                    />
                                ) : (
                                    <TruckIcon
                                        aria-hidden="true"
                                        data-icon="inline-start"
                                        className="size-3.5"
                                    />
                                )}
                                {line.fulfillment_kind === "physical"
                                    ? "配送地址"
                                    : "交付要求"}
                            </Button>
                        </TableCell>
                    )}
                </TableRow>
            ))}
        </>
    )
}
