"use client"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { CompanySkuSearchCombobox } from "@/features/entity-selectors"
import { Field, FieldLabel, FieldError } from "@/components/ui/field"
import { useMatchCatalogMutation } from "../../hooks/queries"
import type { CatalogItem } from "../../api/catalog"
import { z } from "zod"

export function CatalogMatchDialog({
    connectionId,
    item,
    onClose,
}: {
    connectionId: string
    item: CatalogItem
    onClose: () => void
}) {
    const mutation = useMatchCatalogMutation()
    const form = useAppForm({
        defaultValues: { skuId: "" },
        validators: {
            onSubmit: z.object({
                skuId: z.string().min(1, "请选择公司商品规格"),
            }),
        },
        onSubmit: async ({ value }) => {
            try {
                await mutation.mutateAsync({
                    connectionId,
                    itemId: item.id,
                    skuId: value.skuId,
                    expectedVersion: item.version,
                })
                onClose()
            } catch {
                /* mutation.error 在表单显示 */
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !mutation.isPending) onClose()
            }}
        >
            <DialogContent closeButtonId="supplier-catalog-match-close">
                <DialogHeader>
                    <DialogTitle>匹配公司商品</DialogTitle>
                    <DialogDescription>
                        {item.name} · {item.specification}
                        。请核对规格和单位；保存只建立匹配，后续仍需维护供给。
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="supplier-catalog-match-form"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                    className="space-y-4"
                >
                    <form.AppField name="skuId">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="supplier-catalog-match-sku">
                                    公司商品规格
                                </FieldLabel>
                                <CompanySkuSearchCombobox
                                    id="supplier-catalog-match-sku"
                                    value={field.state.value || undefined}
                                    onValueChange={(value) =>
                                        field.handleChange(value ?? "")
                                    }
                                    onBlur={field.handleBlur}
                                    allowClear={false}
                                    label="公司商品规格"
                                    placeholder="搜索商品名称或 SKU 编号"
                                    aria-invalid={
                                        field.state.meta.errors.length > 0
                                    }
                                    aria-describedby="supplier-catalog-match-error"
                                />
                                <FieldError id="supplier-catalog-match-error">
                                    {field.state.meta.errors
                                        .map((error) =>
                                            typeof error === "string"
                                                ? error
                                                : error?.message,
                                        )
                                        .join("；")}
                                </FieldError>
                            </Field>
                        )}
                    </form.AppField>
                    {mutation.error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {mutation.error.message}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="supplier-catalog-match-cancel"
                            type="button"
                            variant="outline"
                            disabled={mutation.isPending}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            id="supplier-catalog-match-save"
                            type="submit"
                            loading={mutation.isPending}
                        >
                            保存匹配
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
