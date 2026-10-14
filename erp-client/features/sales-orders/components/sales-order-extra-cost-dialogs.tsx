"use client"

import * as React from "react"

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
import type {
    ExtraCostCategory,
    SalesExtraCost,
} from "@/features/sales-orders/api/sales-order-extra-costs"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    useReduceSalesExtraCostMutation,
    useRegisterSalesExtraCostMutation,
} from "@/features/sales-orders/hooks/queries"
import {
    REGISTER_CATEGORY_OPTIONS,
    newExtraCostRequestKey,
    reduceDefaults,
    reduceExtraCostSchema,
    registerDefaults,
    registerExtraCostSchema,
    taxRateFromPercent,
    todayDate,
} from "@/features/sales-orders/lib/extra-cost"
import { getErrorMessage } from "@/lib/api/errors"

const WHOLE_ORDER = ""

/** 财务按凭证登记附加费用。 */
export function RegisterExtraCostDialog({
    order,
    open,
    onOpenChange,
}: {
    order: SalesOrderDetailView
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useRegisterSalesExtraCostMutation(order.id)
    const [requestKey, setRequestKey] = React.useState(() =>
        newExtraCostRequestKey("register"),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const lineOptions = [
        { value: WHOLE_ORDER, label: "整张销售单" },
        ...order.lineItems
            .filter((line) => line.quantity !== "0")
            .map((line) => ({
                value: line.id,
                label: line.sku ? `${line.name}（${line.sku}）` : line.name,
            })),
    ]
    const form = useAppForm({
        defaultValues: registerDefaults(),
        validators: { onChange: registerExtraCostSchema },
        onSubmit: async ({ value }) => {
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    category: value.category as ExtraCostCategory,
                    description: value.description.trim(),
                    grossAmount: value.grossAmount.trim(),
                    inputTaxRate: taxRateFromPercent(value.taxRatePercent),
                    occurredOn: value.occurredOn,
                    supplierId: value.supplierId || undefined,
                    salesOrderLineId: value.salesOrderLineId || undefined,
                    evidence: value.evidence as File,
                    idempotencyKey: requestKey,
                })
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error, "登记失败，请稍后重试。"))
            }
        },
    })

    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) {
            form.reset(registerDefaults())
            setRequestKey(newExtraCostRequestKey("register"))
            setFailure(null)
        }
        wasOpen.current = open
    }, [form, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-xl"
                closeButtonId="sales-orders-extra-cost-register-close"
            >
                <DialogHeader>
                    <DialogTitle>登记附加费用</DialogTitle>
                    <DialogDescription>
                        登记已在 ERP
                        外支付或无需付款的费用，登记后直接计入本单实际经营盈亏。需要通过
                        ERP
                        向供应商付款的费用请由采购追加费用采购，不要在这里重复登记。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="category"
                            children={(field) => (
                                <field.SelectField
                                    id="sales-orders-extra-cost-register-category"
                                    label="费用类别"
                                    required
                                    allowClear={false}
                                    options={REGISTER_CATEGORY_OPTIONS}
                                    placeholder="选择费用类别"
                                />
                            )}
                        />
                        <form.AppField
                            name="occurredOn"
                            children={(field) => (
                                <field.DateField
                                    id="sales-orders-extra-cost-register-occurred-on"
                                    label="发生日期"
                                    required
                                    clearable={false}
                                    disabledDates={{
                                        after: new Date(
                                            `${todayDate()}T00:00:00`,
                                        ),
                                    }}
                                />
                            )}
                        />
                    </div>
                    <form.AppField
                        name="description"
                        children={(field) => (
                            <field.TextField
                                id="sales-orders-extra-cost-register-description"
                                label="费用说明"
                                required
                                placeholder="例如：第三方加急快递"
                            />
                        )}
                    />
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="grossAmount"
                            children={(field) => (
                                <field.TextField
                                    id="sales-orders-extra-cost-register-gross"
                                    label="含税金额（元）"
                                    required
                                    inputMode="decimal"
                                    placeholder="0.00"
                                />
                            )}
                        />
                        <form.AppField
                            name="taxRatePercent"
                            children={(field) => (
                                <field.TextField
                                    id="sales-orders-extra-cost-register-tax-rate"
                                    label="税率（%）"
                                    required
                                    inputMode="decimal"
                                    description="无票或免税填 0"
                                />
                            )}
                        />
                    </div>
                    <form.AppField
                        name="salesOrderLineId"
                        children={(field) => (
                            <field.SelectField
                                id="sales-orders-extra-cost-register-line"
                                label="归属明细"
                                allowClear={false}
                                options={lineOptions}
                                description="能明确归到某条明细时选择，否则归属整张销售单"
                            />
                        )}
                    />
                    <form.AppField
                        name="supplierId"
                        children={(field) => (
                            <div className="grid gap-1.5">
                                <Label htmlFor="sales-orders-extra-cost-register-supplier">
                                    收款供应商
                                </Label>
                                <SupplierSearchCombobox
                                    id="sales-orders-extra-cost-register-supplier"
                                    value={field.state.value || undefined}
                                    onValueChange={(id) =>
                                        field.handleChange(id ?? "")
                                    }
                                    placeholder="选填，收款方已建档时选择"
                                />
                            </div>
                        )}
                    />
                    <form.AppField
                        name="evidence"
                        children={(field) => (
                            <field.EvidenceUploadField
                                id="sales-orders-extra-cost-register-evidence"
                                label="费用凭证"
                                required
                                uploadLabel="上传账单、运单或支付凭证"
                            />
                        )}
                    />
                    {failure ? (
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="sales-orders-extra-cost-register-cancel"
                            type="button"
                            variant="ghost"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-orders-extra-cost-register-submit"
                                label="登记费用"
                                pendingLabel="登记中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

/** 冲减一笔财务登记的附加费用。 */
export function ReduceExtraCostDialog({
    order,
    cost,
    onOpenChange,
}: {
    order: SalesOrderDetailView
    cost: SalesExtraCost | null
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useReduceSalesExtraCostMutation(order.id)
    const remaining = cost?.remainingGrossAmount ?? "0.00"
    const [requestKey, setRequestKey] = React.useState(() =>
        newExtraCostRequestKey("reduce"),
    )
    const [failure, setFailure] = React.useState<string | null>(null)
    const schema = React.useMemo(
        () => reduceExtraCostSchema(remaining),
        [remaining],
    )
    const form = useAppForm({
        defaultValues: reduceDefaults(),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (!cost) return
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    costEntryId: cost.id,
                    value: {
                        description: value.description.trim(),
                        grossAmount: value.grossAmount.trim(),
                        occurredOn: value.occurredOn,
                        evidence: value.evidence as File,
                        idempotencyKey: requestKey,
                    },
                })
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error, "冲减失败，请稍后重试。"))
            }
        },
    })

    const openId = cost?.id ?? null
    const lastOpenId = React.useRef<string | null>(null)
    React.useEffect(() => {
        if (openId && lastOpenId.current !== openId) {
            form.reset(reduceDefaults())
            setRequestKey(newExtraCostRequestKey("reduce"))
            setFailure(null)
        }
        lastOpenId.current = openId
    }, [form, openId])

    return (
        <Dialog open={Boolean(cost)} onOpenChange={onOpenChange}>
            <DialogContent
                className="sm:max-w-lg"
                closeButtonId="sales-orders-extra-cost-reduce-close"
            >
                <DialogHeader>
                    <DialogTitle>冲减附加费用</DialogTitle>
                    <DialogDescription>
                        {cost
                            ? `「${cost.description}」剩余 ${remaining} 元可冲减。冲减不修改原费用，追加一笔冲减记录。`
                            : null}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="description"
                        children={(field) => (
                            <field.TextField
                                id="sales-orders-extra-cost-reduce-description"
                                label="冲减原因"
                                required
                                placeholder="例如：快递公司退回部分运费"
                            />
                        )}
                    />
                    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
                        <form.AppField
                            name="grossAmount"
                            children={(field) => (
                                <field.TextField
                                    id="sales-orders-extra-cost-reduce-gross"
                                    label="冲减金额（元）"
                                    required
                                    inputMode="decimal"
                                    placeholder={remaining}
                                />
                            )}
                        />
                        <form.AppField
                            name="occurredOn"
                            children={(field) => (
                                <field.DateField
                                    id="sales-orders-extra-cost-reduce-occurred-on"
                                    label="冲减日期"
                                    required
                                    clearable={false}
                                    disabledDates={{
                                        after: new Date(
                                            `${todayDate()}T00:00:00`,
                                        ),
                                    }}
                                />
                            )}
                        />
                    </div>
                    <form.AppField
                        name="evidence"
                        children={(field) => (
                            <field.EvidenceUploadField
                                id="sales-orders-extra-cost-reduce-evidence"
                                label="冲减凭证"
                                required
                                uploadLabel="上传退款或更正凭证"
                            />
                        )}
                    />
                    {failure ? (
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="sales-orders-extra-cost-reduce-cancel"
                            type="button"
                            variant="ghost"
                            disabled={mutation.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-orders-extra-cost-reduce-submit"
                                label="确认冲减"
                                pendingLabel="冲减中…"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
