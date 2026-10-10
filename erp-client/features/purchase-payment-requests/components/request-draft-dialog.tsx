"use client"

import { useMutation } from "@tanstack/react-query"
import { useRouter } from "next/navigation"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Field, FieldLabel } from "@/components/ui/field"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import {
    usePaymentCandidatesQuery,
    useSaveRequestMutation,
} from "../hooks/queries"
import type { PaymentCandidate, PaymentRequest } from "../types"

const groupKey = (candidate: PaymentCandidate) =>
    `${candidate.supplier_id}:${candidate.our_company_id}`
function positive(value: string) {
    try {
        return compareDecimal(value, "0", 2) > 0
    } catch {
        return false
    }
}
function totalOf(values: Record<string, string>) {
    return sumFixed(Object.values(values).filter(positive), {
        maxScale: 2,
        outputScale: 2,
    })
}

export function RequestDraftDialog({
    open,
    onOpenChange,
    purchaseOrderId,
    request,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    purchaseOrderId?: string
    request?: PaymentRequest
}) {
    const query = usePaymentCandidatesQuery(open)
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                id="ppr-create-dialog"
                closeButtonId="ppr-create-dialog-close"
                className="max-h-[calc(100dvh-2rem)] overflow-y-auto sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {request ? "修改付款申请" : "新建采购付款申请"}
                    </DialogTitle>
                    <DialogDescription>
                        同一供应商、我方付款主体与收款账户分别申请。请按每张采购单明确填写本次金额；保存草稿后核对并提交财务审核。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p className="py-8 text-sm text-muted-foreground">
                        正在读取可申请采购单…
                    </p>
                ) : query.isError ? (
                    <div role="alert" className="space-y-3">
                        <p>{getErrorMessage(query.error, "采购单读取失败")}</p>
                        <Button
                            id="ppr-create-retry"
                            variant="outline"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    </div>
                ) : (
                    <RequestDraftForm
                        candidates={query.data}
                        purchaseOrderId={purchaseOrderId}
                        request={request}
                        onClose={() => onOpenChange(false)}
                    />
                )}
            </DialogContent>
        </Dialog>
    )
}

