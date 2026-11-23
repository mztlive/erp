"use client"

import { useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { FileTextIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { ContractFilePreviewDialog } from "@/features/contracts/components/contract-file-preview-dialog"
import type { ContractCenterView } from "@/features/contracts/types"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"

export function SalesOrderCreateContractSummary({
    form,
    contract,
    canChange,
    onChange,
}: {
    form: SalesOrderCreateFormApi
    contract: ContractCenterView
    canChange: boolean
    onChange: () => void
}) {
    const values = useSelector(form.store, (state) => state.values)
    const [previewOpen, setPreviewOpen] = useState(false)
    const revision = contract.revisionTimeline.find(
        (item) => item.revisionId === values.requestedContractRevisionId,
    )
    const file = revision
        ? contract.attachments.find(
              (item) => item.revisionNo === revision.revisionNo,
          )
        : undefined
    // 续编可能引用旧版本，不能用当前合同条款替换原单已经保存的值。
    const payment =
        values.paymentTerms === "CONTRACT"
            ? values.requestedContractRevisionId ===
              contract.currentRevision.revisionId
                ? contract.currentRevision.paymentTermSnapshot.label
                : "按合同约定"
            : PAYMENT_TERM_OPTIONS.find(
                  (item) => item.value === values.paymentTerms,
              )?.label || values.paymentTerms
    const revisionNo =
        revision?.revisionNo ?? contract.currentRevision.revisionNo

    return (
        <div className="space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-x-4 gap-y-2.5">
                <div className="min-w-0 space-y-1">
                    <form.AppField name="customerId">
                        {(field) => (
                            <div
                                id="sales-orders-create-customer"
                                tabIndex={-1}
                                className="outline-none"
                            >
                                <p className="break-words font-semibold text-foreground">
                                    {values.customerName}
                                </p>
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </div>
                        )}
                    </form.AppField>
                    <form.AppField name="contractId">
                        {(field) => (
                            <div
                                id="contractId"
                                tabIndex={-1}
                                className="outline-none"
                            >
                                <p className="num break-words text-xs text-muted-foreground">
                                    合同编号：{contract.contractNo} · v
                                    {revisionNo}
                                </p>
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </div>
                        )}
                    </form.AppField>
                </div>
                {file || canChange ? (
                    <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                        {file ? (
                            <Button
                                id="sales-orders-create-contract-original"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() => setPreviewOpen(true)}
                            >
                                <FileTextIcon
                                    data-icon="inline-start"
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                查看合同原文
                            </Button>
                        ) : null}
                        {canChange ? (
                            <Button
                                id="sales-orders-create-contract-change"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                onClick={onChange}
                            >
                                更换合同
                            </Button>
                        ) : null}
                    </div>
                ) : null}
            </div>

            <dl className="grid grid-cols-1 gap-x-8 gap-y-3 border-t border-border/50 pt-3.5 sm:grid-cols-3">
                <form.AppField name="paymentTerms">
                    {(field) => (
                        <div
                            id="sales-orders-create-header-payment-terms"
                            tabIndex={-1}
                            className="min-w-0 space-y-1 outline-none"
                        >
                            <dt className="text-xs font-medium text-muted-foreground">
                                付款条件
                            </dt>
                            <dd className="break-words text-foreground">
                                {payment || "未提供"}
                            </dd>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </div>
                    )}
                </form.AppField>
                <form.AppField name="invoiceType">
                    {(field) => (
                        <div
                            id="sales-orders-create-header-invoice-type"
                            tabIndex={-1}
                            className="min-w-0 space-y-1 outline-none"
                        >
                            <dt className="text-xs font-medium text-muted-foreground">
                                开票要求
                            </dt>
                            <dd className="break-words text-foreground">
                                {field.state.value || "未提供"}
                            </dd>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </div>
                    )}
                </form.AppField>
                <form.AppField name="taxRatePercent">
                    {(field) => (
                        <div
                            id="sales-orders-create-header-tax-rate"
                            tabIndex={-1}
                            className="min-w-0 space-y-1 outline-none"
                        >
                            <dt className="text-xs font-medium text-muted-foreground">
                                税率
                            </dt>
                            <dd className="num text-foreground">
                                {field.state.value
                                    ? `${field.state.value}%`
                                    : "未提供"}
                            </dd>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </div>
                    )}
                </form.AppField>
            </dl>

            {file ? (
                <ContractFilePreviewDialog
                    contractId={contract.contractId}
                    fileId={file.id}
                    fileName={file.name}
                    open={previewOpen}
                    onOpenChange={setPreviewOpen}
                />
            ) : null}
        </div>
    )
}
