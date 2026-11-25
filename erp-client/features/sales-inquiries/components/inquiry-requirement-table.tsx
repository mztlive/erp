"use client"

import * as React from "react"
import { ChevronRightIcon, TruckIcon } from "lucide-react"
import {
    BusinessFailureState,
    DetailInlineTable,
    MoneyValue,
    QuantityValue,
    QuickPreviewSheet,
    TableRowActions,
    TableToolbarScope,
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
import { cn } from "@/lib/utils"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import { deliveryProgress } from "../lib/delivery-progress"
import { inquiryDateInput } from "../lib/format"
import type { InquiryLine, InquiryRequirement } from "../types"
import { InquiryProductImage } from "./inquiry-product-image"
import { InquiryRequirementDetail } from "./inquiry-requirement-detail"

/** 与草稿商品表保持同一列顺序、缩略图融合与配送抽屉入口，只读展示已提交需求。 */
export function InquiryRequirementTable({
    requirement,
    procurement = false,
    idPrefix,
}: {
    requirement: InquiryRequirement
    procurement?: boolean
    idPrefix: string
}) {
    const [activeLineId, setActiveLineId] = React.useState<string | null>(null)
    const returnFocus = React.useRef<HTMLButtonElement | null>(null)
    const skuDetails = useInquirySkuDetailsQuery(
        requirement.lines.map((line) => line.sku_id),
    )
    const activeLine = requirement.lines.find(
        (line) => line.line_id === activeLineId,
    )
    const unitLabel = (code: string) =>
        skuDetails.units.get(code) ?? (skuDetails.isPending ? "加载中" : "")
    const productImage = (line: InquiryLine, detail = false) => (
        <InquiryProductImage
            key={line.sku_id}
            idPrefix={`${idPrefix}-${detail ? "detail" : toAutomationIdSegment(line.line_id)}-image`}
            size="sm"
            assetId={skuDetails.images.get(line.sku_id)}
            label={line.product_name}
            pending={
                !skuDetails.images.has(line.sku_id) && skuDetails.isPending
            }
            lookupFailed={
                !skuDetails.images.has(line.sku_id) && Boolean(skuDetails.error)
            }
            detail={detail}
        />
    )

    return (
        <TableToolbarScope>
            <section className="min-w-0 space-y-3" aria-label="商品需求">
                {skuDetails.error && (
                    <BusinessFailureState
                        id={`${idPrefix}-sku-details-retry`}
                        title="商品图片与单位资料读取失败"
                        error={skuDetails.error}
                        onRetry={() => void skuDetails.refetch()}
                    />
                )}
                <DetailInlineTable
                    frame="none"
                    className="min-w-0 border-y border-border/70"
                >
                    <Table
                        className={
                            procurement
                                ? "min-w-[760px] table-fixed"
                                : "min-w-[880px] table-fixed"
                        }
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>商品 / 规格</TableHead>
                                <TableHead data-align="end" className="w-32">
                                    需求数量
                                </TableHead>
                                {!procurement && (
                                    <TableHead
                                        data-align="end"
                                        className="w-32 text-right"
                                    >
                                        销售价
                                    </TableHead>
                                )}
                                <TableHead className="w-28">
                                    要求交付日
                                </TableHead>
                                <TableHead className="w-48">
                                    配送 / 交付
                                </TableHead>
                                <TableHead className="sticky right-0 z-20 w-28 text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {requirement.lines.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={procurement ? 5 : 6}
                                        className="h-28 text-center text-muted-foreground"
                                    >
                                        暂无商品需求
                                    </TableCell>
                                </TableRow>
                            ) : (
                                requirement.lines.map((line) => {
                                    const prefix = `${idPrefix}-line-${toAutomationIdSegment(line.line_id)}`
                                    const unit = unitLabel(line.unit_code)
                                    const progress = deliveryProgress({
                                        ...line,
                                        unit_code: unit,
                                    })
                                    const active = line.line_id === activeLineId
                                    const specLine = [
                                        line.specification &&
                                        line.specification !== "无规格"
                                            ? line.specification
                                            : null,
                                        `SKU ${line.sku_code}`,
                                    ]
                                        .filter(Boolean)
                                        .join(" · ")
                                    const dueDate = inquiryDateInput(
                                        line.delivery_due_at,
                                    )
                                    return (
                                        <TableRow
                                            key={line.line_id}
                                            data-state={
                                                active ? "selected" : undefined
                                            }
                                        >
                                            <TableCell className="whitespace-normal py-3 align-top">
                                                <div className="flex items-start gap-2.5">
                                                    {productImage(line)}
                                                    <div className="min-w-0 flex-1">
                                                        <p className="font-medium text-foreground wrap-anywhere">
                                                            {line.product_name}
                                                        </p>
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
                                                data-align="end"
                                                className="num py-3 text-right align-top"
                                            >
                                                <QuantityValue
                                                    value={line.quantity}
                                                    unit={unit}
                                                />
                                            </TableCell>
                                            {!procurement && (
                                                <TableCell
                                                    data-align="end"
                                                    className="num py-3 text-right align-top"
                                                >
                                                    {line.sales_reference_price_gross ? (
                                                        <MoneyValue
                                                            value={
                                                                line.sales_reference_price_gross
                                                            }
                                                            display="exact"
                                                            showHint={false}
                                                        />
                                                    ) : (
                                                        <span className="text-muted-foreground">
                                                            —
                                                        </span>
                                                    )}
                                                </TableCell>
                                            )}
                                            <TableCell className="num py-3 align-top">
                                                {dueDate ? (
                                                    <span>{dueDate}</span>
                                                ) : (
                                                    <span className="text-muted-foreground">
                                                        未设置
                                                    </span>
                                                )}
                                            </TableCell>
                                            <TableCell className="whitespace-normal py-3 align-top">
                                                <button
                                                    id={`${prefix}-delivery-preview`}
                                                    type="button"
                                                    onClick={(event) => {
                                                        returnFocus.current =
                                                            event.currentTarget
                                                        setActiveLineId(
                                                            line.line_id,
                                                        )
                                                    }}
                                                    className={cn(
                                                        "group/delivery -mx-1.5 -my-1 flex w-[calc(100%+0.75rem)] flex-col items-start gap-1 rounded-md px-1.5 py-1 text-left transition-colors",
                                                        "hover:bg-muted/70 focus-visible:outline-2 focus-visible:outline-ring",
                                                    )}
                                                >
                                                    <span className="flex w-full items-center justify-between gap-1">
                                                        <StatusBadge
                                                            className="h-auto min-h-5 max-w-full justify-start text-left whitespace-normal wrap-anywhere"
                                                            tone={
                                                                progress.complete &&
                                                                !progress.overAllocated
                                                                    ? "neutral"
                                                                    : "warning"
                                                            }
                                                            label={
                                                                progress.label
                                                            }
                                                        />
                                                        <ChevronRightIcon
                                                            aria-hidden="true"
                                                            className="size-3.5 shrink-0 text-muted-foreground/45 transition-colors group-hover/delivery:text-primary"
                                                        />
                                                    </span>
                                                    {line.delivery_terms && (
                                                        <span className="line-clamp-2 text-xs text-muted-foreground wrap-anywhere">
                                                            {
                                                                line.delivery_terms
                                                            }
                                                        </span>
                                                    )}
                                                </button>
                                            </TableCell>
                                            <TableCell className="sticky right-0 z-1 bg-card py-3 align-top group-hover/row:bg-row-hover group-data-[state=selected]/row:bg-row-selected">
                                                <TableRowActions
                                                    moreId={`${prefix}-more`}
                                                    moreLabel={`${line.product_name}更多操作`}
                                                    moreIconOnly
                                                    actions={[
                                                        {
                                                            id: `${prefix}-delivery`,
                                                            label:
                                                                line.fulfillment_kind ===
                                                                "physical"
                                                                    ? "配送地址"
                                                                    : "交付",
                                                            icon: TruckIcon,
                                                            onClick: (
                                                                event,
                                                            ) => {
                                                                returnFocus.current =
                                                                    event.currentTarget as HTMLButtonElement
                                                                setActiveLineId(
                                                                    line.line_id,
                                                                )
                                                            },
                                                        },
                                                    ]}
                                                />
                                            </TableCell>
                                        </TableRow>
                                    )
                                })
                            )}
                        </TableBody>
                    </Table>
                </DetailInlineTable>
            </section>
            <QuickPreviewSheet
                idPrefix={`${idPrefix}-delivery`}
                size="detail"
                open={Boolean(activeLine)}
                onOpenChange={(open) => {
                    if (!open) setActiveLineId(null)
                }}
                finalFocus={returnFocus}
                title={activeLine?.product_name ?? "配送与交付"}
                identity={
                    activeLine ? `SKU 编号：${activeLine.sku_code}` : undefined
                }
                description={
                    activeLine?.specification &&
                    activeLine.specification !== "无规格"
                        ? activeLine.specification
                        : undefined
                }
                footer={
                    <Button
                        id={`${idPrefix}-delivery-done`}
                        type="button"
                        variant="outline"
                        onClick={() => setActiveLineId(null)}
                    >
                        关闭
                    </Button>
                }
            >
                <div
                    key={activeLine?.line_id}
                    className="min-h-0 flex-1 overflow-y-auto px-sheet-inline py-5"
                >
                    {activeLine && (
                        <InquiryRequirementDetail
                            line={activeLine}
                            unit={unitLabel(activeLine.unit_code)}
                            procurement={procurement}
                            image={productImage(activeLine, true)}
                        />
                    )}
                </div>
            </QuickPreviewSheet>
        </TableToolbarScope>
    )
}
