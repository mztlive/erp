"use client"

import { useSelector } from "@tanstack/react-form"
import { UploadIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
} from "@/features/entity-selectors"
import { SalesOrderCreateEvidenceField } from "@/features/sales-orders/components/sales-order-create-evidence-field"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"

export type SalesOrderCreateContractSectionProps = {
    form: SalesOrderCreateFormApi
    sourceCustomerLocked?: boolean
    customerLocked?: boolean
    onContractChange: (contractId: string) => void
    onOrderBasisChange: (basis: "contract" | "evidence") => void
    onUploadClick: () => void
}

export function SalesOrderCreateContractSection({
    form,
    sourceCustomerLocked = false,
    customerLocked = false,
    onContractChange,
    onOrderBasisChange,
    onUploadClick,
}: SalesOrderCreateContractSectionProps) {
    const basis = useSelector(form.store, (state) => state.values.orderBasis)
    const contractId = useSelector(
        form.store,
        (state) => state.values.contractId,
    )
    const customerId = useSelector(
        form.store,
        (state) => state.values.customerId,
    )
    const uploading = useSelector(
        form.store,
        (state) => state.values.evidenceUploadPending,
    )
    const customerFromContract = basis === "contract" && Boolean(contractId)

    const customerField = (
        <form.AppField name="customerId">
            {(field) => {
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                return (
                    <Field data-invalid={invalid || undefined}>
                        <FieldLabel htmlFor="sales-orders-create-customer">
                            客户
                            <RequiredMark />
                        </FieldLabel>
                        <CustomerSearchCombobox
                            disabled={
                                customerLocked ||
                                sourceCustomerLocked ||
                                customerFromContract
                            }
                            id="sales-orders-create-customer"
                            value={field.state.value || undefined}
                            onValueChange={(id) => field.handleChange(id ?? "")}
                            onItemChange={(customer) =>
                                form.setFieldValue(
                                    "customerName",
                                    customer?.legalName ?? "",
                                )
                            }
                            placeholder="搜索客户编号或名称"
                        />
                        {customerFromContract ? (
                            <p className="text-xs text-muted-foreground">
                                客户由合同确定；需更换客户请先清除合同。
                            </p>
                        ) : null}
                        {invalid ? (
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        ) : null}
                    </Field>
                )
            }}
        </form.AppField>
    )

    if (basis === "evidence") {
        return (
            <div className="space-y-4">
                <div className="grid grid-cols-1 gap-x-5 gap-y-4 sm:grid-cols-2">
                    {customerField}
                </div>
                <div className="max-w-2xl">
                    <SalesOrderCreateEvidenceField
                        form={form}
                        locked={customerLocked}
                    />
                </div>
            </div>
        )
    }

    return (
        <div className="space-y-4">
            {!customerLocked ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border/70 bg-surface-sunken/70 px-3.5 py-2.5">
                    <p className="text-xs text-muted-foreground">
                        尚未归档合同？可直接上传合同
                        PDF，核对归档后自动带入客户及条款。
                    </p>
                    <Button
                        id="sales-orders-create-contract-upload"
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        disabled={uploading}
                        onClick={onUploadClick}
                    >
                        <UploadIcon
                            data-icon="inline-start"
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        上传并导入合同
                    </Button>
                </div>
            ) : null}

            <form.AppField name="orderBasis">
                {(field) => (
                    <Field className="gap-2">
                        <div
                            id="sales-orders-create-basis-label"
                            className="flex items-center text-xs font-medium text-foreground/75"
                        >
                            开单方式
                            <RequiredMark />
                        </div>
                        <RadioGroup
                            id="sales-orders-create-basis"
                            aria-labelledby="sales-orders-create-basis-label"
                            aria-describedby="sales-orders-create-basis-description"
                            value={field.state.value}
                            disabled={customerLocked || uploading}
                            onValueChange={(value) => {
                                if (
                                    value === "contract" ||
                                    value === "evidence"
                                ) {
                                    onOrderBasisChange(value)
                                    field.handleBlur()
                                }
                            }}
                            className="flex w-fit gap-0"
                        >
                            {[
                                {
                                    value: "contract",
                                    label: "关联合同开单",
                                },
                                {
                                    value: "evidence",
                                    label: "先开单，后补合同",
                                },
                            ].map((option) => (
                                <RadioGroupItem
                                    key={option.value}
                                    id={`sales-orders-create-basis-${option.value}`}
                                    value={option.value}
                                    variant="segment"
                                    nativeButton
                                    render={
                                        <button
                                            type="button"
                                            aria-label={option.label}
                                        />
                                    }
                                >
                                    {option.label}
                                </RadioGroupItem>
                            ))}
                        </RadioGroup>
                        <p
                            id="sales-orders-create-basis-description"
                            className="text-xs text-muted-foreground"
                            aria-live="polite"
                        >
                            {customerLocked
                                ? "开单依据沿用原单据；补录合同请到销售单详情操作。"
                                : uploading
                                  ? "凭证上传中，请完成后再切换开单依据。"
                                  : "选择已归档合同后自动带入客户及条款。"}
                        </p>
                    </Field>
                )}
            </form.AppField>

            <div className="grid grid-cols-1 gap-x-5 gap-y-4 sm:grid-cols-2">
                {customerField}
                <form.AppField name="contractId">
                    {(field) => {
                        const invalid =
                            field.state.meta.isTouched &&
                            !field.state.meta.isValid
                        return (
                            <Field
                                id="contractId"
                                tabIndex={-1}
                                data-invalid={invalid || undefined}
                            >
                                <FieldLabel htmlFor="sales-orders-create-contract">
                                    有效合同
                                    <RequiredMark />
                                </FieldLabel>
                                <ContractSearchCombobox
                                    id="sales-orders-create-contract"
                                    value={field.state.value || undefined}
                                    disabled={customerLocked}
                                    onValueChange={(id) => {
                                        const next = id ?? ""
                                        field.handleChange(next)
                                        onContractChange(next)
                                    }}
                                    customerId={customerId || undefined}
                                    selectableOnly
                                    placeholder={
                                        customerId
                                            ? "搜索该客户的合同"
                                            : "搜索合同编号或客户"
                                    }
                                    emptyLabel="暂无可用合同，可在本页上传并导入"
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
            </div>
        </div>
    )
}
