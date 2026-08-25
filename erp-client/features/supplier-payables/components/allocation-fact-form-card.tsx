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
import { cn } from "@/lib/utils"
import type {
    InvoiceFormApi,
    PaymentFormApi,
} from "@/features/supplier-payables/lib/allocation-form-types"
import type { AllocationTrack } from "@/features/supplier-payables/types"

export type AllocationFactFormCardProps = {
    track: AllocationTrack
    existingPaymentId?: string
    existingInvoiceId?: string
    existingDocumentNo?: string
    existingUnallocated?: string
    paymentForm: Pick<PaymentFormApi, "AppField" | "handleSubmit">
    invoiceForm: Pick<InvoiceFormApi, "AppField" | "handleSubmit">
    factAmount: string
    allocatedHint: string
    unallocatedHint: string
    mixedSources: boolean
    policyBlocksAuto: boolean
    issues: readonly ValidationIssue[]
    canSubmit: boolean
    isSubmitting: boolean
    onSubmitClick: () => void
}

/** 本次付款/进项发票记录卡：记录表单、分配汇总与提交校验。 */
export function AllocationFactFormCard({
    track,
    existingPaymentId,
    existingInvoiceId,
    existingDocumentNo,
    existingUnallocated,
    paymentForm,
    invoiceForm,
    factAmount,
    allocatedHint,
    unallocatedHint,
    mixedSources,
    policyBlocksAuto,
    issues,
    canSubmit,
    isSubmitting,
    onSubmitClick,
}: AllocationFactFormCardProps) {
    return (
        <section
            className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            aria-label={
                track === "payment" ? "本次付款记录" : "本次进项发票记录"
            }
        >
            <div className="border-b border-border px-4 py-3">
                <h2 className="text-sm font-semibold">
                    {track === "payment" ? "本次付款记录" : "本次进项发票记录"}
                </h2>
                <p className="mt-0.5 text-xs text-muted-foreground">
                    未分配余额以提交后的系统结果为准
                </p>
            </div>
            <div className="space-y-4 p-4">
                {track === "payment" ? (
                    <form
                        className="space-y-3"
                        onSubmit={(e) => {
                            e.preventDefault()
                            void paymentForm.handleSubmit()
                        }}
                    >
                        {!existingPaymentId ? (
                            <>
                                <paymentForm.AppField
                                    name="paidAt"
                                    children={(field) => (
                                        <field.DateTimeField label="实际付款时间" />
                                    )}
                                />
                                <paymentForm.AppField
                                    name="amount"
                                    children={(field) => (
                                        <field.TextField label="付款金额（含税）" />
                                    )}
                                />
                                <paymentForm.AppField
                                    name="bankReference"
                                    children={(field) => (
                                        <field.TextField label="银行流水引用" />
                                    )}
                                />
                                <paymentForm.AppField
                                    name="note"
                                    children={(field) => (
                                        <field.TextareaField label="备注（可选）" />
                                    )}
                                />
                            </>
                        ) : (
                            <div
                                className={cn(
                                    surfaceInsetClassName,
                                    "space-y-1 px-3 py-3 text-sm",
                                )}
                            >
                                <div>原付款 {existingDocumentNo}</div>
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

                <DescriptionList
                    columns="three"
                    aria-label="本次分配摘要"
                    className="border-t border-border pt-4"
                >
                    <DescriptionItem>
                        <DescriptionTerm>记录金额</DescriptionTerm>
                        <DescriptionDetails className="num text-base font-medium">
                            <MoneyValue
                                value={factAmount || "0"}
                                taxBasis="gross"
                            />
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>拟分配</DescriptionTerm>
                        <DescriptionDetails className="num text-base font-medium">
                            <MoneyValue
                                value={allocatedHint}
                                taxBasis="gross"
                            />
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>拟未分配</DescriptionTerm>
                        <DescriptionDetails className="num text-base font-medium">
                            <MoneyValue
                                value={unallocatedHint}
                                taxBasis="gross"
                            />
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>

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
            <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
                <Button
                    type="button"
                    disabled={!canSubmit || isSubmitting}
                    onClick={onSubmitClick}
                >
                    确认登记并核销
                </Button>
            </div>
        </section>
    )
}
