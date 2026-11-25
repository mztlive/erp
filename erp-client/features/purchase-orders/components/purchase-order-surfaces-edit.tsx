"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessFailureState,
    DataTable,
    DetailInlineTable,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
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
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "@/features/purchase-orders/types"
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
        return line.salesAllocationLabel ?? `关联销售明细 · ${line.itemName}`
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
                inputMode="decimal"
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
                <span className="mt-0.5 block text-xs text-destructive">
                    {invalidMessage}
                </span>
            ) : null}
        </>
    )
}

const EditLineContext = React.createContext<{
    lineEdits: LineEdits
    setLineEdits: LineEditSetter
    allowZeroQuantity: boolean
    options: DeliveryOptions
} | null>(null)

function useEditLine() {
    const context = React.useContext(EditLineContext)
    if (!context) throw new Error("采购明细单元格需要草稿编辑上下文")
    return context
}

// 固定单元格组件身份，输入更新或交付选项返回时保留当前输入控件。
const editLineColumns: ColumnDef<PurchaseOrderLineRow>[] = [
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
                    <div className="text-xs text-muted-foreground">
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
        cell: function QuantityCell({ row }) {
            const { lineEdits, setLineEdits, allowZeroQuantity, options } =
                useEditLine()
            const line = row.original
            if (line.lineType === "LOGISTICS_FEE") return "—"
            const draft = lineEdits[line.lineId]
            return (
                <LineNumberInput
                    id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-quantity`}
                    value={draft?.quantity ?? line.quantity ?? ""}
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
        cell: function UnitCostCell({ row }) {
            const { lineEdits, setLineEdits } = useEditLine()
            const line = row.original
            const draft = lineEdits[line.lineId]
            return (
                <LineNumberInput
                    id={`procurement-orders-detail-edit-row-${toAutomationIdSegment(line.lineId)}-cost`}
                    value={draft?.unitCostGross ?? line.unitCostGross ?? ""}
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
        cell: function TaxRateCell({ row }) {
            const { lineEdits, setLineEdits } = useEditLine()
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
        cell: function DeliveryCell({ row }) {
            const { lineEdits, setLineEdits, options } = useEditLine()
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
                            "num mt-0.5 block text-xs",
                            late ? "text-destructive" : "text-muted-foreground",
                        )}
                    >
                        销售履约期限 {option.salesDueDate}
                    </span>
                </>
            )
        },
    },
]

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
                        unit={line.unitName}
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
    editLockPending = false,
    editLockError,
    onRetryEditLock,
    canSubmit = false,
    savePending,
    onSave,
    onSubmitOpen,
}: {
    order: NonNullable<ReturnType<typeof usePurchaseOrderCenterQuery>["data"]>
    lineEdits: LineEdits
    setLineEdits: LineEditSetter
    draftEditToken: string | null
    editLockPending?: boolean
    editLockError?: unknown
    onRetryEditLock?: () => void
    /** 弹窗复用时在表底渲染保存/提交按钮；详情页编辑态由吸顶页头承载，避免同屏双按钮。 */
    canSubmit?: boolean
    savePending: boolean
    onSave?: () => void
    onSubmitOpen?: () => void
}) {
    const lines = order.currentContent.lines
    const allowZeroQuantity = order.sourceUpdate?.affected === true
    const showInlineActions = Boolean(onSave || onSubmitOpen)
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

    return (
        <DetailWorkspaceSection
            width="full"
            title={
                order.identity.status === "REJECTED" ||
                order.identity.reviewStatus === "REJECTED"
                    ? "被驳回待修改"
                    : "调整采购明细"
            }
            subtitle="可改数量、含税单价、税率、预计交付日和收件分配；预计交付日不晚于销售履约期限。⌘S 保存草稿 · ⌘↵ 提交审批。"
        >
            {!draftEditToken ? (
                editLockError && !editLockPending ? (
                    <BusinessFailureState
                        id="procurement-orders-detail-edit-lock"
                        title="获取编辑锁失败"
                        error={editLockError}
                        onRetry={onRetryEditLock}
                        retryLabel="重新获取编辑锁"
                    />
                ) : (
                    <Alert variant="warning">
                        <AlertTitle>正在准备编辑会话…</AlertTitle>
                        <AlertDescription>
                            修改内容保存在当前页面，刷新或离开前请先保存草稿。
                        </AlertDescription>
                    </Alert>
                )
            ) : null}

            <dl className="flex flex-wrap items-center gap-x-6 gap-y-2 border-y border-border/60 py-2.5 text-xs text-muted-foreground">
                {showInlineActions ? (
                    <div className="flex items-center gap-1.5">
                        <dt>供应商</dt>
                        <dd className="font-medium text-foreground">
                            {order.header.supplierSnapshot}
                        </dd>
                    </div>
                ) : null}
                <div className="flex items-center gap-1.5">
                    <dt>履约责任</dt>
                    <dd className="font-medium text-foreground">
                        {
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                                order.header.fulfillmentResponsibility
                            ]
                        }
                    </dd>
                </div>
                {order.header.targetWarehouseName ? (
                    <div className="flex items-center gap-1.5">
                        <dt>目标收货仓</dt>
                        <dd className="font-medium text-foreground">
                            {order.header.targetWarehouseName}
                        </dd>
                    </div>
                ) : null}
                <div className="flex items-center gap-1.5">
                    <dt>付款条件</dt>
                    <dd className="font-medium text-foreground">
                        {order.header.paymentTermLabel}
                    </dd>
                </div>
                <span className="text-muted-foreground/80">
                    （以上条款继承自创建依据，不可修改）
                </span>
            </dl>

            <fieldset
                disabled={!draftEditToken || savePending}
                className="min-w-0 space-y-4"
            >
                <EditLineContext.Provider
                    value={{
                        lineEdits,
                        setLineEdits,
                        allowZeroQuantity,
                        options,
                    }}
                >
                    <DetailInlineTable>
                        <DataTable
                            id={`procurement-orders-detail-edit-table-${order.identity.purchaseOrderId}`}
                            data={[...lines]}
                            columns={editLineColumns}
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
                    </DetailInlineTable>
                </EditLineContext.Provider>
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
            </fieldset>

            {showInlineActions ? (
                <div className="flex flex-wrap items-center gap-2 pt-2">
                    {onSave ? (
                        <LoadingButton
                            id={`procurement-orders-detail-edit-save-${order.identity.purchaseOrderId}`}
                            loading={savePending}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className={detailWorkspaceStyles.subtleButton}
                            disabled={!draftEditToken || savePending}
                            onClick={onSave}
                        >
                            {savePending ? "保存中…" : "保存草稿"}
                        </LoadingButton>
                    ) : null}
                    {onSubmitOpen ? (
                        <Button
                            id={`procurement-orders-detail-edit-submit-${order.identity.purchaseOrderId}`}
                            type="button"
                            size="sm"
                            className="px-3.5"
                            disabled={
                                !draftEditToken || !canSubmit || savePending
                            }
                            onClick={onSubmitOpen}
                        >
                            提交审批
                        </Button>
                    ) : null}
                </div>
            ) : null}
        </DetailWorkspaceSection>
    )
}
