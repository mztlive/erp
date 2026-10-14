"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { getErrorMessage } from "@/lib/api/errors"

import {
    useAssignDeliveryTemplateMutation,
    useSupplierDeliveryTemplatesQuery,
} from "../../hooks/queries"
import { ruleSetSummary } from "../../lib/delivery-template-summary"
import type { SupplierOfferingView } from "../../types"

/** 跟随供应商默认模板的选项值。 */
const FOLLOW_DEFAULT = "__supplier_default__"

const schema = z.object({ choice: z.string().min(1, "请选择配送模板") })

/** 为同一供应商的供给指定配送模板，或改回跟随供应商默认。 */
export function AssignDeliveryTemplateDialog({
    offerings,
    onOpenChange,
}: {
    offerings: readonly SupplierOfferingView[]
    onOpenChange: (open: boolean) => void
}) {
    const supplierIds = [...new Set(offerings.map((item) => item.supplier_id))]
    const supplierId = supplierIds.length === 1 ? supplierIds[0]! : ""
    const templates = useSupplierDeliveryTemplatesQuery(supplierId)
    const mutation = useAssignDeliveryTemplateMutation()
    const [failure, setFailure] = React.useState<string | null>(null)
    const current =
        offerings.length === 1 ? offerings[0]?.delivery_template : null
    const form = useAppForm({
        defaultValues: {
            choice:
                current?.source === "ASSIGNED" && current.template_id
                    ? current.template_id
                    : FOLLOW_DEFAULT,
        },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            setFailure(null)
            try {
                await mutation.mutateAsync({
                    offeringIds: offerings.map((item) => item.id),
                    templateId:
                        value.choice === FOLLOW_DEFAULT ? null : value.choice,
                })
                onOpenChange(false)
            } catch (error) {
                setFailure(getErrorMessage(error, "设置失败，请稍后重试。"))
            }
        },
    })
    const options = [
        { value: FOLLOW_DEFAULT, label: "跟随供应商默认模板" },
        ...(templates.data ?? []).map((template) => ({
            value: template.id,
            label: template.is_default
                ? `${template.name}（默认）`
                : template.name,
        })),
    ]
    return (
        <Dialog open onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="assign-delivery-template-close">
                <DialogHeader>
                    <DialogTitle>设置配送模板</DialogTitle>
                    <DialogDescription>
                        {offerings.length === 1
                            ? `${offerings[0]?.sku_name ?? "供给"} · ${offerings[0]?.supplier_name ?? "供应商"}`
                            : `已选 ${offerings.length} 条供给`}
                        。设置后影响之后的供给分配，不改变价格、起订量和有效期。
                    </DialogDescription>
                </DialogHeader>
                {!supplierId ? (
                    <p role="alert" className="text-sm text-destructive">
                        请只选择同一供应商的供给。
                    </p>
                ) : (
                    <form
                        className="flex flex-col gap-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="choice">
                            {(field) => (
                                <field.SelectField
                                    id="assign-delivery-template-choice"
                                    label="配送模板"
                                    required
                                    allowClear={false}
                                    loading={templates.isPending}
                                    options={options}
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => state.values.choice}
                        >
                            {(choice) => {
                                const template = templates.data?.find(
                                    (item) => item.id === choice,
                                )
                                return template ? (
                                    <div className="space-y-1 text-xs text-muted-foreground">
                                        <p>
                                            代发：
                                            {ruleSetSummary(
                                                "dropship",
                                                template.dropship,
                                            )}
                                        </p>
                                        <p>
                                            集采：
                                            {ruleSetSummary(
                                                "bulk",
                                                template.bulk,
                                            )}
                                        </p>
                                    </div>
                                ) : null
                            }}
                        </form.Subscribe>
                        {failure ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                {failure}
                            </p>
                        ) : null}
                        <DialogFooter>
                            <Button
                                id="assign-delivery-template-cancel"
                                type="button"
                                variant="outline"
                                onClick={() => onOpenChange(false)}
                            >
                                取消
                            </Button>
                            <Button
                                id="assign-delivery-template-submit"
                                type="submit"
                                disabled={mutation.isPending}
                            >
                                确认设置
                            </Button>
                        </DialogFooter>
                    </form>
                )}
            </DialogContent>
        </Dialog>
    )
}
