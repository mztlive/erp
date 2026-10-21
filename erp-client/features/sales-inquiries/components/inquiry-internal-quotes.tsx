"use client"

import * as React from "react"
import { useMutation, useQuery } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
} from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { Checkbox } from "@/components/ui/checkbox"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { FileUpload } from "@/components/ui/file-upload"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/hooks/queries"
import {
    downloadFileAsset,
    uploadEvidenceFileAsset,
} from "@/features/file-assets/api"
import { SUPPLIER_PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { apiGet } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDateInput } from "../lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import type { ProcurementInquiry } from "../types"
import type { InquiryTask } from "./inquiry-feedback-editor"

type QuoteTerms = {
    unit_cost_gross: string
    input_tax_rate: string
    minimum_order_quantity: string
    available_quantity: string
    address_limits: {
        allocation_id: string
        address_fingerprint: string
        quantity: string
    }[]
    expected_delivery_date: string
    valid_from: string
    valid_to: string
    payment_term_code: string
    allow_cumulative_batches: boolean
}
type InternalQuote = {
    id: string
    quote_id: string
    revision_no: number
    line_id: string
    supplier_id: string
    supplier_offering_id: string
    terms: QuoteTerms
    internal_note: string
    attachment_ids: string[]
}
type QuoteInput = Omit<InternalQuote, "id"> & {
    attachments: { id: string; file_name: string }[]
}
function blankQuote(): QuoteInput {
    return {
        quote_id: crypto.randomUUID(),
        revision_no: 1,
        line_id: "",
        supplier_id: "",
        supplier_offering_id: "",
        internal_note: "",
        attachment_ids: [],
        attachments: [],
        terms: {
            unit_cost_gross: "",
            input_tax_rate: "0.13",
            minimum_order_quantity: "1",
            available_quantity: "",
            address_limits: [],
            expected_delivery_date: "",
            valid_from: "",
            valid_to: "",
            payment_term_code: "",
            allow_cumulative_batches: false,
        },
    }
}

