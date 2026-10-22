"use client"

import * as React from "react"
import { TruckIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    QuickPreviewSheet,
} from "@/components/business"
import { TableRowActions } from "@/components/business/table-row-actions"
import { TableToolbarScope } from "@/components/business/table-toolbar"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { deliveryProgress } from "../lib/delivery-progress"
import { inquiryDateInput } from "../lib/format"
import { useInquirySkuDetailsQuery } from "../hooks/queries"
import type { InquiryLine, InquiryRequirement } from "../types"
import { InquiryRequirementDetail } from "./inquiry-requirement-detail"
import { InquiryProductImage } from "./inquiry-product-image"

/** 与草稿商品表保持同一列顺序、配送状态及侧栏入口，只读展示已提交需求。 */
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
        skuDetails.units.get(code) ??
        (skuDetails.isPending ? "单位加载中" : "单位未匹配")
    const productImage = (line: InquiryLine, detail = false) => (
        <InquiryProductImage
            key={line.sku_id}
            idPrefix={`${idPrefix}-${detail ? "detail" : toAutomationIdSegment(line.line_id)}-image`}
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
                <div className="min-w-0 overflow-hidden rounded-lg border [&>[data-slot=table-container]]:max-h-[65vh]">
                    <Table
                        className={
                            procurement
                                ? "min-w-[1072px] table-fixed"
                                : "min-w-[1232px] table-fixed"
                        }
                    >
                        <TableHeader className="sticky top-0 z-10">
                            <TableRow>
                                <TableHead className="w-20">图片</TableHead>
                                <TableHead>商品 / 规格</TableHead>
                                <TableHead className="w-36">需求数量</TableHead>
                                {!procurement && (
                                    <TableHead className="w-40 text-right">
                                        参考单价
                                    </TableHead>
                                )}
                                <TableHead className="w-52">
                                    要求交付日
                                </TableHead>
                                <TableHead className="w-48">
                                    配送 / 交付
                                </TableHead>
                                <TableHead className="sticky right-0 z-20 w-32 border-l text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {requirement.lines.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={procurement ? 6 : 7}
                                        className="h-32 text-center text-muted-foreground"
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
                                    return (
                                        <TableRow
                                            key={line.line_id}
                                            data-state={
                                                active ? "selected" : undefined
                                            }
                                        >
                                            <TableCell>
                                                {productImage(line)}
                                            </TableCell>
                                            <TableCell className="whitespace-normal py-4">
                                                <p className="font-medium leading-5 wrap-anywhere">
                                                    {line.product_name}
                                                </p>
                                                <p className="mt-1 text-xs leading-5 text-muted-foreground wrap-anywhere">
                                                    {line.specification &&
                                                    line.specification !==
                                                        "无规格"
                                                        ? `${line.specification} · `
                                                        : ""}
                                                    SKU {line.sku_code}
                                                </p>
                                            </TableCell>
                                            <TableCell>
                                                <QuantityValue
                                                    value={line.quantity}
                                                    unit={unit}
                                                />
                                            </TableCell>
                                            {!procurement && (
                                                <TableCell className="text-right">
                                                    {line.sales_reference_price_gross ? (
                                                        <MoneyValue
                                                            value={
                                                                line.sales_reference_price_gross
                                                            }
                                                        />
                                                    ) : (
                                                        <span className="text-muted-foreground">
                                                            —
                                                        </span>
                                                    )}
                                                </TableCell>
                                            )}
                                            <TableCell className="num">
                                                {inquiryDateInput(
                                                    line.delivery_due_at,
                                                ) || "未设置"}
                                            </TableCell>
                                            <TableCell>
                                                <div className="min-w-0 max-w-full space-y-1.5 whitespace-normal">
                                                    <Badge
                                                        className="h-auto min-h-5 max-w-full justify-start text-left leading-5 whitespace-normal wrap-anywhere"
                                                        variant={
                                                            progress.complete &&
                                                            !progress.overAllocated
                                                                ? "secondary"
                                                                : "warning"
                                                        }
                                                    >
                                                        {progress.label}
                                                    </Badge>
                                                    {line.delivery_terms && (
                                                        <p className="text-xs text-muted-foreground wrap-anywhere">
                                                            {
                                                                line.delivery_terms
                                                            }
                                                        </p>
                                                    )}
                                                </div>
                                            </TableCell>
                                            <TableCell className="sticky right-0 z-1 border-l bg-card group-hover/row:bg-row-hover">
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
                </div>
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
                    className="min-h-0 flex-1 overflow-y-auto px-7 py-6"
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
