"use client"

import {
    MoneyValue,
    ValidationSummary,
    surfaceInsetClassName,
    surfacePanelClassName,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import { toFieldErrors } from "@/components/form"
import {
    PaymentRecipientFields,
    PaymentRecipientHeading,
    type PaymentRecipientRevealProps,
} from "@/features/supplier-payables/components/payment-recipient-card"
import type {
    InvoiceFormApi,
    PaymentFormApi,
} from "@/features/supplier-payables/lib/allocation-form-types"
import { BANK_RECEIPT_PENDING_REFERENCE } from "@/features/supplier-payables/lib/allocation-model"
import type {
    AllocationTrack,
    PaymentRecipient,
} from "@/features/supplier-payables/types"
import { cn } from "@/lib/utils"

export function AllocationAmountSummary({
    track,
    factAmount,
    allocatedAmount,
    unallocatedAmount,
}: {
    track: AllocationTrack
    factAmount: string
    allocatedAmount: string
    unallocatedAmount: string
}) {
    const items =
        track === "payment"
            ? [
                  ["付款总额", factAmount || "0"],
                  ["核销金额", allocatedAmount],
                  ["未核销金额", unallocatedAmount],
              ]
            : [
                  ["记录金额", factAmount || "0"],
                  ["拟分配", allocatedAmount],
                  ["拟未分配", unallocatedAmount],
              ]

    return (
        <DescriptionList
            columns="three"
            aria-label={track === "payment" ? "付款金额摘要" : "分配金额摘要"}
            className="gap-0 border-y border-border sm:grid-cols-3 xl:grid-cols-3"
        >
            {items.map(([label, value], index) => (
                <DescriptionItem
                    key={label}
                    className={cn(
                        "px-1 py-3 sm:px-5",
                        index > 0 &&
                            "border-t border-border sm:border-l sm:border-t-0",
                        index === 0 && "sm:pl-1",
                    )}
                >
                    <DescriptionTerm>{label}</DescriptionTerm>
                    <DescriptionDetails className="num text-lg font-semibold">
                        <MoneyValue value={value} taxBasis="gross" />
                    </DescriptionDetails>
                </DescriptionItem>
            ))}
        </DescriptionList>
    )
}

export type AllocationFactFormCardProps = {
    track: AllocationTrack
    existingInvoiceId?: string
    existingDocumentNo?: string
    existingUnallocated?: string
    paymentForm: Pick<
        PaymentFormApi,
        "AppField" | "handleSubmit" | "setFieldValue"
    >
    invoiceForm: Pick<InvoiceFormApi, "AppField" | "handleSubmit">
    mixedSources: boolean
    policyBlocksAuto: boolean
    issues: readonly ValidationIssue[]
    canSubmit: boolean
    isSubmitting: boolean
    draftHint?: string | null
    isSavingDraft?: boolean
    onSaveDraft?: () => void
    onSubmitClick: () => void
    paymentRecipient?: PaymentRecipient
    paymentRecipientReveal?: Omit<PaymentRecipientRevealProps, "recipient">
}

/** 本次付款/进项发票记录卡：收款信息、记录表单与提交校验。 */
export function AllocationFactFormCard({
    track,
    existingInvoiceId,
    existingDocumentNo,
    existingUnallocated,
    paymentForm,
    invoiceForm,
    mixedSources,
    policyBlocksAuto,
    issues,
    canSubmit,
    isSubmitting,
    draftHint,
    isSavingDraft = false,
    onSaveDraft,
    onSubmitClick,
    paymentRecipient,
    paymentRecipientReveal,
}: AllocationFactFormCardProps) {
    return (
        <section
            className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            aria-label={track === "payment" ? "付款信息" : "本次进项发票记录"}
        >
            <div className="border-b border-border px-4 py-3">
                <h2 className="text-sm font-semibold">
                    {track === "payment" ? "付款信息" : "本次进项发票记录"}
                </h2>
                <p className="mt-0.5 text-xs text-muted-foreground">
                    {track === "payment"
                        ? "银行回单为必填付款凭证，流水号仅用于辅助查找"
                        : "未分配余额以提交后的系统结果为准"}
                </p>
            </div>
            {paymentRecipient && paymentRecipientReveal ? (
                <div className="space-y-3 border-b border-border px-4 py-3">
                    <PaymentRecipientHeading />
                    <PaymentRecipientFields
                        key={`${paymentRecipientReveal.workItemId}:${paymentRecipient.bankAccountId}:${paymentRecipient.version}`}
                        payableAccountId={
                            paymentRecipientReveal.payableAccountId
                        }
                        workItemId={paymentRecipientReveal.workItemId}
                        expectedTaskVersion={
                            paymentRecipientReveal.expectedTaskVersion
                        }
                        recipient={paymentRecipient}
                    />
                </div>
            ) : null}
            <div className="space-y-4 p-4">
                {track === "payment" ? (
                    <form
                        className="grid gap-4 md:grid-cols-2"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void paymentForm.handleSubmit()
                        }}
                    >
                        <paymentForm.AppField
                            name="amount"
                            children={(field) => (
                                <field.TextField
                                    label="付款金额"
                                    inputMode="decimal"
                                />
                            )}
                        />
                        <paymentForm.AppField
                            name="paidAt"
                            children={(field) => (
                                <field.DateTimeField
                                    label="实际付款时间"
                                    clearable={false}
                                />
                            )}
                        />
                        <paymentForm.AppField
                            name="bankReference"
                            children={(field) => (
                                <field.TextField label="银行流水号（可选）" />
                            )}
                        />
                        <paymentForm.AppField
                            name="note"
                            children={(field) => (
                                <field.TextareaField
                                    label="备注（可选）"
                                    rows={1}
                                    textareaClassName="min-h-control"
                                />
                            )}
                        />
                        <div className="md:col-span-2">
                            <paymentForm.AppField
                                name="bankReceipt"
                                children={(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    const errors = toFieldErrors(
                                        field.state.meta.errors,
                                    )
                                    return (
                                        <Field
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel>
                                                银行回单图片
                                                <span className="text-destructive">
                                                    *
                                                </span>
                                            </FieldLabel>
                                            <FileUpload
                                                className="w-full"
                                                accept="image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp"
                                                multiple={false}
                                                density="compact"
                                                label="上传银行回单"
                                                description="支持 JPG、PNG、WebP，单张不超过 5 MB"
                                                previewSelectedImage
                                                onFilesSelected={(files) => {
                                                    field.handleChange(
                                                        files[0] ?? null,
                                                    )
                                                    paymentForm.setFieldValue(
                                                        "bankReceiptAssetId",
                                                        BANK_RECEIPT_PENDING_REFERENCE,
                                                    )
                                                    field.handleBlur()
                                                }}
                                                onPreviewRemove={() => {
                                                    field.handleChange(null)
                                                    paymentForm.setFieldValue(
                                                        "bankReceiptAssetId",
                                                        "",
                                                    )
                                                }}
                                            />
                                            <FieldDescription>
                                                提交后作为付款凭证长期留存；可在付款详情中受控预览。
                                            </FieldDescription>
                                            {invalid ? (
                                                <FieldError errors={errors} />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            />
                        </div>
                    </form>
                ) : (
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void invoiceForm.handleSubmit()
                        }}
                    >
                        {!existingInvoiceId ? (
                            <>
                                <invoiceForm.AppField
                                    name="invoiceCode"
                                    children={(field) => (
                                        <field.TextField label="发票代码" />
                                    )}
                                />
                                <invoiceForm.AppField
                                    name="invoiceNo"
                                    children={(field) => (
                                        <field.TextField label="发票号码" />
                                    )}
                                />
                                <invoiceForm.AppField
                                    name="invoiceDate"
                                    children={(field) => (
                                        <field.DateField label="开票日期" />
                                    )}
                                />
                                <invoiceForm.AppField
                                    name="grossAmount"
                                    children={(field) => (
                                        <field.TextField label="含税金额" />
                                    )}
                                />
                                <div className="grid grid-cols-2 gap-2">
                                    <invoiceForm.AppField
                                        name="netAmount"
                                        children={(field) => (
                                            <field.TextField label="不含税" />
                                        )}
                                    />
                                    <invoiceForm.AppField
                                        name="taxAmount"
                                        children={(field) => (
                                            <field.TextField label="税额" />
                                        )}
                                    />
                                </div>
                            </>
                        ) : (
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "space-y-1 px-3 py-3 text-sm",
                                )}
                            >
                                <div>原发票 {existingDocumentNo}</div>
                                <div className="flex justify-between">
                                    <span className="text-muted-foreground">
                                        未分配余额
                                    </span>
                                    <MoneyValue
                                        value={existingUnallocated}
                                        taxBasis="gross"
                                    />
                                </div>
                            </div>
                        )}
                    </form>
                )}

                {mixedSources ? (
                    <p className="text-xs text-muted-foreground">
                        已选择混合来源（采购单 + 结算单）。
                        {policyBlocksAuto
                            ? "策略不可用，已强制显式选择。"
                            : "混合来源已按系统优先级分配；提交时系统将重新校验。"}
                    </p>
                ) : null}

                <ValidationSummary issues={issues} />
            </div>
            <div className="flex flex-wrap items-center justify-between gap-3 border-t border-border px-4 py-3">
                <div className="min-w-0">
                    {draftHint ? (
                        <p className="text-xs text-muted-foreground">
                            {draftHint}（不形成业务记录）
                        </p>
                    ) : null}
                </div>
                <div className="flex items-center gap-2">
                    {onSaveDraft ? (
                        <Button
                            type="button"
                            variant="outline"
                            disabled={isSavingDraft || isSubmitting}
                            onClick={onSaveDraft}
                        >
                            {isSavingDraft ? "保存中…" : "保存草稿"}
                        </Button>
                    ) : null}
                    <Button
                        type="button"
                        disabled={!canSubmit || isSubmitting}
                        onClick={onSubmitClick}
                    >
                        {track === "payment"
                            ? "登记付款并核销"
                            : "确认登记并核销"}
                    </Button>
                </div>
            </div>
        </section>
    )
}
