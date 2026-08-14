"use client"

import {
    MoneyValue,
    ValidationSummary,
    type ValidationIssue,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
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
    onClose: () => void
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
    onClose,
    onSubmitClick,
}: AllocationFactFormCardProps) {
    return (
        <Card>
            <CardHeader className="border-b border-border">
                <CardTitle className="text-base">
                    {track === "payment"
                        ? "本次付款记录"
                        : "本次进项发票记录"}
                </CardTitle>
                <CardDescription>
                    未分配余额以提交后的系统结果为准
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
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
                            <div className="rounded-lg bg-muted/50 p-3 text-sm">
                                <div>原付款 {existingDocumentNo}</div>
                                <div className="mt-1 flex justify-between">
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
                            <div className="rounded-lg bg-muted/50 p-3 text-sm">
                                <div>原发票 {existingDocumentNo}</div>
                                <div className="mt-1 flex justify-between">
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

                <Separator />

                <dl className="grid grid-cols-3 gap-2 text-sm">
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            记录金额
                        </dt>
                        <dd>
                            <MoneyValue value={factAmount || "0"} />
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            拟分配
                        </dt>
                        <dd>
                            <MoneyValue value={allocatedHint} />
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            拟未分配
                        </dt>
                        <dd>
                            <MoneyValue value={unallocatedHint} />
                        </dd>
                    </div>
                </dl>

                {mixedSources ? (
                    <p className="text-xs text-muted-foreground">
                        已选择混合来源（采购单 + 结算单）。
                        {policyBlocksAuto
                            ? "策略不可用，已强制显式选择。"
                            : "混合来源已按系统优先级分配；提交时系统将重新校验。"}
                    </p>
                ) : null}

                <ValidationSummary issues={issues} />
            </CardContent>
            <CardFooter className="justify-end gap-2 border-t border-border">
                <Button type="button" variant="outline" onClick={onClose}>
                    取消
                </Button>
                <Button
                    type="button"
                    disabled={!canSubmit || isSubmitting}
                    onClick={onSubmitClick}
                >
                    确认登记并核销
                </Button>
            </CardFooter>
        </Card>
    )
}
