"use client"

import { SupplierTaxRateField } from "../supplier-tax-rate-field"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldGroup, FieldLegend, FieldSet } from "@/components/ui/field"
import { useReviseSupplierOfferingMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    errorMessage,
    idempotencyKey,
    percentageFromRate,
    rateFromPercentage,
    reviseSchema,
    apiReviseSchema,
} from "@/features/supplier-offerings/lib/offering-forms"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { OFFERING_STATUS_LABELS } from "@/features/supplier-offerings/types"

export function ReviseOfferingDialog({
    offering,
    onOpenChange,
}: {
    offering: SupplierOfferingView
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useReviseSupplierOfferingMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: {
            dropshipPrice: offering.dropship_supply_price_gross ?? "",
            bulkPrice: offering.bulk_supply_price_gross ?? "",
            minimumQuantity:
                offering.bulk_minimum_order_quantity ??
                (offering.source_type === "API" ? "" : "1"),
            inputTaxPercentage: percentageFromRate(offering.input_tax_rate),
            validFrom: offering.valid_from ?? "",
            validTo: offering.valid_to ?? "",
            status: offering.status,
            changeReason: "调整供给条款",
        },
        validators: {
            onSubmit:
                offering.source_type === "API" ? apiReviseSchema : reviseSchema,
        },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            try {
                await mutation.mutateAsync({
                    offeringId: offering.id,
                    expected_revision_no: offering.current_revision_no ?? 1,
                    terms: {
                        dropship_supply_price_gross: value.dropshipPrice.trim(),
                        bulk_supply_price_gross: value.bulkPrice.trim() || null,
                        input_tax_rate: rateFromPercentage(
                            value.inputTaxPercentage,
                        ),
                        bulk_minimum_order_quantity:
                            value.minimumQuantity.trim() || null,
                        product_capabilities: offering.product_capabilities,
                        valid_from: value.validFrom,
                        valid_to: value.validTo || null,
                    },
                    status: value.status,
                    change_reason: value.changeReason.trim(),
                    idempotency_key: idempotencyKey("revise-supplier-offering"),
                })
                onOpenChange(false)
            } catch (error) {
                setSubmitError(errorMessage(error, "供给条款保存失败"))
            }
        },
    })

    return (
        <Dialog open onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="supplier-offerings-dialog-revise-close"
                className="w-[calc(100vw-2rem)] gap-4 p-5 sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>修订供给条款</DialogTitle>
                    <DialogDescription>
                        {offering.sku_name ??
                            offering.sku_no ??
                            offering.sku_id}{" "}
                        · {offering.supplier_name ?? offering.supplier_sku_code}
                    </DialogDescription>
                </DialogHeader>
                {submitError ? (
                    <Alert variant="destructive">
                        <AlertTitle>保存失败</AlertTitle>
                        <AlertDescription>{submitError}</AlertDescription>
                    </Alert>
                ) : null}
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <FieldGroup className="gap-4">
                        <div className="grid gap-4 lg:grid-cols-2">
                            <FieldSet className="gap-4 rounded-lg border p-4">
                                <FieldLegend variant="label">
                                    价格与条款
                                </FieldLegend>
                                {offering.source_type === "API" ? (
                                    <p className="text-sm text-muted-foreground">
                                        供应商采购价格及集采条件由接口同步，不可修改。税率和有效期等内部补充信息可维护。
                                    </p>
                                ) : null}
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="dropshipPrice">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-dropship-price"
                                                disabled={
                                                    offering.source_type ===
                                                    "API"
                                                }
                                                label="一件代发采购价（含税）"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="bulkPrice">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-bulk-price"
                                                disabled={
                                                    offering.source_type ===
                                                    "API"
                                                }
                                                label="集采采购价（含税）"
                                                required={
                                                    offering.source_type !==
                                                    "API"
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="minimumQuantity">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-minimum-quantity"
                                                disabled={
                                                    offering.source_type ===
                                                    "API"
                                                }
                                                label="集采起订量"
                                                required={
                                                    offering.source_type !==
                                                    "API"
                                                }
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="inputTaxPercentage">
                                        {(field) => (
                                            <SupplierTaxRateField
                                                id="supplier-offerings-dialog-revise-input-tax-percentage"
                                                supplierId={
                                                    offering.supplier_id
                                                }
                                                value={field.state.value}
                                                onChange={field.handleChange}
                                                onBlur={field.handleBlur}
                                                errors={field.state.meta.errors}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </FieldSet>

                            <FieldSet className="gap-4 rounded-lg border p-4">
                                <FieldLegend variant="label">
                                    有效期与状态
                                </FieldLegend>
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="validFrom">
                                        {(field) => (
                                            <field.DateField
                                                id="supplier-offerings-dialog-revise-valid-from"
                                                label="生效日期"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="validTo">
                                        {(field) => (
                                            <field.DateField
                                                id="supplier-offerings-dialog-revise-valid-to"
                                                label="失效日期"
                                                description="不填为长期有效"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="status">
                                        {(field) => (
                                            <field.SelectField
                                                id="supplier-offerings-dialog-revise-status"
                                                label="供给关系状态"
                                                required
                                                options={Object.entries(
                                                    OFFERING_STATUS_LABELS,
                                                ).map(([value, label]) => ({
                                                    value,
                                                    label,
                                                }))}
                                                allowClear={false}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="changeReason">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-change-reason"
                                                label="变更原因"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </FieldSet>
                        </div>
                    </FieldGroup>
                    <DialogFooter className="border-t pt-4">
                        <DialogClose
                            render={
                                <Button
                                    id="supplier-offerings-dialog-revise-cancel"
                                    type="button"
                                    variant="outline"
                                    disabled={mutation.isPending}
                                />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-offerings-dialog-revise-submit"
                                label="保存新版本"
                                loading={mutation.isPending}
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
