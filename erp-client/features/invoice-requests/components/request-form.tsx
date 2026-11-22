"use client"

import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { RequiredMark } from "@/components/ui/required-mark"
import { useRef, useState } from "react"
import { useQuery } from "@tanstack/react-query"
import { revalidateLogic, useStore } from "@tanstack/react-form"
import { z } from "zod"
import { PlusIcon, Trash2Icon } from "lucide-react"
import { useAppForm, withForm } from "@/components/form"
import { Input } from "@/components/ui/input"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { MoneyValue } from "@/components/business"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { loadReceivables } from "@/features/customer-receivables/api/loaders"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorCode, getErrorMessage } from "@/lib/api/errors"
import {
    useInvoiceRequestAmounts,
    useInvoiceRequestCommands,
    useInvoiceRequestSaveState,
} from "../hooks/queries"
import {
    requestStatusLabels,
    type InvoiceRequest,
    type InvoiceRequestLine,
    type SubmitRequest,
    type UpdateRequest,
} from "../api"
import {
    canonicalInvoiceDecimal,
    INVOICE_CONTENT_LIMIT,
    INVOICE_QUANTITY_SCALE,
    INVOICE_REQUEST_LINE_LIMIT,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
    invoiceLinesTotal,
    invoiceTotalMismatch,
    isPositiveInvoiceDecimal,
} from "../line-amounts"

type LineInput = InvoiceRequestLine & { rowKey: string }

const zeroLineMessage = "请提高数量或含税单价，使本行金额至少为 0.01 元。"
const totalMismatchMessage = "请调整明细或申请金额，使两者一致。"
const lineLimitMessage = `最多填写 ${INVOICE_REQUEST_LINE_LIMIT} 行；如需增加项目，请先删除一行或另发起一笔申请。`
const lineSchema = z.object({
    rowKey: z.string(),
    content: z
        .string()
        .trim()
        .min(1, "请输入开票内容")
        .max(INVOICE_CONTENT_LIMIT, `开票内容最多 ${INVOICE_CONTENT_LIMIT} 字`),
    quantity: z
        .string()
        .refine(
            (value) => isPositiveInvoiceDecimal(value, INVOICE_QUANTITY_SCALE),
            `请输入大于零的数量，最多 ${INVOICE_QUANTITY_SCALE} 位小数`,
        ),
    unit_price: z
        .string()
        .refine(
            (value) =>
                isPositiveInvoiceDecimal(value, INVOICE_UNIT_PRICE_SCALE),
            `请输入大于零的单价，最多 ${INVOICE_UNIT_PRICE_SCALE} 位小数`,
        ),
})

const schema = z
    .object({
        receivableAccountId: z.string(),
        salesOrderId: z.string().min(1, "请选择销售单"),
        amount: z
            .string()
            .regex(/^\d+(\.\d{1,2})?$/, "金额最多两位小数")
            .refine((v) => /[1-9]/.test(v), "请输入大于零的金额"),
        lines: z
            .array(lineSchema)
            .min(1, "请至少添加一行开票明细")
            .max(INVOICE_REQUEST_LINE_LIMIT, lineLimitMessage),
        reason: z.string().trim().min(1, "请输入申请事由").max(1000),
    })
    .superRefine(({ amount, lines }, context) => {
        const total = invoiceLinesTotal(lines)
        if (invoiceTotalMismatch(total, amount)) {
            context.addIssue({
                code: "custom",
                path: ["lines"],
                message: totalMismatchMessage,
            })
        }
        lines.forEach((line, index) => {
            if (invoiceLineAmount(line) === "0.00") {
                context.addIssue({
                    code: "custom",
                    path: ["lines", index, "unit_price"],
                    message: zeroLineMessage,
                })
            }
        })
    })

const formDefaults: z.input<typeof schema> = {
    receivableAccountId: "",
    salesOrderId: "",
    amount: "",
    lines: [],
    reason: "",
}

