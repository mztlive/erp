"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    DataTable,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import type { PurchaseOrderLineRow } from "@/features/purchase-orders/components/purchase-order-surfaces-lines-table"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import {
    positiveDecimal,
    taxRateValid,
} from "@/features/purchase-orders/lib/purchase-order-validation"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { compactFixed, divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type LineEditDraft = {
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
}

export type LineEdits = Record<string, LineEditDraft>

type LineEditSetter = React.Dispatch<React.SetStateAction<LineEdits>>

function patchLineEdit(
    setLineEdits: LineEditSetter,
    lineId: string,
    fallbackTaxRate: string,
    patch: Partial<LineEditDraft>,
) {
    setLineEdits((prev) => ({
        ...prev,
        [lineId]: {
            ...prev[lineId],
            inputTaxRate: prev[lineId]?.inputTaxRate ?? fallbackTaxRate,
            ...patch,
        },
    }))
}

function lineSubtitle(line: PurchaseOrderLineRow) {
    if (line.lineType === "LOGISTICS_FEE") return "物流费用"
    if (line.procurementConfirmationLineId) {
        return line.salesAllocationLabel ?? `确认分行 · ${line.itemName}`
    }
    return "商品/服务"
}

function taxRateInputValue(raw: string) {
    if (raw === "") return ""
    try {
        return compactFixed(
            multiplyFixed(raw, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 4,
            }),
        )
    } catch {
        return raw
    }
}

function parseTaxRateInput(raw: string) {
    if (raw === "") return raw
    try {
        return compactFixed(
            divideFixed(raw, "100", {
                numeratorMaxScale: 4,
                denominatorMaxScale: 0,
                outputScale: 6,
            }),
        )
    } catch {
        return raw
    }
}

function LineNumberInput({
    id,
    value,
    ariaLabel,
    widthClass,
    invalid,
    invalidMessage,
    suffix,
    onValueChange,
}: {
    id?: string
    value: string
    ariaLabel: string
    widthClass: string
    invalid: boolean
    invalidMessage: string
    suffix?: string
    onValueChange: (value: string) => void
}) {
    return (
        <>
            <input
                id={id}
                className={cn(
                    "num rounded border border-border bg-background px-2 py-1 text-right text-sm",
                    widthClass,
                )}
                value={value}
                onChange={(event) => onValueChange(event.target.value)}
                aria-label={ariaLabel}
            />
            {suffix ? (
                <span className="ml-1 text-xs text-muted-foreground">
                    {suffix}
                </span>
            ) : null}
            {invalid ? (
                <span className="block text-tiny text-destructive">
                    {invalidMessage}
                </span>
            ) : null}
        </>
    )
}

