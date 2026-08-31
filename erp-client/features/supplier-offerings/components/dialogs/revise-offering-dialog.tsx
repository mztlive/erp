"use client"

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
    splitValues,
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
            minimumQuantity: offering.bulk_minimum_order_quantity ?? "1",
            inputTaxPercentage: percentageFromRate(offering.input_tax_rate),
            supplyRegionText: offering.supply_region.join("，"),
            validFrom: offering.valid_from ?? "",
            validTo: offering.valid_to ?? "",
            dropshipExpress: offering.dropship_express ?? "",
            freightAmount: offering.freight_amount ?? "",
            serviceFeeAmount: offering.service_fee_amount ?? "",
            status: offering.status,
            changeReason: "调整供给条款",
        },
        validators: { onSubmit: reviseSchema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            try {
                await mutation.mutateAsync({
                    offeringId: offering.id,
                    expected_revision_no: offering.current_revision_no ?? 1,
                    terms: {
                        dropship_supply_price_gross: value.dropshipPrice.trim(),
                        bulk_supply_price_gross: value.bulkPrice.trim(),
                        input_tax_rate: rateFromPercentage(
                            value.inputTaxPercentage,
                        ),
                        bulk_minimum_order_quantity:
                            value.minimumQuantity.trim(),
                        supply_region: splitValues(value.supplyRegionText),
                        product_capabilities: offering.product_capabilities,
                        valid_from: value.validFrom,
                        valid_to: value.validTo || null,
                        dropship_express: value.dropshipExpress.trim() || null,
                        freight_amount: value.freightAmount.trim() || null,
                        service_fee_amount:
                            value.serviceFeeAmount.trim() || null,
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
                className="w-[calc(100vw-2rem)] gap-4 p-5 sm:max-w-5xl"
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
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="dropshipPrice">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-dropship-price"
                                                label="一件代发供给价（含税）"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="bulkPrice">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-bulk-price"
                                                label="集采供给价（含税）"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="minimumQuantity">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-minimum-quantity"
                                                label="集采起订量"
                                                required
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="inputTaxPercentage">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-input-tax-percentage"
                                                label="进项税率（%）"
                                                required
                                                description="例如 13 表示 13%"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </FieldSet>

                            <FieldSet className="gap-4 rounded-lg border p-4">
                                <FieldLegend variant="label">
                                    供应范围与时效
                                </FieldLegend>
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="supplyRegionText">
                                        {(field) => (
                                            <div className="sm:col-span-2">
                                                <field.TextField
                                                    id="supplier-offerings-dialog-revise-supply-region"
                                                    label="可供区域"
                                                    required
                                                    description="多个区域使用逗号分隔"
                                                />
                                            </div>
                                        )}
                                    </form.AppField>
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
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </FieldSet>
                        </div>

                        <div className="grid gap-4 lg:grid-cols-2">
                            <FieldSet className="gap-4 rounded-lg border p-4">
                                <FieldLegend variant="label">
                                    物流与费用
                                </FieldLegend>
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="dropshipExpress">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-dropship-express"
                                                label="一件代发快递说明"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="freightAmount">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-freight-amount"
                                                label="运费"
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="serviceFeeAmount">
                                        {(field) => (
                                            <field.TextField
                                                id="supplier-offerings-dialog-revise-service-fee-amount"
                                                label="服务费"
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            </FieldSet>

                            <FieldSet className="gap-4 rounded-lg border p-4">
                                <FieldLegend variant="label">
                                    状态与说明
                                </FieldLegend>
                                <div className="grid gap-3 sm:grid-cols-2">
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
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
