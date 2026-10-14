"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    DataTable,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import type { PurchaseDeliveryLineOption } from "@/features/purchase-orders/api/purchase-delivery-options"
import { PurchaseDeliveryAllocationEditor } from "@/features/purchase-orders/components/purchase-delivery-allocation-editor"
import type { PurchaseOrderLineRow } from "@/features/purchase-orders/components/purchase-order-surfaces-lines-table"
import {
    usePurchaseDeliveryOptionsQuery,
    usePurchaseOrderCenterQuery,
} from "@/features/purchase-orders/hooks/queries"
import {
    afterDueDates,
    deliveryAddressKey,
    editableDeliveryRows,
    expectedDateAfterDue,
    positiveDeliveryRows,
    removedDeliveryRows,
} from "@/features/purchase-orders/lib/delivery-adjustment"
import type {
    DraftLineEdit,
    DraftLineEdits,
} from "@/features/purchase-orders/lib/draft-line-payload"
import {
    positiveDecimal,
    purchaseQuantityValid,
    taxRateValid,
} from "@/features/purchase-orders/lib/purchase-order-validation"
import {
    FULFILLMENT_RESPONSIBILITY_LABEL,
    PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import { compactFixed, divideFixed, multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type LineEditDraft = DraftLineEdit

export type LineEdits = DraftLineEdits

type DeliveryOptions = ReadonlyMap<string, PurchaseDeliveryLineOption>

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

/** 行可调整的交付安排范围；仍在销售当前版本中的商品行才有。 */
function lineOption(line: PurchaseOrderLineRow, options: DeliveryOptions) {
    if (line.lineType !== "ITEM_SERVICE" || !line.salesOrderLineId)
        return undefined
    return options.get(line.salesOrderLineId)
}

/** 本行在编辑器中显示的收件分配。 */
function lineDeliveryRows(
    line: PurchaseOrderLineRow,
    option: PurchaseDeliveryLineOption,
    draft: LineEditDraft | undefined,
) {
    return (
        draft?.deliveryAllocations ??
        editableDeliveryRows(option, line.deliveryAllocations ?? [])
    )
}

/** 只使用一个收件地址时，数量变化同步到该地址；已分到多个地址时由采购手动调整。 */
function followSingleAddress(
    line: PurchaseOrderLineRow,
    option: PurchaseDeliveryLineOption | undefined,
    draft: LineEditDraft | undefined,
    quantity: string,
): Partial<LineEditDraft> {
    if (!option?.requiresAddresses) return {}
    const rows = lineDeliveryRows(line, option, draft)
    const original = positiveDeliveryRows(line.deliveryAllocations ?? [])
    const target =
        original.length === 1
            ? rows.findIndex(
                  (row) =>
                      deliveryAddressKey(row) ===
                      deliveryAddressKey(original[0]),
              )
            : rows.length === 1
              ? 0
              : -1
    if (target < 0) return {}
    const others = rows.filter((_, index) => index !== target)
    if (positiveDeliveryRows(others).length > 0) return {}
    return {
        deliveryAllocations: rows.map((row, index) =>
            index === target ? { ...row, quantity } : row,
        ),
    }
}

function lineSubtitle(line: PurchaseOrderLineRow) {
    if (line.lineType === "LOGISTICS_FEE") return "费用"
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
            <Input
                id={id}
                className={cn("num text-right", widthClass)}
                value={value}
                onChange={(event) => onValueChange(event.target.value)}
                aria-label={ariaLabel}
                aria-invalid={invalid || undefined}
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
    allowZeroQuantity: boolean,
    options: DeliveryOptions,
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
            header: () => (
                <>
                    数量
                    <RequiredMark />
                </>
            ),
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
                            !purchaseQuantityValid(
                                draft?.quantity ?? line.quantity,
                                allowZeroQuantity,
                            )
                        }
                        invalidMessage={
                            allowZeroQuantity ? "须为 0 或正数" : "须为正数"
                        }
                        onValueChange={(value) =>
                            patchLineEdit(
                                setLineEdits,
                                line.lineId,
                                line.inputTaxRate,
                                {
                                    quantity: value,
                                    ...followSingleAddress(
                                        line,
                                        lineOption(line, options),
                                        draft,
                                        value,
                                    ),
                                },
                            )
                        }
                    />
                )
            },
        },
        {
            id: "unitCost",
            accessorKey: "unitCostGross",
            header: () => (
                <>
                    含税单价
                    <RequiredMark />
                </>
            ),
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
            header: () => (
                <>
                    税率
                    <RequiredMark />
                </>
            ),
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
            header: () => (
                <>
                    预计交付日
                    <RequiredMark />
                </>
            ),
            meta: { label: "预计交付日", width: "default" },
            cell: ({ row }) => {
                const line = row.original
                const option = lineOption(line, options)
                if (!option) {
                    return (
                        <span className="text-xs text-muted-foreground">
                            {line.expectedDeliveryDate ?? "—"}
                        </span>
                    )
                }
                const value =
                    lineEdits[line.lineId]?.expectedDeliveryDate ??
                    line.expectedDeliveryDate ??
                    ""
                const late =
                    value !== line.expectedDeliveryDate &&
                    expectedDateAfterDue(value, option.salesDueDate)
                return (
                    <>
                        <DatePicker
                            id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-expected-date`}
                            size="sm"
                            className="w-40"
                            value={value}
                            clearable={false}
                            disabledDates={afterDueDates(option.salesDueDate)}
                            aria-invalid={late || undefined}
                            onValueChange={(next) =>
                                patchLineEdit(
                                    setLineEdits,
                                    line.lineId,
                                    line.inputTaxRate,
                                    { expectedDeliveryDate: next ?? "" },
                                )
                            }
                        />
                        <span
                            className={cn(
                                "block text-tiny",
                                late
                                    ? "text-destructive"
                                    : "text-muted-foreground",
                            )}
                        >
                            销售履约期限 {option.salesDueDate}
                        </span>
                    </>
                )
            },
        },
    ]
}

/** 须按收件地址分配的行逐行给出分配编辑。 */
function DeliveryAllocationEditors({
    lines,
    lineEdits,
    setLineEdits,
    options,
}: {
    lines: readonly PurchaseOrderLineRow[]
    lineEdits: LineEdits
    setLineEdits: LineEditSetter
    options: DeliveryOptions
}) {
    const editable = lines.flatMap((line) => {
        const option = lineOption(line, options)
        return option?.requiresAddresses ? [{ line, option }] : []
    })
    if (!editable.length) return null
    return (
        <div className="space-y-3">
            {editable.map(({ line, option }) => {
                const draft = lineEdits[line.lineId]
                return (
                    <PurchaseDeliveryAllocationEditor
                        key={line.lineId}
                        id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-delivery`}
                        option={option}
                        rows={lineDeliveryRows(line, option, draft)}
                        removed={removedDeliveryRows(
                            option,
                            line.deliveryAllocations ?? [],
                        )}
                        quantity={draft?.quantity ?? line.quantity ?? ""}
                        unit={line.unit}
                        onChange={(rows) =>
                            patchLineEdit(
                                setLineEdits,
                                line.lineId,
                                line.inputTaxRate,
                                { deliveryAllocations: rows },
                            )
                        }
                    />
                )
            })}
        </div>
    )
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
    const allowZeroQuantity = order.sourceUpdate?.affected === true
    const optionsQuery = usePurchaseDeliveryOptionsQuery(
        order.identity.purchaseOrderId,
        true,
    )
    const options = React.useMemo<DeliveryOptions>(
        () =>
            new Map(
                (optionsQuery.data ?? []).map((option) => [
                    option.salesOrderLineId,
                    option,
                ]),
            ),
        [optionsQuery.data],
    )
    const columns = React.useMemo(
        () =>
            buildPurchaseOrderEditLineColumns(
                lineEdits,
                setLineEdits,
                allowZeroQuantity,
                options,
            ),
        [lineEdits, setLineEdits, allowZeroQuantity, options],
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
                        。供应商、类型、履约责任和付款条件来自创建依据，本卡只读。
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
                        可改数量、含税单价、税率、预计交付日和收件分配；预计交付日不晚于销售履约期限，收件地址取自销售当前收件资料。⌘S
                        保存 · ⌘↵ 打开提交确认。
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
                    {optionsQuery.isError ? (
                        <p className="text-xs text-muted-foreground">
                            暂时读取不到销售当前的履约期限和收件资料，预计交付日与收件分配本次不可调整。
                        </p>
                    ) : null}
                    <DeliveryAllocationEditors
                        lines={lines}
                        lineEdits={lineEdits}
                        setLineEdits={setLineEdits}
                        options={options}
                    />

                    <div className="flex flex-wrap gap-2">
                        <LoadingButton
                            id={`procurement-orders-detail-edit-save-${order.identity.purchaseOrderId}`}
                            loading={savePending}
                            type="button"
                            variant="outline"
                            disabled={!draftEditToken || savePending}
                            onClick={onSave}
                        >
                            {savePending ? "保存中…" : "保存草稿"}
                        </LoadingButton>
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