export function InquiryInternalQuotes({
    inquiry,
    task,
    userId,
    permissions,
    onSaved,
}: {
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    permissions: readonly string[]
    onSaved: () => void
}) {
    const [open, setOpen] = React.useState(false)
    const [initial, setInitial] = React.useState<QuoteInput | null>(null)
    const canRead = hasPermission(
        permissions,
        "procurement_inquiry:internal_quote",
    )
    const quotes = useQuery({
        queryKey: ["sales-inquiries", "internal-quotes", inquiry.id, task.id],
        queryFn: () =>
            apiGet<InternalQuote[]>(
                `${inquiryPath(true, inquiry.id)}/internal-quotes`,
                { task_id: task.id },
            ),
        enabled: canRead,
    })
    const offerings = useSupplierOfferingsForSkusQuery(
        canRead ? inquiry.draft.lines.map((line) => line.sku_id) : [],
    )
    const download = useMutation({
        mutationFn: ({ id, name }: { id: string; name: string }) =>
            downloadFileAsset(id, name),
    })
    const storageKey = inquiryStorageKey(
        userId,
        `${inquiry.id}:${task.id}:internal-quote`,
    )
    if (!canRead) return null
    return (
        <section className="space-y-4" aria-label="采购内部报价">
            <div className="flex flex-wrap items-center justify-between gap-3">
                <div>
                    <h2 className="text-lg font-medium">内部供应商报价</h2>
                    <p className="mt-1 text-xs text-muted-foreground">
                        仅当前责任采购可见。每次保存新增报价版本，不修改已发布的对销售结果。
                    </p>
                </div>
                <Button
                    id="procurement-inquiry-add-internal-quote"
                    type="button"
                    variant="outline"
                    onClick={() => {
                        setInitial(
                            readInquiryLocalValue<QuoteInput>(storageKey) ??
                                blankQuote(),
                        )
                        setOpen(true)
                    }}
                >
                    登记供应商报价
                </Button>
            </div>
            {quotes.isError ? (
                <BusinessFailureState
                    id="procurement-inquiry-quotes-retry"
                    title="内部报价加载失败"
                    error={quotes.error}
                    onRetry={() => void quotes.refetch()}
                />
            ) : quotes.isPending ? (
                <p role="status">正在读取内部报价…</p>
            ) : quotes.data.length === 0 ? (
                <p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">
                    尚未登记内部报价。
                </p>
            ) : (
                <div className="space-y-3">
                    {quotes.data.map((quote) => {
                        const offering = offerings.data?.find(
                            (item) => item.id === quote.supplier_offering_id,
                        )
                        const line = inquiry.draft.lines.find(
                            (item) => item.line_id === quote.line_id,
                        )
                        const prefix = `procurement-inquiry-quote-${toAutomationIdSegment(quote.id)}`
                        return (
                            <article
                                key={quote.id}
                                className="space-y-3 rounded-lg border p-4"
                            >
                                <div className="flex flex-wrap items-start justify-between gap-3">
                                    <div>
                                        <h3 className="font-medium">
                                            {line?.product_name ?? "商品报价"} ·{" "}
                                            {offering?.supplier_name ??
                                                "已登记供应商"}
                                        </h3>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            第 {quote.revision_no} 版 ·{" "}
                                            {quote.terms.valid_from} 至{" "}
                                            {quote.terms.valid_to} 前有效
                                        </p>
                                    </div>
                                    <Button
                                        id={`${prefix}-revise`}
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        onClick={() => {
                                            setInitial({
                                                ...quote,
                                                revision_no:
                                                    quote.revision_no + 1,
                                                attachments:
                                                    quote.attachment_ids.map(
                                                        (id, index) => ({
                                                            id,
                                                            file_name: `报价凭证 ${index + 1}`,
                                                        }),
                                                    ),
                                            })
                                            setOpen(true)
                                        }}
                                    >
                                        登记新版本
                                    </Button>
                                </div>
                                <p className="flex flex-wrap gap-4 text-sm">
                                    <span>
                                        原始采购单价{" "}
                                        <MoneyValue
                                            value={quote.terms.unit_cost_gross}
                                        />
                                    </span>
                                    <span>
                                        税率{" "}
                                        <RateValue
                                            value={multiplyFixed(
                                                quote.terms.input_tax_rate,
                                                "100",
                                                {
                                                    leftMaxScale: 6,
                                                    rightMaxScale: 0,
                                                    outputScale: 2,
                                                },
                                            )}
                                            precision={2}
                                        />
                                    </span>
                                    <span>
                                        可供{" "}
                                        <QuantityValue
                                            value={
                                                quote.terms.available_quantity
                                            }
                                            unit={line?.unit_code}
                                        />
                                    </span>
                                    <span>
                                        起订{" "}
                                        <QuantityValue
                                            value={
                                                quote.terms
                                                    .minimum_order_quantity
                                            }
                                            unit={line?.unit_code}
                                        />
                                    </span>
                                </p>
                                {quote.internal_note && (
                                    <p className="whitespace-pre-wrap text-sm text-muted-foreground">
                                        {quote.internal_note}
                                    </p>
                                )}
                                {quote.attachment_ids.length > 0 && (
                                    <div className="flex flex-wrap gap-2">
                                        {quote.attachment_ids.map(
                                            (id, index) => (
                                                <Button
                                                    key={id}
                                                    id={`${prefix}-attachment-${toAutomationIdSegment(id)}`}
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    onClick={() =>
                                                        download.mutate({
                                                            id,
                                                            name: `报价凭证-${index + 1}`,
                                                        })
                                                    }
                                                >
                                                    下载报价凭证 {index + 1}
                                                </Button>
                                            ),
                                        )}
                                    </div>
                                )}
                            </article>
                        )
                    })}
                </div>
            )}
            {initial && (
                <QuoteEditor
                    key={`${initial.quote_id}:${initial.revision_no}`}
                    initial={initial}
                    inquiry={inquiry}
                    task={task}
                    userId={userId}
                    storageKey={storageKey}
                    open={open}
                    onOpenChange={setOpen}
                    offerings={offerings.data ?? []}
                    offeringError={offerings.error}
                    onSaved={() => {
                        void quotes.refetch()
                        onSaved()
                    }}
                />
            )}
        </section>
    )
}

