"use client"
import { useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { MoneyValue } from "@/components/business"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { loadReceivables } from "@/features/customer-receivables/api/loaders"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import {
    useInvoiceRequestAmounts,
    useInvoiceRequestCommands,
} from "../hooks/queries"
import type { InvoiceRequest, SubmitRequest } from "../api"

const schema = z.object({
    receivableAccountId: z.string(),
    salesOrderId: z.string().min(1, "请选择销售单"),
    amount: z
        .string()
        .regex(/^\d+(\.\d{1,2})?$/, "金额最多两位小数")
        .refine((v) => /[1-9]/.test(v), "请输入大于零的金额"),
    invoice_title: z.string().trim().min(1, "请输入开票抬头").max(256),
    tax_number: z.string().trim().min(1, "请输入税号").max(64),
    invoice_content: z.string().trim().min(1, "请输入开票内容").max(1000),
    reason: z.string().trim().min(1, "请输入申请事由").max(1000),
})
/** 申请表单保留未知结果载荷，锁定编辑后使用同一命令核对结果。 */
export function InvoiceRequestForm({
    salesOrderId,
    accountId,
    title,
    existing,
    onDone,
    onCancel,
}: {
    salesOrderId?: string
    accountId?: string
    title?: string
    existing?: InvoiceRequest
    onDone: (request: InvoiceRequest) => void
    onCancel: () => void
}) {
    const commands = useInvoiceRequestCommands()
    const pending = useRef<SubmitRequest | null>(null)
    const [uncertain, setUncertain] = useState(false)
    const [error, setError] = useState<unknown>(null)
    const form = useAppForm({
        defaultValues: {
            receivableAccountId:
                existing?.receivable_account_id ?? accountId ?? "",
            salesOrderId: existing?.sales_order_id ?? salesOrderId ?? "",
            amount: existing?.data.amount ?? "",
            invoice_title: existing?.data.invoice_title ?? title ?? "",
            tax_number: existing?.data.tax_number ?? "",
            invoice_content: existing?.data.invoice_content ?? "",
            reason: existing?.data.reason ?? "",
        },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            if (!resolvedAccountId) return
            const input = pending.current ?? {
                receivable_account_id: resolvedAccountId,
                request_id: existing?.id,
                expected_version: existing?.version,
                data: {
                    amount: value.amount,
                    invoice_title: value.invoice_title,
                    tax_number: value.tax_number,
                    invoice_content: value.invoice_content,
                    reason: value.reason,
                },
                idempotency_key: crypto.randomUUID(),
            }
            pending.current = input
            await send(input)
        },
    })
    const selectedOrder = useStore(form.store, (s) => s.values.salesOrderId)
    const accounts = useQuery({
        queryKey: ["invoice-requests", "source", selectedOrder],
        queryFn: () =>
            loadReceivables({
                view: "receivable",
                page: 1,
                pageSize: 100,
                salesOrderId: selectedOrder,
            }),
        enabled: Boolean(selectedOrder) && !accountId && !existing,
    })
    const selectedAccount = useStore(
        form.store,
        (s) => s.values.receivableAccountId,
    )
    const resolvedAccountId =
        existing?.receivable_account_id ??
        accountId ??
        (accounts.data?.items.find((account) => account.id === selectedAccount)
            ?.id ||
            (accounts.data?.items.length === 1
                ? accounts.data.items[0]?.id
                : undefined))
    const amounts = useInvoiceRequestAmounts(resolvedAccountId)
    const locked = commands.submit.isPending || uncertain
    /** 成功时关闭表单；未知结果保持同一请求，明确失败才允许修改。 */
    async function send(input: SubmitRequest) {
        setError(null)
        try {
            const result = await commands.submit.mutateAsync(input)
            pending.current = null
            setUncertain(false)
            onDone(result)
        } catch (err) {
            setError(err)
            const unknown = classifyFormalCommandError(err) === "unknown"
            setUncertain(unknown)
            if (!unknown) pending.current = null
        }
    }
    return (
        <form
            id="invoice-request-form"
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            {!salesOrderId && !existing ? (
                <form.AppField name="salesOrderId">
                    {(field) => (
                        <div className="space-y-2">
                            <label htmlFor="invoice-request-sales-order">
                                销售单
                            </label>
                            <SalesOrderSearchCombobox
                                id="invoice-request-sales-order"
                                value={field.state.value}
                                onValueChange={(id) => {
                                    field.handleChange(id ?? "")
                                    form.setFieldValue(
                                        "receivableAccountId",
                                        "",
                                    )
                                }}
                                disabled={locked}
                            />
                        </div>
                    )}
                </form.AppField>
            ) : null}
            {!accountId &&
            !existing &&
            (accounts.data?.items.length ?? 0) > 1 ? (
                <form.AppField name="receivableAccountId">
                    {(field) => (
                        <div className="space-y-2 text-sm">
                            <label htmlFor="invoice-request-account">
                                开票结算主体
                            </label>
                            <select
                                id="invoice-request-account"
                                className="h-9 w-full rounded-md border bg-background px-3"
                                value={field.state.value}
                                disabled={locked}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                            >
                                <option value="">选择本次开票的应收</option>
                                {accounts.data?.items.map((account) => (
                                    <option key={account.id} value={account.id}>
                                        第 {account.account_seq} 笔 ·{" "}
                                        {account.counterparty_party_name ??
                                            account.customer_name}{" "}
                                        · 未开票{" "}
                                        {account.open_invoiceable_total} 元
                                    </option>
                                ))}
                            </select>
                        </div>
                    )}
                </form.AppField>
            ) : null}
            {amounts.data ? (
                <p className="rounded-lg bg-muted p-3 text-sm">
                    本单可申请{" "}
                    <MoneyValue value={amounts.data.available_amount} />
                    ，批准后由财务开票。
                </p>
            ) : (
                <p className="text-sm text-muted-foreground">
                    {amounts.isError || accounts.isError
                        ? "销售应收读取失败，请重新选择或刷新。"
                        : selectedOrder &&
                            !accounts.isPending &&
                            !resolvedAccountId
                          ? "请选择本次开票的结算主体；如无应收，请核对销售单是否已生效。"
                          : "请选择已生效的销售单。"}
                </p>
            )}
            <fieldset disabled={locked} className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="amount">
                    {(field) => (
                        <field.TextField
                            id="invoice-request-amount"
                            label="本次申请金额（含税）"
                            inputMode="decimal"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="invoice_title">
                    {(field) => (
                        <field.TextField
                            id="invoice-request-title"
                            label="开票抬头"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="tax_number">
                    {(field) => (
                        <field.TextField
                            id="invoice-request-tax-number"
                            label="税号"
                            required
                        />
                    )}
                </form.AppField>
                <form.AppField name="invoice_content">
                    {(field) => (
                        <field.TextField
                            id="invoice-request-content"
                            label="开票内容"
                            required
                        />
                    )}
                </form.AppField>
                <div className="sm:col-span-2">
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id="invoice-request-reason"
                                label="申请事由"
                            />
                        )}
                    </form.AppField>
                </div>
            </fieldset>
            {error ? (
                <p role="alert" className="text-sm text-destructive">
                    {uncertain
                        ? "提交结果尚未确认，请核对本次提交结果，勿重复新建申请。"
                        : getErrorMessage(error, "提交失败，请检查后重试")}
                </p>
            ) : null}
            <div className="flex justify-end gap-2">
                <Button
                    id="invoice-request-form-cancel"
                    type="button"
                    variant="outline"
                    disabled={locked}
                    onClick={onCancel}
                >
                    取消
                </Button>
                {uncertain ? (
                    <Button
                        id="invoice-request-resolve"
                        type="button"
                        disabled={commands.submit.isPending}
                        onClick={() =>
                            pending.current && void send(pending.current)
                        }
                    >
                        核对提交结果
                    </Button>
                ) : (
                    <Button
                        id="invoice-request-submit"
                        type="submit"
                        disabled={
                            locked ||
                            !amounts.data ||
                            !/[1-9]/.test(amounts.data.available_amount)
                        }
                    >
                        提交审批
                    </Button>
                )}
            </div>
        </form>
    )
}
