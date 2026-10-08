"use client"

import { useSelector } from "@tanstack/react-form"
import { UploadIcon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import type { SettlementPartyComboboxItem } from "@/components/business/entity-comboboxes"
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
    settlementSnapshot?: SettlementPartyComboboxItem
    contractFetching: boolean
    onContractChange: (contractId: string) => void
    onOrderBasisChange: (basis: "contract" | "evidence") => void
    onUploadClick: () => void
}

export function SalesOrderCreateContractSection({
    form,
    customerLocked = false,
    settlementSnapshot,
    contractFetching,
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
    return (
        <div className="space-y-5">
            {!customerLocked ? (
                <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border p-4">
                    <div>
                        <p className="text-sm font-medium">导入合同并开单</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                            核对后归档，自动选择客户与合同，带入付款、开票要求和税率
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
                        上传并导入合同
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
                                { value: "contract", label: "关联合同开单" },
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
                            className="text-xs text-muted-foreground sm:col-start-2"
                            aria-live="polite"
                        >
                            {customerLocked
                                ? "开单依据沿用原单据；补录合同请到销售单详情操作。"
                                : uploading
                                  ? "凭证上传中，请完成后再切换开单依据。"
                                  : basis === "contract"
                                    ? "选择已归档合同，或在本页导入签署合同后自动带入开单信息。"
                                    : "客户已确定、合同尚未完成盖章时，可凭业务材料先开单，签署完成后补录同一客户的合同。"}
                        </p>
                    </Field>
                )}
            </form.AppField>
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
                                    <span className="text-destructive">*</span>
                                </FieldLabel>
                                <CustomerSearchCombobox
                                    disabled={
                                        customerLocked || customerFromContract
                                    }
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
                                {customerFromContract ? (
                                    <p className="text-xs text-muted-foreground">
                                        客户由合同确定；需要更换客户时，请先清除所选合同。
                                    </p>
                                ) : null}
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
                                selectedSnapshot={settlementSnapshot}
                                disabled={
                                    customerFromContract && contractFetching
                                }
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
                            <p className="text-xs text-muted-foreground">
                                {basis === "contract"
                                    ? "默认使用合同的我方签约主体，可按本次销售安排调整。"
                                    : "选择本次销售的结算主体，补合同时保留此选择。"}
                            </p>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </Field>
                    )}
                </form.AppField>
                {basis === "contract" ? (
                    <form.AppField name="contractId">
                        {(field) => {
                            const invalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field
                                    className="gap-2 md:col-span-2"
                                    id="contractId"
                                    tabIndex={-1}
                                    data-invalid={invalid || undefined}
                                >
                                    <FieldLabel htmlFor="sales-orders-create-contract">
                                        有效合同
                                        <span className="text-destructive">
                                            *
                                        </span>
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
                ) : (
                    <SalesOrderCreateEvidenceField
                        form={form}
                        locked={customerLocked}
                    />
                )}
            </div>
            <form.Subscribe
                selector={(state) => state.values.contractRevisionLabel}
            >
                {(label) =>
                    label ? (
                        <div className="flex flex-wrap items-center gap-3 rounded-lg bg-muted/50 px-3 py-3 text-sm">
                            <Badge variant="outline" className="font-normal">
                                {label}
                            </Badge>
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
