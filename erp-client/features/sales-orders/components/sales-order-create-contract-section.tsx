"use client"

import { useSelector } from "@tanstack/react-form"
import { UploadIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { SalesOrderCreateEvidenceField } from "@/features/sales-orders/components/sales-order-create-evidence-field"
import {
    ContractSearchCombobox,
    CustomerSearchCombobox,
    SettlementPartySearchCombobox,
} from "@/features/entity-selectors"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"

export type SalesOrderCreateContractSectionProps = {
    form: SalesOrderCreateFormApi
    customerLocked?: boolean
    initialCustomerId: string
    /** 合同详情查询进行中（显示"加载中…"）。 */
    contractFetching: boolean
    onContractChange: (contractId: string) => void
    onOrderBasisChange: (basis: "contract" | "evidence") => void
    onUploadClick: () => void
}

export function SalesOrderCreateContractSection({
    form,
    customerLocked = false,
    initialCustomerId,
    contractFetching,
    onContractChange,
    onOrderBasisChange,
    onUploadClick,
}: SalesOrderCreateContractSectionProps) {
    const basis = useSelector(form.store, (state) => state.values.orderBasis)
    const uploading = useSelector(
        form.store,
        (state) => state.values.evidenceUploadPending,
    )
    return (
        <div className="space-y-5">
            {!customerLocked ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border p-4">
                    <div>
                        <p className="text-sm font-medium">用合同自动填写</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            匹配客户与结算主体，预填付款、开票要求和税率
                        </p>
                    </div>
                    <Button
                        id="sales-orders-create-contract-upload"
                        type="button"
                        variant="outline"
                        disabled={uploading}
                        onClick={onUploadClick}
                    >
                        <UploadIcon aria-hidden="true" />
                        上传合同并预填
                    </Button>
                </div>
            ) : null}
            <form.AppField name="orderBasis">
                {(field) => (
                    <Field className="gap-2 sm:grid sm:grid-cols-[auto_1fr] sm:gap-x-6">
                        <div
                            id="sales-orders-create-basis-label"
                            className="flex items-center text-sm font-medium"
                        >
                            开单方式
                            <span className="ml-1 text-destructive">*</span>
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
                                { value: "contract", label: "关联合同" },
                                { value: "evidence", label: "上传材料开单" },
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
                            className="text-xs text-muted-foreground sm:col-start-2"
                            aria-live="polite"
                        >
                            {customerLocked
                                ? "开单依据沿用原单据；补录合同请到销售单详情操作。"
                                : uploading
                                  ? "凭证上传中，请完成后再切换开单依据。"
                                  : basis === "contract"
                                    ? "选择已归档合同，自动带入客户与结算条款"
                                    : "上传合同或其他凭证作为开单材料；合同资料可另行归档后关联"}
                        </p>
                    </Field>
                )}
            </form.AppField>
            {basis === "contract" ? (
                <form.AppField name="contractId">
                    {(field) => {
                        const isInvalid =
                            field.state.meta.isTouched &&
                            !field.state.meta.isValid
                        const errors = toFieldErrors(field.state.meta.errors)
                        return (
                            <Field
                                className="gap-2"
                                id="contractId"
                                tabIndex={-1}
                                data-invalid={isInvalid || undefined}
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <FieldLabel htmlFor="sales-orders-create-contract">
                                        有效合同
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                </div>
                                <ContractSearchCombobox
                                    id="sales-orders-create-contract"
                                    value={field.state.value || undefined}
                                    onValueChange={(id) => {
                                        const next = id ?? ""
                                        field.handleChange(next)
                                        onContractChange(next)
                                    }}
                                    customerId={initialCustomerId || undefined}
                                    selectableOnly
                                    placeholder="搜索合同编号或客户"
                                    emptyLabel="暂无可用合同，可上传合同预填或使用其他开单材料"
                                />
                                {isInvalid ? (
                                    <FieldError errors={errors} />
                                ) : null}
                            </Field>
                        )
                    }}
                </form.AppField>
            ) : (
                <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                    <form.AppField name="customerId">
                        {(field) => {
                            const invalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field data-invalid={invalid || undefined}>
                                    <FieldLabel htmlFor="sales-orders-create-customer">
                                        客户
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <CustomerSearchCombobox
                                        disabled={customerLocked}
                                        id="sales-orders-create-customer"
                                        value={field.state.value || undefined}
                                        onValueChange={(id) => {
                                            if (id !== field.state.value) {
                                                form.setFieldValue(
                                                    "settlementPartyId",
                                                    "",
                                                )
                                                form.setFieldValue(
                                                    "settlementEntity",
                                                    "",
                                                )
                                            }
                                            field.handleChange(id ?? "")
                                        }}
                                        onItemChange={(customer) =>
                                            form.setFieldValue(
                                                "customerName",
                                                customer?.legalName ?? "",
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
                    <form.AppField name="settlementPartyId">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="sales-orders-create-settlement">
                                    结算主体
                                    <span className="text-destructive">*</span>
                                </FieldLabel>
                                <SettlementPartySearchCombobox
                                    id="sales-orders-create-settlement"
                                    purpose="sales-order"
                                    disabled={customerLocked}
                                    value={field.state.value || undefined}
                                    onValueChange={(id) =>
                                        field.handleChange(id ?? "")
                                    }
                                    onItemChange={(item) =>
                                        form.setFieldValue(
                                            "settlementEntity",
                                            item?.displayName ?? "",
                                        )
                                    }
                                    placeholder="搜索并选择结算主体"
                                />
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    <SalesOrderCreateEvidenceField
                        form={form}
                        locked={customerLocked}
                    />
                </div>
            )}
            <form.Subscribe
                selector={(state) => ({
                    contractId: state.values.contractId,
                    contractRevisionLabel: state.values.contractRevisionLabel,
                    customerName: state.values.customerName,
                    settlementEntity: state.values.settlementEntity,
                })}
            >
                {({
                    contractId,
                    contractRevisionLabel,
                    customerName,
                    settlementEntity,
                }) =>
                    (basis === "evidence" || contractId) &&
                    (contractRevisionLabel ||
                        customerName ||
                        settlementEntity) ? (
                        <div className="flex min-w-0 flex-wrap items-center gap-x-6 gap-y-2 rounded-lg bg-muted/50 px-3 py-3 text-sm [&>span]:min-w-0 [&>span]:break-words">
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
                                    客户：
                                    <span className="text-foreground">
                                        {customerName}
                                    </span>
                                </span>
                            ) : null}
                            {settlementEntity ? (
                                <span className="text-muted-foreground">
                                    结算主体：
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
                    ) : null
                }
            </form.Subscribe>
        </div>
    )
}
