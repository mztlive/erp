"use client"

import { useSelector } from "@tanstack/react-form"
import { FileTextIcon, PaperclipIcon, UploadIcon } from "lucide-react"

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
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { cn } from "@/lib/utils"

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
        <div className="max-w-4xl space-y-4">
            <form.AppField name="orderBasis">
                {(field) => (
                    <Field className="gap-2">
                        <div
                            id="sales-orders-create-basis-label"
                            className="text-sm font-medium"
                        >
                            开单依据
                            <span className="ml-1 text-destructive">*</span>
                        </div>
                        <RadioGroup
                            id="sales-orders-create-basis"
                            aria-labelledby="sales-orders-create-basis-label"
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
                            className="grid gap-3 sm:grid-cols-2"
                        >
                            {[
                                {
                                    value: "contract",
                                    label: "关联合同",
                                    description:
                                        "选择已签合同，自动带入客户与付款条件",
                                    icon: FileTextIcon,
                                },
                                {
                                    value: "evidence",
                                    label: "凭证开单",
                                    description:
                                        "暂未签约，上传凭证先开单，合同后补",
                                    icon: PaperclipIcon,
                                },
                            ].map((option) => (
                                <label
                                    key={option.value}
                                    htmlFor={`sales-orders-create-basis-${option.value}`}
                                    className={cn(
                                        "flex min-w-0 cursor-pointer items-start gap-3 rounded-lg border p-3 transition-colors",
                                        field.state.value === option.value
                                            ? "border-foreground bg-background"
                                            : "border-grid hover:bg-muted/40",
                                        (customerLocked || uploading) &&
                                            "cursor-default",
                                    )}
                                >
                                    <RadioGroupItem
                                        id={`sales-orders-create-basis-${option.value}`}
                                        value={option.value}
                                        nativeButton
                                        render={
                                            <button
                                                type="button"
                                                aria-label={option.label}
                                            />
                                        }
                                        className="mt-0.5"
                                        aria-labelledby={`sales-orders-create-basis-${option.value}-label`}
                                        aria-describedby={`sales-orders-create-basis-${option.value}-description`}
                                    />
                                    <span className="min-w-0 space-y-1">
                                        <span
                                            id={`sales-orders-create-basis-${option.value}-label`}
                                            className="flex items-center gap-2 text-sm font-medium"
                                        >
                                            <option.icon
                                                className="size-4 text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            {option.label}
                                        </span>
                                        <span
                                            id={`sales-orders-create-basis-${option.value}-description`}
                                            className="block text-xs leading-relaxed text-muted-foreground"
                                        >
                                            {option.description}
                                        </span>
                                    </span>
                                </label>
                            ))}
                        </RadioGroup>
                        {customerLocked ? (
                            <p className="text-xs text-muted-foreground">
                                开单依据沿用原单据；补录合同请到销售单详情操作。
                            </p>
                        ) : uploading ? (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                凭证上传中，请完成后再切换开单依据。
                            </p>
                        ) : null}
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
                                            value={
                                                field.state.value || undefined
                                            }
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
                                            emptyLabel="暂无可用合同，请上传合同或切换为凭证开单"
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
                                {isInvalid ? (
                                    <FieldError errors={errors} />
                                ) : null}
                            </Field>
                        )
                    }}
                </form.AppField>
            ) : (
                <div className="grid gap-4 md:grid-cols-2">
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
                                        onValueChange={(id) =>
                                            field.handleChange(id ?? "")
                                        }
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
                    ) : null
                }
            </form.Subscribe>
        </div>
    )
}
