"use client"

import {
    MoneyValue,
    QuantityValue,
    RateValue,
    surfacePanelClassName,
} from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import type { PurchaseOrderPreview } from "@/features/purchase-orders/lib/purchase-order-create-model"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

export type PurchaseOrderCreatePreviewProps = {
    previews: readonly PurchaseOrderPreview[]
}

/**
 * 按供应商拆分后的采购单预览，布局对齐采购单详情概览与明细。
 */
export function PurchaseOrderCreatePreview({
    previews,
}: PurchaseOrderCreatePreviewProps) {
    return (
        <div className="flex flex-col gap-4">
            {previews.map((preview, index) => (
                <article
                    key={preview.key}
                    className={cn(surfacePanelClassName, "overflow-hidden")}
                    data-testid={`purchase-create-preview-${preview.supplierId}`}
                >
                    <div className="border-b border-grid px-4 py-3 md:px-5">
                        <div className="flex flex-wrap items-baseline justify-between gap-2">
                            <h2 className="font-heading text-sm font-semibold">
                                预览采购单 {index + 1} · {preview.supplierName}
                            </h2>
                            <MoneyValue value={preview.totals.gross} />
                        </div>
                        <DescriptionList
                            columns="three"
                            className="mt-3 gap-y-3"
                        >
                            <DescriptionItem>
                                <DescriptionTerm>供应商</DescriptionTerm>
                                <DescriptionDetails>
                                    {preview.supplierName}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>采购类型</DescriptionTerm>
                                <DescriptionDetails>
                                    {PURCHASE_TYPE_LABEL[preview.purchaseType]}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>履约责任</DescriptionTerm>
                                <DescriptionDetails>
                                    {
                                        FULFILLMENT_RESPONSIBILITY_LABEL[
                                            preview.fulfillmentResponsibility
                                        ]
                                    }
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>付款条件</DescriptionTerm>
                                <DescriptionDetails>
                                    {preview.paymentTermLabel}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>明细行数</DescriptionTerm>
                                <DescriptionDetails className="num">
                                    {preview.lines.length}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>含税合计</DescriptionTerm>
                                <DescriptionDetails>
                                    <MoneyValue value={preview.totals.gross} />
                                </DescriptionDetails>
                            </DescriptionItem>
                        </DescriptionList>
                    </div>
                    <div className="overflow-x-auto">
                        <Table data-density="compact">
                            <TableHeader>
                                <TableRow>
                                    <TableHead>采购项目</TableHead>
                                    <TableHead data-align="end">数量</TableHead>
                                    <TableHead data-align="end">
                                        含税成本
                                    </TableHead>
                                    <TableHead data-align="end">
                                        进项税率
                                    </TableHead>
                                    <TableHead data-align="end">
                                        含税金额
                                    </TableHead>
                                    <TableHead data-align="end">
                                        预计交期
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {preview.lines.map((line) => (
                                    <TableRow key={line.salesOrderLineId}>
                                        <TableCell className="max-w-[16rem] whitespace-normal">
                                            <div className="font-medium">
                                                {line.itemName}
                                            </div>
                                            {line.itemSku ? (
                                                <div className="mt-0.5 text-xs text-muted-foreground">
                                                    {line.itemSku}
                                                </div>
                                            ) : null}
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={line.unit}
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.unitCostGross}
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <RateValue
                                                value={multiplyFixed(
                                                    line.inputTaxRate,
                                                    "100",
                                                    {
                                                        leftMaxScale: 6,
                                                        rightMaxScale: 0,
                                                        outputScale: 2,
                                                    },
                                                )}
                                                precision={2}
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={line.grossAmount}
                                            />
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="num"
                                        >
                                            {line.expectedDeliveryDate || "—"}
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                </article>
            ))}
        </div>
    )
}