function RequestDraftForm({
    candidates,
    purchaseOrderId,
    request,
    onClose,
}: {
    candidates: PaymentCandidate[]
    purchaseOrderId?: string
    request?: PaymentRequest
    onClose: () => void
}) {
    const router = useRouter()
    const save = useSaveRequestMutation()
    const upload = useMutation({
        mutationFn: (files: File[]) =>
            Promise.all(files.map(uploadEvidenceFileAsset)),
    })
    const initialCandidate =
        candidates.find(
            (candidate) => candidate.purchase_order_id === purchaseOrderId,
        ) ??
        candidates.find(
            (candidate) =>
                candidate.supplier_id === request?.supplier_id &&
                candidate.our_company_id === request?.our_company_id,
        )
    const groups = Array.from(
        new Map(
            candidates.map((candidate) => [groupKey(candidate), candidate]),
        ).values(),
    )
    const schema = z
        .object({
            group: z.string().min(1, "请选择供应商和付款主体"),
            account: z.string().min(1, "请选择有效收款账户"),
            date: z.string().min(1, "请填写希望付款日"),
            description: z
                .string()
                .trim()
                .min(1, "请填写申请说明")
                .max(2000, "申请说明最多 2000 字"),
            amounts: z.record(z.string(), z.string()),
            files: z.array(z.custom<File>()),
        })
        .superRefine((value, context) => {
            const selected = candidates.filter(
                (candidate) => groupKey(candidate) === value.group,
            )
            let count = 0
            for (const candidate of selected) {
                const amount =
                    value.amounts[candidate.payable_account_id]?.trim()
                if (!amount) continue
                count += 1
                if (!positive(amount))
                    context.addIssue({
                        code: "custom",
                        path: ["amounts", candidate.payable_account_id],
                        message: "请填写大于零且最多两位小数的金额",
                    })
                else if (
                    compareDecimal(amount, candidate.available_amount, 2) > 0
                )
                    context.addIssue({
                        code: "custom",
                        path: ["amounts", candidate.payable_account_id],
                        message: "申请金额超过当前可申请余额",
                    })
            }
            if (count === 0)
                context.addIssue({
                    code: "custom",
                    path: ["amounts"],
                    message: "请至少填写一张采购单的本次申请金额",
                })
        })
    const form = useAppForm({
        defaultValues: {
            group: initialCandidate ? groupKey(initialCandidate) : "",
            account:
                request?.payment_recipient.bank_account_id ??
                initialCandidate?.payment_recipient?.bank_account_id ??
                "",
            date: request?.desired_payment_date ?? "",
            description: request?.description ?? "",
            amounts: Object.fromEntries(
                candidates.map((candidate) => [
                    candidate.payable_account_id,
                    request?.lines.find(
                        (line) =>
                            line.payable_account_id ===
                            candidate.payable_account_id,
                    )?.requested_amount ?? "",
                ]),
            ) as Record<string, string>,
            files: [] as File[],
        },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            const group = candidates.filter(
                (candidate) => groupKey(candidate) === value.group,
            )
            const candidate = group[0]
            const account = candidate?.bank_accounts.find(
                (item) => item.bank_account_id === value.account,
            )
            if (!candidate || !account) return
            const attachments = await upload.mutateAsync(value.files)
            const result = await save.mutateAsync({
                id: request?.id,
                version: request?.version,
                data: {
                    supplier_id: candidate.supplier_id,
                    bank_account_id: account.bank_account_id,
                    bank_account_version: account.version,
                    desired_payment_date: value.date,
                    description: value.description.trim(),
                    attachment_ids: [
                        ...(request?.attachment_ids ?? []),
                        ...attachments.map((file) => file.id),
                    ],
                    lines: group
                        .filter((item) =>
                            positive(
                                value.amounts[item.payable_account_id] ?? "",
                            ),
                        )
                        .map((item) => ({
                            payable_account_id: item.payable_account_id,
                            amount: value.amounts[item.payable_account_id],
                        })),
                },
            })
            onClose()
            router.push(
                `/procurement/payment-requests/${encodeURIComponent(result.id)}`,
            )
        },
    })
    return (
        <form
            id="ppr-create-form"
            className="space-y-6"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            <form.AppField name="group">
                {(field) => (
                    <field.SelectField
                        id="ppr-create-group"
                        label="供应商 · 我方付款主体"
                        required
                        options={groups.map((candidate) => ({
                            value: groupKey(candidate),
                            label: `${candidate.supplier_name} · ${candidate.our_company_name}`,
                        }))}
                        onValueChange={(value) => {
                            form.setFieldValue(
                                "amounts",
                                Object.fromEntries(
                                    candidates.map((candidate) => [
                                        candidate.payable_account_id,
                                        "",
                                    ]),
                                ),
                            )
                            form.setFieldValue(
                                "account",
                                candidates.find(
                                    (candidate) =>
                                        groupKey(candidate) === value,
                                )?.payment_recipient?.bank_account_id ?? "",
                            )
                        }}
                    />
                )}
            </form.AppField>
            {groups.length > 1 && (
                <p className="text-xs text-muted-foreground">
                    跨供应商或跨我方付款主体的采购单须分别申请，主体来自采购引用的销售生效版本。
                </p>
            )}
            <form.Subscribe selector={(state) => state.values.group}>
                {(selectedGroup) => {
                    const group = candidates.filter(
                        (candidate) => groupKey(candidate) === selectedGroup,
                    )
                    return (
                        <>
                            <form.AppField name="account">
                                {(field) => (
                                    <field.SelectField
                                        id="ppr-create-account"
                                        label="供应商收款账户"
                                        required
                                        options={(
                                            group[0]?.bank_accounts ?? []
                                        ).map((account) => ({
                                            value: account.bank_account_id,
                                            label: `${account.account_name} · ${account.bank_name} · ${account.account_number_masked}`,
                                        }))}
                                        description="提交后冻结选定账户；账户变更须关闭未付部分并重新申请。"
                                    />
                                )}
                            </form.AppField>
                            <section
                                aria-label="逐采购单申请金额"
                                className="space-y-3"
                            >
                                <h3 className="text-sm font-medium">
                                    逐采购单申请金额
                                </h3>
                                {group.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        请选择供应商和付款主体。
                                    </p>
                                ) : (
                                    group.map((candidate) => (
                                        <div
                                            key={candidate.payable_account_id}
                                            className="grid min-w-0 gap-3 border-b py-4 sm:grid-cols-[minmax(0,1fr)_12rem]"
                                        >
                                            <div className="space-y-2">
                                                <p className="num font-medium">
                                                    {
                                                        candidate.purchase_order_no
                                                    }
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    {
                                                        candidate.payment_term_name
                                                    }
                                                </p>
                                                <dl className="flex flex-wrap gap-x-5 gap-y-2 text-xs">
                                                    <div>
                                                        采购总额{" "}
                                                        <MoneyValue
                                                            value={
                                                                candidate.total_amount
                                                            }
                                                        />
                                                    </div>
                                                    <div>
                                                        有效已付{" "}
                                                        <MoneyValue
                                                            value={
                                                                candidate.paid_amount
                                                            }
                                                        />
                                                    </div>
                                                    <div>
                                                        其他申请占用{" "}
                                                        <MoneyValue
                                                            value={
                                                                candidate.reserved_amount
                                                            }
                                                        />
                                                    </div>
                                                    <div>
                                                        可申请{" "}
                                                        <MoneyValue
                                                            value={
                                                                candidate.available_amount
                                                            }
                                                        />
                                                    </div>
                                                </dl>
                                            </div>
                                            <form.AppField
                                                name={`amounts.${candidate.payable_account_id}`}
                                            >
                                                {(field) => (
                                                    <field.TextField
                                                        id={`ppr-create-line-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                                        label="本次申请金额"
                                                        inputMode="decimal"
                                                        placeholder="填写本次金额"
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                    ))
                                )}
                            </section>
                        </>
                    )
                }}
            </form.Subscribe>
            <div className="grid gap-5 sm:grid-cols-2">
                <form.AppField name="date">
                    {(field) => (
                        <field.TextField
                            id="ppr-create-desired-payment-date"
                            type="date"
                            label="希望付款日"
                            required
                        />
                    )}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values.amounts}>
                    {(amounts) => (
                        <div className="self-end rounded-lg bg-muted p-3">
                            <p className="text-xs text-muted-foreground">
                                本次申请合计
                            </p>
                            <MoneyValue value={totalOf(amounts)} size="hero" />
                        </div>
                    )}
                </form.Subscribe>
            </div>
            <form.AppField name="description">
                {(field) => (
                    <field.TextareaField
                        id="ppr-create-description"
                        label="申请说明"
                        required
                        placeholder="说明对账范围、付款安排及业务依据"
                    />
                )}
            </form.AppField>
            <form.Field name="files">
                {(field) => (
                    <Field>
                        <FieldLabel htmlFor="ppr-create-attachments">
                            对账材料及其他依据（可选）
                        </FieldLabel>
                        <Input
                            id="ppr-create-attachments"
                            type="file"
                            multiple
                            accept="application/pdf,image/jpeg,image/png,image/webp"
                            onChange={(event) =>
                                field.handleChange(
                                    Array.from(event.target.files ?? []),
                                )
                            }
                        />
                        <p className="text-xs text-muted-foreground">
                            支持 PDF、JPG、PNG、WebP，每份不超过 5 MB。
                        </p>
                    </Field>
                )}
            </form.Field>
            {(save.isError || upload.isError) && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(
                        save.error ?? upload.error,
                        "保存失败，请重试",
                    )}
                </p>
            )}
            <form.Subscribe selector={(state) => state.errors}>
                {(errors) =>
                    errors.length ? (
                        <p role="alert" className="text-sm text-destructive">
                            请核对必填项和逐单申请金额。
                        </p>
                    ) : null
                }
            </form.Subscribe>
            <DialogFooter>
                <Button
                    id="ppr-create-cancel"
                    variant="outline"
                    type="button"
                    onClick={onClose}
                >
                    取消
                </Button>
                <Button
                    id="ppr-create-submit"
                    type="submit"
                    disabled={save.isPending || upload.isPending}
                >
                    {save.isPending || upload.isPending
                        ? "正在保存…"
                        : "保存申请草稿"}
                </Button>
            </DialogFooter>
        </form>
    )
}
