"use client"

import { UploadIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { SalesOrderCreateEvidenceField } from "@/features/sales-orders/components/sales-order-create-evidence-field"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
} from "@/features/entity-selectors"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"

export type SalesOrderCreateContractSectionProps = {
    form: SalesOrderCreateFormApi
    customerLocked?: boolean
    initialCustomerId: string
    /** 合同详情查询进行中（显示"加载中…"）。 */
    contractFetching: boolean
    onContractChange: (contractId: string) => void
    onUploadClick: () => void
}

export function SalesOrderCreateContractSection({
    form,
    customerLocked = false,
    initialCustomerId,
    contractFetching,
    onContractChange,
    onUploadClick,
}: SalesOrderCreateContractSectionProps) {
    return (
        <div className="space-y-2">
            <form.AppField name="contractId">
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
                                        emptyLabel="暂无可用合同，可上传合同或先上传开单凭证"
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
            <form.Subscribe selector={(state) => state.values.contractId}>
                {(contractId) =>
                    !contractId ? (
                        <div className="grid max-w-4xl gap-4 pt-2 md:grid-cols-2">
                            <form.AppField name="customerId">
                                {(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    return (
                                        <Field
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel htmlFor="sales-orders-create-customer">
                                                客户
                                                <span className="text-destructive">
                                                    *
                                                </span>
                                            </FieldLabel>
                                            <CustomerSearchCombobox
                                                disabled={customerLocked}
                                                id="sales-orders-create-customer"
                                                value={
                                                    field.state.value ||
                                                    undefined
                                                }
                                                onValueChange={(id) =>
                                                    field.handleChange(id ?? "")
                                                }
                                                onItemChange={(customer) =>
                                                    form.setFieldValue(
                                                        "customerName",
                                                        customer?.legalName ??
                                                            "",
                                                    )
                                                }
                                                placeholder="搜索客户编号或名称"
                                            />
                                            {invalid ? (
                                                <FieldError
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            </form.AppField>
                            <SalesOrderCreateEvidenceField
                                form={form}
                                locked={customerLocked}
                            />
                        </div>
                    ) : null
                }
            </form.Subscribe>
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
                            选择合同后带入客户、结算主体和付款条件；无合同时选择客户并上传开单凭证。
                        </p>
                    )
                }
            </form.Subscribe>
        </div>
    )
}