function QuoteEditor({
    initial,
    inquiry,
    task,
    userId,
    storageKey,
    open,
    onOpenChange,
    offerings,
    offeringError,
    onSaved,
}: {
    initial: QuoteInput
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    storageKey: string
    open: boolean
    onOpenChange: (value: boolean) => void
    offerings: readonly {
        id: string
        sku_id: string
        supplier_id: string
        supplier_name?: string | null
        supplier_sku_code: string
        status: string
    }[]
    offeringError: unknown
    onSaved: () => void
}) {
    const command = useInquiryCommand(userId, inquiry.id)
    const [error, setError] = React.useState<string | null>(null)
    const upload = useMutation({
        mutationFn: (file: File) => uploadEvidenceFileAsset(file),
    })
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    writeInquiryLocalValue(storageKey, formApi.state.values)
                } catch {
                    setError("浏览器无法保留输入，请先登记报价后离开。")
                }
            },
        },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                if (!value.line_id || !value.supplier_offering_id)
                    throw new Error("请选择商品和当前可用的供应商供给。")
                const quote = {
                    quote_id: value.quote_id,
                    revision_no: value.revision_no,
                    line_id: value.line_id,
                    supplier_id: value.supplier_id,
                    supplier_offering_id: value.supplier_offering_id,
                    terms: value.terms,
                    internal_note: value.internal_note,
                    attachment_ids: value.attachments.map((item) => item.id),
                }
                await command.run(
                    `${inquiryPath(true, inquiry.id)}/internal-quotes`,
                    {
                        expected_version: inquiry.version,
                        task_id: task.id,
                        task_version: task.version,
                        quote,
                    },
                )
                localStorage.removeItem(storageKey)
                onOpenChange(false)
                onSaved()
            } catch (failure) {
                setError(
                    getErrorMessage(
                        failure,
                        "登记内部报价失败，请保留输入并重试。",
                    ),
                )
            }
        },
    })
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="procurement-inquiry-quote-editor-close"
                className="max-h-[85svh] overflow-y-auto sm:max-w-3xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        登记供应商报价 · 第 {initial.revision_no} 版
                    </DialogTitle>
                    <DialogDescription>
                        填写实际采购条件和内部依据；此处内容不会出现在销售反馈、客户模板或销售来源资料中。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset disabled={command.isPending} className="contents">
                        {error && (
                            <Alert variant="destructive">
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        )}
                        {offeringError != null && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    当前权限下无法读取供应商供给，请联系有供给维护资格的采购核对。
                                </AlertDescription>
                            </Alert>
                        )}
                        <div className="grid gap-4 md:grid-cols-2">
                            <form.Field
                                name="line_id"
                                children={(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor="procurement-inquiry-quote-line">
                                            商品需求
                                        </Label>
                                        <OptionCombobox
                                            id="procurement-inquiry-quote-line"
                                            aria-label="商品需求"
                                            allowClear={false}
                                            value={field.state.value}
                                            options={inquiry.draft.lines.map(
                                                (line) => ({
                                                    value: line.line_id,
                                                    label: `${line.product_name} · ${line.sku_code}`,
                                                }),
                                            )}
                                            onValueChange={(value) => {
                                                field.handleChange(value ?? "")
                                                form.setFieldValue(
                                                    "supplier_offering_id",
                                                    "",
                                                )
                                                form.setFieldValue(
                                                    "supplier_id",
                                                    "",
                                                )
                                                const line =
                                                    inquiry.draft.lines.find(
                                                        (item) =>
                                                            item.line_id ===
                                                            value,
                                                    )
                                                if (line) {
                                                    form.setFieldValue(
                                                        "terms.available_quantity",
                                                        line.quantity,
                                                    )
                                                    form.setFieldValue(
                                                        "terms.expected_delivery_date",
                                                        inquiryDateInput(
                                                            line.delivery_due_at,
                                                        ),
                                                    )
                                                    form.setFieldValue(
                                                        "terms.address_limits",
                                                        line.allocations.map(
                                                            (item) => ({
                                                                allocation_id:
                                                                    item.allocation_id,
                                                                address_fingerprint:
                                                                    "",
                                                                quantity:
                                                                    item.quantity,
                                                            }),
                                                        ),
                                                    )
                                                }
                                            }}
                                        />
                                    </div>
                                )}
                            />
                            <form.Subscribe
                                selector={(state) => state.values.line_id}
                                children={(lineId) => (
                                    <form.Field
                                        name="supplier_offering_id"
                                        children={(field) => {
                                            const line =
                                                inquiry.draft.lines.find(
                                                    (item) =>
                                                        item.line_id === lineId,
                                                )
                                            return (
                                                <div className="space-y-2">
                                                    <Label htmlFor="procurement-inquiry-quote-offering">
                                                        供应商供给
                                                    </Label>
                                                    <OptionCombobox
                                                        id="procurement-inquiry-quote-offering"
                                                        aria-label="供应商供给"
                                                        allowClear={false}
                                                        value={
                                                            field.state.value
                                                        }
                                                        options={offerings
                                                            .filter(
                                                                (offering) =>
                                                                    offering.sku_id ===
                                                                        line?.sku_id &&
                                                                    offering.status ===
                                                                        "ACTIVE",
                                                            )
                                                            .map(
                                                                (offering) => ({
                                                                    value: offering.id,
                                                                    label: `${offering.supplier_name ?? "供应商"} · ${offering.supplier_sku_code}`,
                                                                }),
                                                            )}
                                                        onValueChange={(id) => {
                                                            field.handleChange(
                                                                id ?? "",
                                                            )
                                                            form.setFieldValue(
                                                                "supplier_id",
                                                                offerings.find(
                                                                    (item) =>
                                                                        item.id ===
                                                                        id,
                                                                )
                                                                    ?.supplier_id ??
                                                                    "",
                                                            )
                                                        }}
                                                    />
                                                </div>
                                            )
                                        }}
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.unit_cost_gross"
                                children={(field) => (
                                    <field.TextField
                                        id="procurement-inquiry-quote-cost"
                                        label="原始含税采购单价"
                                        inputMode="decimal"
                                        money
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.input_tax_rate"
                                children={(field) => (
                                    <field.TextField
                                        id="procurement-inquiry-quote-tax"
                                        label="进项税率（0.13 表示 13%）"
                                        inputMode="decimal"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.minimum_order_quantity"
                                children={(field) => (
                                    <field.TextField
                                        id="procurement-inquiry-quote-minimum"
                                        label="起订量"
                                        inputMode="decimal"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.available_quantity"
                                children={(field) => (
                                    <field.TextField
                                        id="procurement-inquiry-quote-available"
                                        label="可供数量"
                                        inputMode="decimal"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.expected_delivery_date"
                                children={(field) => (
                                    <field.DateField
                                        id="procurement-inquiry-quote-delivery"
                                        label="承诺交付日期"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.valid_from"
                                children={(field) => (
                                    <field.DateField
                                        id="procurement-inquiry-quote-valid-from"
                                        label="报价生效日期"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.valid_to"
                                children={(field) => (
                                    <field.DateField
                                        id="procurement-inquiry-quote-valid-to"
                                        label="报价失效日期"
                                        description="失效日期当天起不可采用"
                                    />
                                )}
                            />
                            <form.AppField
                                name="terms.payment_term_code"
                                children={(field) => (
                                    <field.SelectField
                                        id="procurement-inquiry-quote-payment"
                                        label="付款条件"
                                        options={[
                                            ...SUPPLIER_PAYMENT_TERM_OPTIONS,
                                        ]}
                                    />
                                )}
                            />
                            <form.Field
                                name="terms.allow_cumulative_batches"
                                children={(field) => (
                                    <label
                                        htmlFor="procurement-inquiry-quote-cumulative"
                                        className="flex items-center gap-2 text-sm"
                                    >
                                        <Checkbox
                                            id="procurement-inquiry-quote-cumulative"
                                            checked={field.state.value}
                                            onCheckedChange={(value) =>
                                                field.handleChange(
                                                    value === true,
                                                )
                                            }
                                        />
                                        条款允许累计分批满足起订量
                                    </label>
                                )}
                            />
                        </div>
                        <form.Subscribe
                            selector={(state) => ({
                                line_id: state.values.line_id,
                                addresses: state.values.terms.address_limits,
                            })}
                            children={({ line_id, addresses }) => (
                                <div className="space-y-2">
                                    <h3 className="text-sm font-medium">
                                        适用配送范围
                                    </h3>
                                    {inquiry.draft.lines
                                        .find(
                                            (line) => line.line_id === line_id,
                                        )
                                        ?.allocations.map((allocation) => {
                                            const index = addresses.findIndex(
                                                (item) =>
                                                    item.allocation_id ===
                                                    allocation.allocation_id,
                                            )
                                            const prefix = `procurement-inquiry-quote-address-${toAutomationIdSegment(allocation.allocation_id)}`
                                            return (
                                                <div
                                                    key={
                                                        allocation.allocation_id
                                                    }
                                                    className="flex flex-wrap items-center gap-3 text-sm"
                                                >
                                                    <label
                                                        htmlFor={`${prefix}-included`}
                                                        className="flex min-w-56 flex-1 items-center gap-2"
                                                    >
                                                        <Checkbox
                                                            id={`${prefix}-included`}
                                                            checked={index >= 0}
                                                            onCheckedChange={(
                                                                checked,
                                                            ) => {
                                                                const next = [
                                                                    ...addresses,
                                                                ]
                                                                if (checked)
                                                                    next.push({
                                                                        allocation_id:
                                                                            allocation.allocation_id,
                                                                        address_fingerprint:
                                                                            "",
                                                                        quantity:
                                                                            allocation.quantity,
                                                                    })
                                                                else if (
                                                                    index >= 0
                                                                )
                                                                    next.splice(
                                                                        index,
                                                                        1,
                                                                    )
                                                                form.setFieldValue(
                                                                    "terms.address_limits",
                                                                    next,
                                                                )
                                                            }}
                                                        />
                                                        {allocation.recipient} ·{" "}
                                                        {allocation.region_name}{" "}
                                                        {allocation.address}
                                                    </label>
                                                    {index >= 0 && (
                                                        <form.Field
                                                            name={`terms.address_limits[${index}].quantity`}
                                                            children={(
                                                                field,
                                                            ) => (
                                                                <Input
                                                                    id={`${prefix}-quantity`}
                                                                    className="w-28"
                                                                    aria-label={`${allocation.recipient}可供数量`}
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    inputMode="decimal"
                                                                    onChange={(
                                                                        event,
                                                                    ) =>
                                                                        field.handleChange(
                                                                            event
                                                                                .target
                                                                                .value,
                                                                        )
                                                                    }
                                                                />
                                                            )}
                                                        />
                                                    )}
                                                </div>
                                            )
                                        })}
                                </div>
                            )}
                        />
                        <form.AppField
                            name="internal_note"
                            children={(field) => (
                                <field.TextareaField
                                    id="procurement-inquiry-quote-note"
                                    label="内部议价说明"
                                />
                            )}
                        />
                        <FileUpload
                            idPrefix="procurement-inquiry-quote-file"
                            accept="application/pdf,image/jpeg,image/png,image/webp"
                            maxFileSize={5 * 1024 * 1024}
                            label="上传内部报价凭证"
                            disabled={upload.isPending}
                            onFilesSelected={(files) => {
                                const file = files[0]
                                if (file)
                                    void upload
                                        .mutateAsync(file)
                                        .then((asset) =>
                                            form.pushFieldValue(
                                                "attachments",
                                                asset,
                                            ),
                                        )
                                        .catch((failure) =>
                                            setError(getErrorMessage(failure)),
                                        )
                            }}
                        />
                        <form.Field
                            name="attachments"
                            mode="array"
                            children={(field) => (
                                <div className="space-y-1">
                                    {field.state.value.map((asset, index) => (
                                        <div
                                            key={asset.id}
                                            className="flex items-center justify-between gap-3 text-sm"
                                        >
                                            <span>{asset.file_name}</span>
                                            <Button
                                                id={`procurement-inquiry-quote-file-${toAutomationIdSegment(asset.id)}-remove`}
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                onClick={() =>
                                                    field.removeValue(index)
                                                }
                                            >
                                                移除凭证
                                            </Button>
                                        </div>
                                    ))}
                                </div>
                            )}
                        />
                        <DialogFooter>
                            <Button
                                id="procurement-inquiry-quote-cancel"
                                type="button"
                                variant="outline"
                                onClick={() => onOpenChange(false)}
                            >
                                取消
                            </Button>
                            <Button
                                id="procurement-inquiry-quote-save"
                                type="submit"
                                disabled={
                                    command.isPending ||
                                    command.unresolved ||
                                    !command.ready ||
                                    upload.isPending
                                }
                            >
                                {command.isPending
                                    ? "正在登记…"
                                    : "登记报价版本"}
                            </Button>
                        </DialogFooter>
                    </fieldset>
                </form>
            </DialogContent>
        </Dialog>
    )
}