function buildPurchaseOrderEditLineColumns(
    lineEdits: LineEdits,
    setLineEdits: LineEditSetter,
): ColumnDef<PurchaseOrderLineRow>[] {
    return [
        {
            id: "item",
            accessorFn: (row) => row.itemName,
            header: "项目",
            meta: { label: "项目", width: "flex" },
            cell: ({ row }) => {
                const line = row.original
                return (
                    <div className="whitespace-normal">
                        <div className="font-medium">{line.itemName}</div>
                        <div className="text-tiny text-muted-foreground">
                            {lineSubtitle(line)}
                        </div>
                    </div>
                )
            },
        },
        {
            id: "quantity",
            accessorKey: "quantity",
            header: "数量",
            meta: {
                label: "数量",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const line = row.original
                if (line.lineType === "LOGISTICS_FEE") return "—"
                const draft = lineEdits[line.lineId]
                return (
                    <LineNumberInput
                        id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-quantity`}
                        value={draft?.quantity ?? ""}
                        ariaLabel={`${line.itemName} 数量`}
                        widthClass="w-20"
                        invalid={
                            !positiveDecimal(draft?.quantity ?? line.quantity)
                        }
                        invalidMessage="须为正数"
                        onValueChange={(value) =>
                            patchLineEdit(
                                setLineEdits,
                                line.lineId,
                                line.inputTaxRate,
                                { quantity: value },
                            )
                        }
                    />
                )
            },
        },
        {
            id: "unitCost",
            accessorKey: "unitCostGross",
            header: "含税单价",
            meta: {
                label: "含税单价",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const line = row.original
                const draft = lineEdits[line.lineId]
                return (
                    <LineNumberInput
                        id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-cost`}
                        value={draft?.unitCostGross ?? ""}
                        ariaLabel={`${line.itemName} 含税单价`}
                        widthClass="w-28"
                        invalid={
                            !positiveDecimal(
                                draft?.unitCostGross ?? line.unitCostGross,
                            )
                        }
                        invalidMessage="须为正数"
                        onValueChange={(value) =>
                            patchLineEdit(
                                setLineEdits,
                                line.lineId,
                                line.inputTaxRate,
                                { unitCostGross: value },
                            )
                        }
                    />
                )
            },
        },
        {
            id: "taxRate",
            accessorKey: "inputTaxRate",
            header: "税率",
            meta: {
                label: "税率",
                width: "rate",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const line = row.original
                const draft = lineEdits[line.lineId]
                const storedRate = draft?.inputTaxRate ?? line.inputTaxRate
                return (
                    <LineNumberInput
                        id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-tax-rate`}
                        value={taxRateInputValue(storedRate)}
                        ariaLabel={`${line.itemName} 税率（%）`}
                        widthClass="w-20"
                        suffix="%"
                        invalid={!taxRateValid(storedRate)}
                        invalidMessage="税率须为 0–100 的百分数"
                        onValueChange={(value) =>
                            patchLineEdit(
                                setLineEdits,
                                line.lineId,
                                line.inputTaxRate,
                                { inputTaxRate: parseTaxRateInput(value) },
                            )
                        }
                    />
                )
            },
        },
        {
            id: "delivery",
            accessorKey: "expectedDeliveryDate",
            header: "交期（只读）",
            meta: { label: "交期", width: "default" },
            cell: ({ row }) => (
                <span className="text-xs text-muted-foreground">
                    {row.original.expectedDeliveryDate ?? "—"}
                </span>
            ),
        },
    ]
}

export function EditSurface({
    order,
    lineEdits,
    setLineEdits,
    draftEditToken,
    canSubmit,
    savePending,
    onSave,
    onSubmitOpen,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    lineEdits: LineEdits
    setLineEdits: LineEditSetter
    draftEditToken: string | null
    canSubmit: boolean
    savePending: boolean
    onSave: () => void
    onSubmitOpen: () => void
}) {
    const lines = order.currentContent.lines
    const columns = React.useMemo(
        () => buildPurchaseOrderEditLineColumns(lineEdits, setLineEdits),
        [lineEdits, setLineEdits],
    )

    return (
        <div className="space-y-4">
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>
                        {order.identity.reviewStatus === "REJECTED"
                            ? "被驳回待修改"
                            : "采购草稿"}
                    </CardTitle>
                    <CardDescription>
                        来源销售 {order.header.salesOrderNo}
                        。供应商、类型、履约责任、付款条件和交期来自创建依据，本卡只读。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4 pt-4">
                    {!draftEditToken ? (
                        <Alert variant="warning">
                            <AlertTitle>正在进入编辑…</AlertTitle>
                            <AlertDescription>
                                编辑内容仅保存在当前页面；刷新或关闭将丢失，请及时保存。
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            正在编辑中
                        </p>
                    )}

                    <div className="grid gap-3 sm:grid-cols-2">
                        <div className="space-y-1.5">
                            <Label>供应商（只读）</Label>
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-3 py-2 text-sm",
                                )}
                            >
                                {order.header.supplierSnapshot}
                            </div>
                        </div>
                        <div className="space-y-1.5">
                            <Label>采购类型 / 履约责任（只读）</Label>
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-3 py-2 text-sm",
                                )}
                            >
                                {PURCHASE_TYPE_LABEL[order.header.purchaseType]}{" "}
                                ·{" "}
                                {
                                    FULFILLMENT_RESPONSIBILITY_LABEL[
                                        order.header.fulfillmentResponsibility
                                    ]
                                }
                            </div>
                        </div>
                        <div className="space-y-1.5 sm:col-span-2">
                            <Label>付款条件（只读）</Label>
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-3 py-2 text-sm",
                                )}
                            >
                                {order.header.paymentTermLabel}
                            </div>
                        </div>
                    </div>
                </CardContent>
            </Card>

            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>本单可调</CardTitle>
                    <CardDescription>
                        可改数量、含税单价和税率。⌘S 保存 · ⌘↵ 打开提交确认。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4 pt-4">
                    <DataTable
                        id={`procurement-orders-detail-edit-table-${order.identity.purchaseOrderId}`}
                        data={[...lines]}
                        columns={columns}
                        getRowId={(row) => row.lineId}
                        rowCount={lines.length}
                        rowLabel={(row) => row.itemName}
                        caption="本单可调明细"
                        layout="flush"
                        density="compact"
                        showPagination={false}
                        showColumnVisibility={false}
                        defaultColumnPinning={{ left: ["item"] }}
                        emptyTitle="暂无采购明细"
                    />

                    <div className="flex flex-wrap gap-2">
                        <Button
                            id={`procurement-orders-detail-edit-save-${order.identity.purchaseOrderId}`}
                            type="button"
                            variant="outline"
                            disabled={!draftEditToken || savePending}
                            onClick={onSave}
                        >
                            {savePending ? "保存中…" : "保存草稿"}
                        </Button>
                        <Button
                            id={`procurement-orders-detail-edit-submit-${order.identity.purchaseOrderId}`}
                            type="button"
                            disabled={
                                !draftEditToken || !canSubmit || savePending
                            }
                            onClick={onSubmitOpen}
                        >
                            提交审批
                        </Button>
                    </div>
                </CardContent>
            </Card>
        </div>
    )
}