function initialLines(request?: InvoiceRequest): LineInput[] {
    const lines = request?.data.lines?.length
        ? request.data.lines
        : [
              {
                  content: request?.data.invoice_content ?? "",
                  quantity: "",
                  unit_price: "",
              },
          ]
    let nextKey = 0
    return lines.map((line) => ({ ...line, rowKey: `item-${++nextKey}` }))
}

/** 申请表单保留未知结果载荷，锁定编辑后使用同一命令核对结果。 */
export function InvoiceRequestForm({
    salesOrderId,
    accountId,
    existing: original,
    presentation = "inline",
    onDone,
    onCancel,
}: {
    salesOrderId?: string
    accountId?: string
    existing?: InvoiceRequest
    presentation?: "inline" | "dialog"
    onDone: (request: InvoiceRequest) => void
    onCancel: () => void
}) {
    const commands = useInvoiceRequestCommands()
    const [adopted, setAdopted] = useState<Pick<
        InvoiceRequest,
        "id" | "version" | "status"
    > | null>(null)
    const existing =
        original && adopted?.id === original.id
            ? { ...original, ...adopted }
            : original
    const saveState = useInvoiceRequestSaveState(existing?.id)
    const [checked, setChecked] = useState<Pick<
        InvoiceRequest,
        "id" | "version" | "status"
    > | null>(null)

    const pending = useRef<SubmitRequest | null>(null)
    const pendingSave = useRef<UpdateRequest | null>(null)
    const intent = useRef<"submit" | "save">("submit")
    const [saveUncertain, setSaveUncertain] = useState(false)
    const [uncertain, setUncertain] = useState(false)
    const [committedUnreadable, setCommittedUnreadable] = useState(false)
    const [error, setError] = useState<unknown>(null)
    const rework = useResubmissionReason({
        open: true,
        required: existing?.status === "rejected",
        id: "invoice-request-resubmission-reason",
        disabled:
            commands.submit.isPending ||
            commands.update.isPending ||
            uncertain ||
            saveUncertain,
    })
    const form = useAppForm({
        defaultValues: {
            receivableAccountId:
                existing?.receivable_account_id ?? accountId ?? "",
            salesOrderId: existing?.sales_order_id ?? salesOrderId ?? "",
            amount: existing?.data.amount ?? "",
            lines: initialLines(existing),
            reason: existing?.data.reason ?? "",
        },
        validationLogic: revalidateLogic(),
        validators: { onDynamic: schema },
        onSubmit: async ({ value }) => {
            if (
                commands.update.isPending ||
                commands.submit.isPending ||
                uncertain ||
                saveUncertain
            )
                return
            const lines = value.lines.map(
                ({ content, quantity, unit_price }) => ({
                    content: content.trim(),
                    quantity: canonicalInvoiceDecimal(quantity),
                    unit_price: canonicalInvoiceDecimal(unit_price),
                }),
            )
            const data = {
                amount: canonicalInvoiceDecimal(value.amount),
                invoice_content: lines[0]?.content ?? "",
                reason: value.reason.trim(),
                seller_name: amounts.data?.seller_name,
                seller_tax_number: amounts.data?.seller_tax_number,
                lines,
            }
            if (intent.current === "save" && existing) {
                const input = pendingSave.current ?? {
                    id: existing.id,
                    expected_version: existing.version,
                    data: {
                        ...data,
                        invoice_title:
                            amounts.data?.invoice_title ??
                            existing.data.invoice_title,
                        tax_number:
                            amounts.data?.tax_number ??
                            existing.data.tax_number,
                    },
                }
                pendingSave.current = input
                await save(input)
                return
            }
            if (
                !rework.valid ||
                !resolvedAccountId ||
                !amounts.data?.can_submit ||
                amounts.isError ||
                amounts.isFetching
            )
                return
            const input = pending.current ?? {
                resubmission_reason: rework.reason,
                receivable_account_id: resolvedAccountId,
                request_id: existing?.id,
                expected_version: existing?.version,
                data: {
                    ...data,
                    invoice_title: amounts.data.invoice_title,
                    tax_number: amounts.data.tax_number,
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
    const locked =
        commands.submit.isPending ||
        commands.update.isPending ||
        uncertain ||
        saveUncertain
    const latest = checked?.id === existing?.id ? checked : null
    const versionAdvanced = Boolean(
        latest &&
        pendingSave.current &&
        latest.version > pendingSave.current.expected_version,
    )
    const latestEditable =
        latest?.status === "draft" || latest?.status === "rejected"
    async function checkSave() {
        setChecked(null)
        const result = await saveState.refetch()
        if (result.isError) {
            setError(result.error)
            return
        }
        if (result.data) setChecked(result.data)
    }
    function adoptLatest() {
        if (!latest || !versionAdvanced || !latestEditable) return
        setAdopted(latest)
        pendingSave.current = null
        setSaveUncertain(false)
        setError(null)
        setChecked(null)
    }
    async function save(input: UpdateRequest) {
        setError(null)
        setChecked(null)
        try {
            const result = await commands.update.mutateAsync(input)
            pendingSave.current = null
            setSaveUncertain(false)
            onDone(result)
        } catch (err) {
            setError(err)
            const unknown = classifyFormalCommandError(err) === "unknown"
            setSaveUncertain((previous) => previous || unknown)
            if (!unknown && !saveUncertain) pendingSave.current = null
        }
    }
    /** 成功时关闭表单；未知结果保持同一请求，明确失败才允许修改。 */
    async function send(input: SubmitRequest) {
        setError(null)
        try {
            const result = await commands.submit.mutateAsync(input)
            pending.current = null
            setUncertain(false)
            setCommittedUnreadable(false)
            onDone(result)
        } catch (err) {
            setError(err)
            const committed =
                getErrorCode(err) === "COMMITTED_RESULT_UNAVAILABLE"
            const unknown =
                committed || classifyFormalCommandError(err) === "unknown"
            setCommittedUnreadable((previous) => previous || committed)
            setUncertain((previous) => previous || unknown)
            if (!unknown && !uncertain) pending.current = null
        }
    }
    const content = (
        <form
            id="invoice-request-form"
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                intent.current = "submit"
                void form.handleSubmit()
            }}
        >
            {!salesOrderId && !existing ? (
                <form.AppField name="salesOrderId">
                    {(field) => (
                        <div className="space-y-2">
                            <label htmlFor="invoice-request-sales-order">
                                销售单
                                <RequiredMark />
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
                        <field.SelectField
                            onRefresh={() =>
                                accounts.refetch({ throwOnError: true })
                            }
                            id="invoice-request-account"
                            label="开票结算主体"
                            placeholder="选择本次开票的应收"
                            disabled={locked}
                            options={(accounts.data?.items ?? []).map(
                                (account) => ({
                                    value: account.id,
                                    label: `第 ${account.account_seq} 笔 · ${account.counterparty_party_name ?? account.customer_name} · 未开票 ${account.open_invoiceable_total} 元`,
                                }),
                            )}
                        />
                    )}
                </form.AppField>
            ) : null}
            {!amounts.data ? (
                <p className="text-xs text-muted-foreground">
                    {amounts.isError || accounts.isError
                        ? "销售应收读取失败，请重新选择或刷新。"
                        : selectedOrder &&
                            !accounts.isPending &&
                            !resolvedAccountId
                          ? "请选择本次开票的结算主体；如无应收，请核对销售单是否已生效。"
                          : "请选择已生效的销售单。"}
                </p>
            ) : null}
            {amounts.data?.unavailable_reason ? (
                <p role="status" className="text-xs text-muted-foreground">
                    {amounts.data.unavailable_reason}
                </p>
            ) : null}
            <fieldset disabled={locked} className="grid gap-5 sm:grid-cols-2">
                <div className="rounded-lg border border-border/60 bg-muted/25 p-4 sm:col-span-2">
                    <div className="mb-3 flex flex-wrap items-center justify-between gap-2 border-b border-border/50 pb-2.5">
                        <span className="text-xs font-medium text-muted-foreground">
                            自动带入开票档案（只读）
                        </span>
                        {amounts.data ? (
                            <span className="text-xs text-muted-foreground">
                                本单可申请{" "}
                                <MoneyValue
                                    value={amounts.data.available_amount}
                                    className="font-semibold text-foreground"
                                />
                            </span>
                        ) : null}
                    </div>
                    <div className="grid gap-x-6 gap-y-3 sm:grid-cols-2">
                        <Field className="gap-0.5">
                            <FieldLabel
                                htmlFor="invoice-request-title"
                                className="text-xs font-normal text-muted-foreground"
                            >
                                购方开票抬头
                            </FieldLabel>
                            <Input
                                id="invoice-request-title"
                                readOnly
                                className="h-auto border-0 bg-transparent px-0 py-0 font-medium text-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0"
                                value={
                                    pending.current?.data.invoice_title ??
                                    amounts.data?.invoice_title ??
                                    existing?.data.invoice_title ??
                                    ""
                                }
                                placeholder="选择结算主体后自动带入"
                            />
                        </Field>
                        <Field className="gap-0.5">
                            <FieldLabel
                                htmlFor="invoice-request-tax-number"
                                className="text-xs font-normal text-muted-foreground"
                            >
                                购方纳税人识别号
                            </FieldLabel>
                            <Input
                                id="invoice-request-tax-number"
                                readOnly
                                className="num h-auto border-0 bg-transparent px-0 py-0 font-medium text-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0"
                                value={
                                    pending.current?.data.tax_number ??
                                    amounts.data?.tax_number ??
                                    existing?.data.tax_number ??
                                    ""
                                }
                                placeholder="从结算主体档案自动带入"
                            />
                        </Field>
                        <Field className="gap-0.5">
                            <FieldLabel
                                htmlFor="invoice-request-seller-name"
                                className="text-xs font-normal text-muted-foreground"
                            >
                                开票主体名称（销方）
                            </FieldLabel>
                            <Input
                                id="invoice-request-seller-name"
                                readOnly
                                className="h-auto border-0 bg-transparent px-0 py-0 font-medium text-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0"
                                value={
                                    pending.current?.data.seller_name ??
                                    pendingSave.current?.data.seller_name ??
                                    (amounts.data
                                        ? amounts.data.seller_name
                                        : existing?.data.seller_name) ??
                                    ""
                                }
                                placeholder={
                                    amounts.data
                                        ? "销方名称未带入"
                                        : "选择应收后自动带入"
                                }
                            />
                        </Field>
                        <Field className="gap-0.5">
                            <FieldLabel
                                htmlFor="invoice-request-seller-tax-number"
                                className="text-xs font-normal text-muted-foreground"
                            >
                                开票主体纳税人识别号
                            </FieldLabel>
                            <Input
                                id="invoice-request-seller-tax-number"
                                readOnly
                                className="num h-auto border-0 bg-transparent px-0 py-0 font-medium text-foreground shadow-none hover:bg-transparent focus-visible:border-transparent focus-visible:ring-0"
                                value={
                                    pending.current?.data.seller_tax_number ??
                                    pendingSave.current?.data
                                        .seller_tax_number ??
                                    (amounts.data
                                        ? amounts.data.seller_tax_number
                                        : existing?.data.seller_tax_number) ??
                                    ""
                                }
                                placeholder={
                                    amounts.data
                                        ? "销方税号未带入"
                                        : "选择应收后自动带入"
                                }
                            />
                        </Field>
                    </div>
                </div>
                <form.AppField name="amount">
                    {(field) => (
                        <field.TextField
                            money
                            id="invoice-request-amount"
                            label="本次申请金额（含税）"
                            inputMode="decimal"
                            required
                        />
                    )}
                </form.AppField>
                <InvoiceLinesFields form={form} locked={locked} />
                <div className="sm:col-span-2">
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id="invoice-request-reason"
                                label="申请事由"
                                required
                            />
                        )}
                    </form.AppField>
                </div>
            </fieldset>
            {rework.field}
            {error ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {saveUncertain
                        ? "保存结果尚未确认，请核对本次保存结果。"
                        : committedUnreadable
                          ? "已提交，结果暂不可读取。请稍后核对本次提交结果。"
                          : uncertain
                            ? "提交结果尚未确认，请核对本次提交结果，勿重复新建申请。"
                            : getErrorMessage(error, "提交失败，请检查后重试")}
                </p>
            ) : null}
            {saveUncertain ? (
                <div
                    role="status"
                    className="space-y-3 rounded-lg border p-3 text-body-compact"
                >
                    <p>刚才的保存结果尚未确认，已保留您当前填写的内容。</p>
                    <LoadingButton
                        id="invoice-request-check-save"
                        type="button"
                        variant="outline"
                        loading={saveState.isFetching}
                        disabled={
                            commands.update.isPending || saveState.isFetching
                        }
                        onClick={() => void checkSave()}
                    >
                        读取最新版本
                    </LoadingButton>
                    {saveState.isError ? (
                        <p role="alert" className="text-destructive">
                            {getErrorMessage(
                                saveState.error,
                                "读取最新版本失败，请重试",
                            )}
                        </p>
                    ) : null}
                    {latest ? (
                        <p>
                            {versionAdvanced
                                ? `该申请单已被更新（当前状态：${requestStatusLabels[latest.status]}），请加载最新内容后再修改。`
                                : "刚才的保存尚未生效，可点击重新保存继续提交。"}
                        </p>
                    ) : null}
                    {versionAdvanced && latestEditable ? (
                        <Button
                            id="invoice-request-adopt-latest"
                            type="button"
                            variant="outline"
                            disabled={
                                saveState.isFetching ||
                                commands.update.isPending
                            }
                            onClick={adoptLatest}
                        >
                            采用最新版本并保留输入
                        </Button>
                    ) : null}
                    {versionAdvanced && !latestEditable ? (
                        <>
                            <p>
                                当前状态不能修改。可保留此页查看本地输入，或退出编辑；退出不表示原保存成功。
                            </p>
                            <Button
                                id="invoice-request-exit-save-recovery"
                                type="button"
                                variant="outline"
                                disabled={
                                    saveState.isFetching ||
                                    commands.update.isPending
                                }
                                onClick={onCancel}
                            >
                                退出编辑
                            </Button>
                        </>
                    ) : null}
                </div>
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
                {existing && !uncertain ? (
                    <LoadingButton
                        id="invoice-request-save"
                        type="button"
                        variant="outline"
                        loading={commands.update.isPending}
                        disabled={
                            commands.update.isPending ||
                            commands.submit.isPending ||
                            saveState.isFetching ||
                            (saveUncertain && versionAdvanced)
                        }
                        onClick={() => {
                            if (saveUncertain && pendingSave.current) {
                                void save(pendingSave.current)
                                return
                            }
                            intent.current = "save"
                            void form.handleSubmit()
                        }}
                    >
                        {saveUncertain ? "重新保存" : "保存修改"}
                    </LoadingButton>
                ) : null}
                {uncertain ? (
                    <LoadingButton
                        id="invoice-request-resolve"
                        type="button"
                        loading={commands.submit.isPending}
                        disabled={commands.submit.isPending}
                        onClick={() =>
                            pending.current && void send(pending.current)
                        }
                    >
                        核对提交结果
                    </LoadingButton>
                ) : (
                    <LoadingButton
                        id="invoice-request-submit"
                        type="submit"
                        loading={commands.submit.isPending}
                        disabled={
                            locked ||
                            !rework.valid ||
                            amounts.isError ||
                            amounts.isFetching ||
                            !amounts.data?.can_submit
                        }
                    >
                        提交审批
                    </LoadingButton>
                )}
            </div>
        </form>
    )
    if (presentation === "inline") return content
    return (
        <Dialog open onOpenChange={(open) => !open && !locked && onCancel()}>
            <DialogContent
                id="invoice-request-dialog"
                closeButtonId="invoice-request-dialog-close"
                showCloseButton={!locked}
                className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-3xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {existing ? "修改开票申请" : "申请开票"}
                    </DialogTitle>
                    <DialogDescription>
                        {existing
                            ? "保存修改保留当前状态；重新提交需说明依据，审批通过后由财务开票。"
                            : "提交申请，审批通过后由财务开票。"}
                    </DialogDescription>
                </DialogHeader>
                {content}
            </DialogContent>
        </Dialog>
    )
}

const InvoiceLinesFields = withForm({
    defaultValues: formDefaults,
    props: { locked: false },
    render: function InvoiceLinesFields({ form, locked }) {
        const nextLineKey = useRef(form.getFieldValue("lines").length)
        return (
            <form.AppField name="lines" mode="array">
                {(array) => (
                    <section
                        aria-labelledby="invoice-request-lines-heading"
                        className="flex min-w-0 flex-col gap-3 sm:col-span-2"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <div className="flex items-center gap-2">
                                <h3
                                    id="invoice-request-lines-heading"
                                    className="font-semibold"
                                >
                                    开票明细
                                </h3>
                                <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                                    {array.state.value.length} /{" "}
                                    {INVOICE_REQUEST_LINE_LIMIT} 行
                                </span>
                            </div>
                            <Button
                                id="invoice-request-add-line"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    locked ||
                                    array.state.value.length >=
                                        INVOICE_REQUEST_LINE_LIMIT
                                }
                                onClick={() => {
                                    nextLineKey.current += 1
                                    void array.pushValue({
                                        rowKey: `item-${nextLineKey.current}`,
                                        content: "",
                                        quantity: "",
                                        unit_price: "",
                                    })
                                }}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                添加明细
                            </Button>
                        </div>
                        <FieldDescription>
                            每行按数量 ×
                            含税单价舍入到分（四舍六入五成双）。明细合计须等于本次申请金额。
                        </FieldDescription>
                        {array.state.value.length >=
                            INVOICE_REQUEST_LINE_LIMIT && (
                            <FieldDescription>
                                {lineLimitMessage}
                            </FieldDescription>
                        )}
                        <div className="overflow-x-auto border-y border-border/70">
                            <Table className="min-w-[620px]">
                                <TableHeader>
                                    <TableRow>
                                        <TableHead className="w-10">
                                            #
                                        </TableHead>
                                        <TableHead>
                                            开票内容
                                            <RequiredMark />
                                        </TableHead>
                                        <TableHead className="w-28 text-right">
                                            数量
                                            <RequiredMark />
                                        </TableHead>
                                        <TableHead className="w-36 text-right">
                                            含税单价（元）
                                            <RequiredMark />
                                        </TableHead>
                                        <TableHead className="w-32 text-right">
                                            行金额
                                        </TableHead>
                                        <TableHead className="w-12">
                                            <span className="sr-only">
                                                删除
                                            </span>
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {array.state.value.map((line, index) => {
                                        const prefix = `invoice-request-line-${line.rowKey}`
                                        return (
                                            <TableRow
                                                key={line.rowKey}
                                                className="align-top"
                                            >
                                                <TableCell className="num pt-3 text-muted-foreground">
                                                    {index + 1}
                                                </TableCell>
                                                <TableCell className="min-w-48">
                                                    <form.AppField
                                                        name={`lines[${index}].content`}
                                                    >
                                                        {(field) => (
                                                            <field.TextField
                                                                id={`${prefix}-content`}
                                                                label="开票内容"
                                                                hideLabel
                                                                placeholder="如：货物名称或服务项目"
                                                                required
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </TableCell>
                                                <TableCell>
                                                    <form.AppField
                                                        name={`lines[${index}].quantity`}
                                                    >
                                                        {(field) => (
                                                            <field.TextField
                                                                id={`${prefix}-quantity`}
                                                                label="数量"
                                                                hideLabel
                                                                inputMode="decimal"
                                                                placeholder="0"
                                                                required
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </TableCell>
                                                <TableCell>
                                                    <form.AppField
                                                        name={`lines[${index}].unit_price`}
                                                    >
                                                        {(field) => (
                                                            <field.TextField
                                                                id={`${prefix}-unit-price`}
                                                                label="含税单价（元）"
                                                                hideLabel
                                                                inputMode="decimal"
                                                                placeholder="0.00"
                                                                required
                                                            />
                                                        )}
                                                    </form.AppField>
                                                </TableCell>
                                                <TableCell className="pt-3 text-right">
                                                    <form.Subscribe
                                                        selector={(state) => {
                                                            const value =
                                                                state.values
                                                                    .lines[
                                                                    index
                                                                ]
                                                            return value
                                                                ? invoiceLineAmount(
                                                                      value,
                                                                  )
                                                                : null
                                                        }}
                                                    >
                                                        {(lineAmount) =>
                                                            lineAmount ===
                                                            null ? (
                                                                <span className="text-muted-foreground">
                                                                    —
                                                                </span>
                                                            ) : (
                                                                <MoneyValue
                                                                    value={
                                                                        lineAmount
                                                                    }
                                                                    display="exact"
                                                                />
                                                            )
                                                        }
                                                    </form.Subscribe>
                                                </TableCell>
                                                <TableCell className="pt-2">
                                                    <Button
                                                        id={`${prefix}-remove`}
                                                        type="button"
                                                        size="icon-sm"
                                                        variant="ghost"
                                                        aria-label={`删除第 ${index + 1} 行`}
                                                        disabled={
                                                            locked ||
                                                            array.state.value
                                                                .length <= 1
                                                        }
                                                        onClick={() =>
                                                            void array.removeValue(
                                                                index,
                                                            )
                                                        }
                                                    >
                                                        <Trash2Icon
                                                            className="size-3.5"
                                                            aria-hidden="true"
                                                        />
                                                    </Button>
                                                </TableCell>
                                            </TableRow>
                                        )
                                    })}
                                </TableBody>
                                <TableFooter>
                                    <TableRow>
                                        <TableCell colSpan={4}>
                                            <form.Subscribe
                                                selector={(state) => {
                                                    const total =
                                                        invoiceLinesTotal(
                                                            state.values.lines,
                                                        )
                                                    return [
                                                        total,
                                                        invoiceTotalMismatch(
                                                            total,
                                                            state.values.amount,
                                                        ),
                                                    ] as const
                                                }}
                                            >
                                                {([total, mismatch]) => (
                                                    <div
                                                        role="status"
                                                        className="flex flex-wrap items-center gap-x-3 gap-y-1"
                                                    >
                                                        <span className="font-medium">
                                                            明细合计
                                                        </span>
                                                        {mismatch ? (
                                                            <>
                                                                <span className="text-xs font-normal text-destructive">
                                                                    {
                                                                        totalMismatchMessage
                                                                    }
                                                                </span>
                                                                {total !==
                                                                    null &&
                                                                !locked ? (
                                                                    <Button
                                                                        id="invoice-request-sync-amount"
                                                                        type="button"
                                                                        size="xs"
                                                                        variant="outline"
                                                                        onClick={() =>
                                                                            form.setFieldValue(
                                                                                "amount",
                                                                                total,
                                                                            )
                                                                        }
                                                                    >
                                                                        按明细合计回填申请金额
                                                                    </Button>
                                                                ) : null}
                                                            </>
                                                        ) : null}
                                                    </div>
                                                )}
                                            </form.Subscribe>
                                        </TableCell>
                                        <TableCell className="text-right font-semibold">
                                            <form.Subscribe
                                                selector={(state) =>
                                                    invoiceLinesTotal(
                                                        state.values.lines,
                                                    )
                                                }
                                            >
                                                {(total) =>
                                                    total === null ? (
                                                        "—"
                                                    ) : (
                                                        <MoneyValue
                                                            value={total}
                                                            display="exact"
                                                        />
                                                    )
                                                }
                                            </form.Subscribe>
                                        </TableCell>
                                        <TableCell />
                                    </TableRow>
                                </TableFooter>
                            </Table>
                        </div>
                    </section>
                )}
            </form.AppField>
        )
    },
})
