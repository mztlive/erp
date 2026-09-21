"use client"

import { UploadIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import { validateSalesOrderContractId } from "@/features/sales-orders/lib/sales-order-create-model"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { ContractSearchCombobox } from "@/features/entity-selectors"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"

export type SalesOrderCreateContractSectionProps = {
    form: SalesOrderCreateFormApi
    initialCustomerId: string
    /** 合同详情查询进行中（显示"加载中…"）。 */
    contractFetching: boolean
    onContractChange: (contractId: string) => void
    onUploadClick: () => void
}

export function SalesOrderCreateContractSection({
    form,
    initialCustomerId,
    contractFetching,
    onContractChange,
    onUploadClick,
}: SalesOrderCreateContractSectionProps) {
    return (
        <div className="space-y-2">
            <form.AppField
                name="contractId"
                validators={{
                    onSubmit: ({ value }) =>
                        validateSalesOrderContractId(value),
                }}
            >
                {(field) => {
                    const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    const errors = toFieldErrors(field.state.meta.errors)
                    return (
                        <Field
                            className="max-w-4xl gap-2"
                            id="contractId"
                            tabIndex={-1}
                            data-invalid={isInvalid || undefined}
                        >
                            <FieldLabel htmlFor="sales-orders-create-contract">
                                有效合同
                                <span className="text-destructive">*</span>
                            </FieldLabel>
                            <div className="flex flex-wrap items-start gap-2">
                                <div className="min-w-0 flex-1 basis-48">
                                    <ContractSearchCombobox
                                        id="sales-orders-create-contract"
                                        value={field.state.value || undefined}
                                        onValueChange={(id) => {
                                            const next = id ?? ""
                                            field.handleChange(next)
                                            onContractChange(next)
                                        }}
                                        customerId={
                                            initialCustomerId || undefined
                                        }
                                        selectableOnly
                                        placeholder="搜索合同编号或客户"
                                        emptyLabel="暂无可用合同，请上传合同 PDF"
                                    />
                                </div>
                                <Button
                                    id="sales-orders-create-contract-upload"
                                    type="button"
                                    variant="outline"
                                    className="shrink-0"
                                    aria-label="上传合同 PDF"
                                    title="上传合同 PDF"
                                    onClick={() => onUploadClick()}
                                >
                                    <UploadIcon aria-hidden="true" />
                                    上传合同
                                </Button>
                            </div>
                            {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                    )
                }}
            </form.AppField>
            <form.Subscribe
                selector={(state) => ({
                    contractRevisionLabel: state.values.contractRevisionLabel,
                    customerName: state.values.customerName,
                    settlementEntity: state.values.settlementEntity,
                })}
            >
                {({ contractRevisionLabel, customerName, settlementEntity }) =>
                    contractRevisionLabel ||
                    customerName ||
                    settlementEntity ? (
                        <div className="flex min-w-0 flex-wrap items-center gap-x-5 gap-y-1 text-xs [&>span]:min-w-0 [&>span]:break-words">
                            {contractRevisionLabel ? (
                                <Badge
                                    variant="outline"
                                    className="font-normal"
                                >
                                    {contractRevisionLabel}
                                </Badge>
                            ) : null}
                            {customerName ? (
                                <span className="text-muted-foreground">
                                    客户{" "}
                                    <span className="text-foreground">
                                        {customerName}
                                    </span>
                                </span>
                            ) : null}
                            {settlementEntity ? (
                                <span className="text-muted-foreground">
                                    结算主体{" "}
                                    <span className="text-foreground">
                                        {settlementEntity}
                                    </span>
                                </span>
                            ) : null}
                            {contractFetching ? (
                                <span className="text-muted-foreground">
                                    加载中…
                                </span>
                            ) : null}
                        </div>
                    ) : (
                        <p className="text-xs leading-relaxed text-muted-foreground">
                            选择合同后带入客户、结算主体和付款条件，付款条件可按本单调整。
                        </p>
                    )
                }
            </form.Subscribe>
        </div>
    )
}
