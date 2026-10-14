"use client"

import * as React from "react"
import { z } from "zod"

import { MoneyValue } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { useCreateExpensePurchaseMutation } from "@/features/purchase-orders/hooks/queries"
import {
    PURCHASE_EXPENSE_CATEGORY_LABEL,
    type PurchaseExpenseCategory,
} from "@/features/purchase-orders/types"
import { getErrorMessage } from "@/lib/api/errors"
import { SUPPLIER_PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { compareDecimal, divideFixed, sumFixed } from "@/lib/fixed-decimal"

const CATEGORY_OPTIONS = (
    Object.keys(PURCHASE_EXPENSE_CATEGORY_LABEL) as PurchaseExpenseCategory[]
).map((value) => ({ value, label: PURCHASE_EXPENSE_CATEGORY_LABEL[value] }))

const WHOLE_ORDER = ""
const LINE_LIMIT = 50

const amountPattern = /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/
const percentPattern = /^(?:0|[1-9]\d?)(?:\.\d{1,2})?$/

const lineSchema = z.object({
    rowId: z.string(),
    category: z.string().min(1, "请选择费用类别"),
    description: z
        .string()
        .trim()
        .min(1, "请填写费用说明")
        .max(200, "费用说明不能超过 200 字"),
    grossAmount: z
        .string()
        .trim()
        .regex(amountPattern, "金额最多保留两位小数")
        .refine((value) => {
            try {
                return compareDecimal(value, "0", 2) > 0
            } catch {
                return false
            }
        }, "金额必须大于 0"),
    taxRatePercent: z
        .string()
        .trim()
        .regex(percentPattern, "税率填写 0–99 的百分数，最多两位小数"),
    salesOrderLineId: z.string(),
})

const schema = z.object({
    supplierId: z.string().min(1, "请选择收款供应商"),
    paymentTermCode: z.string().min(1, "请选择付款条件"),
    lines: z.array(lineSchema).min(1, "至少填写一行费用").max(LINE_LIMIT),
})

type FormValues = z.input<typeof schema>
type LineValues = FormValues["lines"][number]

function newLine(): LineValues {
    return {
        rowId: crypto.randomUUID(),
        category: "",
        description: "",
        grossAmount: "",
        taxRatePercent: "0",
        salesOrderLineId: WHOLE_ORDER,
    }
}

function defaults(): FormValues {
    return { supplierId: "", paymentTermCode: "", lines: [newLine()] }
}

function percentToRate(percent: string) {
    return divideFixed(percent.trim() || "0", "100", {
        numeratorMaxScale: 6,
        denominatorMaxScale: 0,
        outputScale: 6,
    })
}

function total(lines: readonly LineValues[]) {
    try {
        return sumFixed(
            lines.map((line) => line.grossAmount.trim() || "0"),
            { maxScale: 2, outputScale: 2 },
        )
    } catch {
        return null
    }
}

export type ExpensePurchaseCreated = {
    purchaseOrderId: string
    purchaseNo: string
}

/**
 * 从销售单追加费用采购单：选收款供应商与付款条件，填写费用行后创建并提交审批。
 * 不经供给分配，不计入销售明细覆盖；费用发生后在采购单上确认发生。
 */
export function ExpensePurchaseCreateDialog({
    salesOrderId,
    salesLines,
    open,
    onOpenChange,
    onCreated,
}: {
    salesOrderId: string
    salesLines: ReadonlyArray<{ id: string; name: string }>
    open: boolean
    onOpenChange: (open: boolean) => void
    onCreated?: (created: ExpensePurchaseCreated) => void
}) {
    const mutation = useCreateExpensePurchaseMutation(salesOrderId)
    const [requestKey, setRequestKey] = React.useState(() =>
        crypto.randomUUID(),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const lineOptions = [
        { value: WHOLE_ORDER, label: "整张销售单" },
        ...salesLines.map((line) => ({ value: line.id, label: line.name })),
    ]
    const form = useAppForm({
        defaultValues: defaults(),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            setFailure(null)
            try {
                const created = await mutation.mutateAsync({
                    supplierId: value.supplierId,
                    paymentTermCode: value.paymentTermCode,
                    lines: value.lines.map((line) => ({
                        category: line.category as PurchaseExpenseCategory,
                        description: line.description.trim(),
                        grossAmount: line.grossAmount.trim(),
                        inputTaxRate: percentToRate(line.taxRatePercent),
                        salesOrderLineId: line.salesOrderLineId || undefined,
                    })),
                    idempotencyKey: `expense-purchase:${requestKey}`,
                })
                onOpenChange(false)
                onCreated?.({
                    purchaseOrderId: created.purchase_order_id,
                    purchaseNo: created.purchase_no,
                })
            } catch (error) {
                setFailure(getErrorMessage(error, "创建失败，请稍后重试。"))
            }
        },
    })

    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset(defaults())
            setRequestKey(crypto.randomUUID())
            setFailure(null)
        }
        wasOpen.current = open
    }, [form, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-3xl"
                closeButtonId="sales-orders-expense-purchase-close"
            >
                <DialogHeader>
                    <DialogTitle>追加附加费用</DialogTitle>
                    <DialogDescription>
                        为本单发生、由我方承担、客户不另付的费用（如第三方物流、额外包装、上门安装）创建费用采购单，提交后进入采购审批。客户另付费的事项请改单添加明细；要交给客户的物品请作为赠品明细。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="supplierId"
                            children={(field) => (
                                <div className="grid gap-1.5">
                                    <Label htmlFor="sales-orders-expense-purchase-supplier">
                                        收款供应商
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </Label>
                                    <SupplierSearchCombobox
                                        id="sales-orders-expense-purchase-supplier"
                                        value={field.state.value || undefined}
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
                                        placeholder="选择已建档的收款方"
                                    />
                                </div>
                            )}
                        />
                        <form.AppField
                            name="paymentTermCode"
                            children={(field) => (
                                <field.SelectField
                                    id="sales-orders-expense-purchase-payment-term"
                                    label="付款条件"
                                    required
                                    allowClear={false}
                                    options={SUPPLIER_PAYMENT_TERM_OPTIONS}
                                    placeholder="选择付款条件"
                                />
                            )}
                        />
                    </div>
                    <form.AppField name="lines" mode="array">
                        {(array) => (
                            <section className="space-y-3">
                                <div className="flex items-center justify-between gap-2">
                                    <h3 className="text-sm font-semibold">
                                        费用明细 · {array.state.value.length} 行
                                    </h3>
                                    <Button
                                        id="sales-orders-expense-purchase-add-line"
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={
                                            array.state.value.length >=
                                            LINE_LIMIT
                                        }
                                        onClick={() =>
                                            array.pushValue(newLine())
                                        }
                                    >
                                        添加一行
                                    </Button>
                                </div>
                                {array.state.value.map((line, index) => {
                                    const prefix = `sales-orders-expense-purchase-line-${line.rowId}`
                                    return (
                                        <div
                                            key={line.rowId}
                                            className="grid gap-3 rounded-lg border p-3 sm:grid-cols-[8rem_minmax(0,1fr)_8rem_6rem]"
                                        >
                                            <form.AppField
                                                name={`lines[${index}].category`}
                                            >
                                                {(field) => (
                                                    <field.SelectField
                                                        id={`${prefix}-category`}
                                                        label="类别"
                                                        required
                                                        allowClear={false}
                                                        options={
                                                            CATEGORY_OPTIONS
                                                        }
                                                        placeholder="类别"
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`lines[${index}].description`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-description`}
                                                        label="费用说明"
                                                        required
                                                        placeholder="例如：加急专车配送"
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`lines[${index}].grossAmount`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-gross`}
                                                        label="含税金额"
                                                        required
                                                        inputMode="decimal"
                                                        inputClassName="num"
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField
                                                name={`lines[${index}].taxRatePercent`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-tax-rate`}
                                                        label="税率（%）"
                                                        required
                                                        inputMode="decimal"
                                                        inputClassName="num"
                                                    />
                                                )}
                                            </form.AppField>
                                            <div className="flex items-end gap-3 sm:col-span-4">
                                                <form.AppField
                                                    name={`lines[${index}].salesOrderLineId`}
                                                >
                                                    {(field) => (
                                                        <field.SelectField
                                                            id={`${prefix}-sales-line`}
                                                            label="归属"
                                                            allowClear={false}
                                                            options={
                                                                lineOptions
                                                            }
                                                            className="min-w-0 flex-1"
                                                        />
                                                    )}
                                                </form.AppField>
                                                <Button
                                                    id={`${prefix}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    disabled={
                                                        array.state.value
                                                            .length <= 1
                                                    }
                                                    onClick={() =>
                                                        array.removeValue(index)
                                                    }
                                                >
                                                    移除
                                                </Button>
                                            </div>
                                        </div>
                                    )
                                })}
                            </section>
                        )}
                    </form.AppField>
                    <form.Subscribe selector={(state) => state.values.lines}>
                        {(lines) => (
                            <div className="flex items-baseline justify-end gap-3 text-sm">
                                <span className="text-muted-foreground">
                                    费用合计（含税）
                                </span>
                                <MoneyValue
                                    value={total(lines)}
                                    size="section"
                                />
                            </div>
                        )}
                    </form.Subscribe>
                    {failure ? (
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="sales-orders-expense-purchase-cancel"
                            type="button"
                            variant="ghost"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-orders-expense-purchase-submit"
                                label="创建并提交审批"
                                pendingLabel="提交中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
