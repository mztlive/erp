"use client"

import { useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { CheckCircle2Icon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { toFieldErrors } from "@/components/form"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import type { ContractCenterView } from "@/features/contracts/types"
import { ContractFilePreviewDialog } from "@/features/contracts/components/contract-file-preview-dialog"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"

export function SalesOrderCreateContractSummary({
    form,
    contract,
    imported,
    canChange,
    onChange,
}: {
    form: SalesOrderCreateFormApi
    contract: ContractCenterView
    imported: boolean
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
            ? "按合同约定"
            : PAYMENT_TERM_OPTIONS.find(
                  (item) => item.value === values.paymentTerms,
              )?.label || values.paymentTerms
    return (
        <div className="space-y-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-2">
                    <p className="flex items-center gap-2 text-sm text-muted-foreground">
                        <CheckCircle2Icon
                            className="size-4 text-success"
                            aria-hidden="true"
                        />
                        {imported ? "合同已归档，开单信息已带入" : "已关联合同"}
                    </p>
                    <form.AppField name="customerId">
                        {(field) => (
                            <div
                                id="sales-orders-create-customer"
                                tabIndex={-1}
                                className="outline-none"
                            >
                                <p className="text-lg font-semibold">
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
                                className="text-xs text-muted-foreground outline-none"
                            >
                                合同编号：{contract.contractNo} · v
                                {revision?.revisionNo ??
                                    contract.currentRevision.revisionNo}
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </div>
                        )}
                    </form.AppField>
                </div>
                <div className="flex flex-wrap gap-2">
                    {file ? (
                        <Button
                            id="sales-orders-create-contract-original"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => setPreviewOpen(true)}
                        >
                            查看合同原文
                        </Button>
                    ) : null}
                    {canChange ? (
                        <Button
                            id="sales-orders-create-contract-change"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onChange}
                        >
                            更换合同
                        </Button>
                    ) : null}
                </div>
            </div>
            <dl className="grid grid-cols-1 gap-3 rounded-lg bg-muted/40 p-3 text-sm sm:grid-cols-3">
                <form.AppField name="paymentTerms">
                    {(field) => (
                        <div
                            id="sales-orders-create-header-payment-terms"
                            tabIndex={-1}
                            className="outline-none"
                        >
                            <dt className="text-xs text-muted-foreground">
                                付款条件
                            </dt>
                            <dd className="mt-1">{payment || "未提供"}</dd>
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
                            className="outline-none"
                        >
                            <dt className="text-xs text-muted-foreground">
                                开票要求
                            </dt>
                            <dd className="mt-1">
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
                            className="outline-none"
                        >
                            <dt className="text-xs text-muted-foreground">
                                税率
                            </dt>
                            <dd className="mt-1">
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
