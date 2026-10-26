"use client"

import { useState } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Textarea } from "@/components/ui/textarea"
import type { CatalogIdentityReview } from "../../api/identity-review"
import {
    useCatalogIdentityReviewQuery,
    useConfirmCatalogIdentityReviewMutation,
} from "../../hooks/identity-review"
import { newIdempotencyKey } from "../../lib/operations"

export function CatalogIdentityReviewDialog({
    connectionId,
    itemId,
    onClose,
}: {
    connectionId: string
    itemId: string
    onClose: () => void
}) {
    const query = useCatalogIdentityReviewQuery(connectionId, itemId)
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent closeButtonId="supplier-catalog-identity-close">
                <DialogHeader>
                    <DialogTitle>核对规格与单位</DialogTitle>
                    <DialogDescription>
                        仅在供应商修改了描述、实际商品和单位仍一致时确认。尺寸、包装数量或单位含义实际变化时，应建立正确的商品规格和供给。
                    </DialogDescription>
                </DialogHeader>
                {query.isPending ? (
                    <p role="status">正在读取核对资料…</p>
                ) : null}
                {query.error ? (
                    <p role="alert" className="text-sm text-destructive">
                        {query.error.message}
                    </p>
                ) : null}
                <Button
                    id="supplier-catalog-identity-refresh"
                    type="button"
                    variant="outline"
                    disabled={query.isFetching}
                    onClick={() => void query.refetch()}
                >
                    重新读取资料
                </Button>
                {query.data ? (
                    <IdentityReviewForm
                        key={`${query.data.item_version}-${query.data.sku_revision_id}-${query.data.offering_version}`}
                        view={query.data}
                        connectionId={connectionId}
                        onClose={onClose}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

function IdentityReviewForm({
    view,
    connectionId,
    onClose,
}: {
    view: CatalogIdentityReview
    connectionId: string
    onClose: () => void
}) {
    const mutation = useConfirmCatalogIdentityReviewMutation()
    const [operationId] = useState(() => newIdempotencyKey("catalog-identity"))
    const form = useAppForm({
        defaultValues: { sameIdentity: false, sameUnit: false, reason: "" },
        validators: {
            onSubmit: z.object({
                sameIdentity: z
                    .boolean()
                    .refine(Boolean, "请先核对实际商品及包装数量"),
                sameUnit: z.boolean().refine(Boolean, "请先核对单位含义"),
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写核对依据")
                    .max(1000, "核对依据最多1000字"),
            }),
        },
        onSubmit: async ({ value }) => {
            try {
                await mutation.mutateAsync({
                    connectionId,
                    itemId: view.item_id,
                    request: {
                        expected_version: view.item_version,
                        expected_sku_revision_id: view.sku_revision_id,
                        expected_offering_version: view.offering_version,
                        source_specification: view.source_specification,
                        source_unit: view.source_unit,
                        confirmed_base_unit_id: view.company_unit_id,
                        confirm_same_identity:
                            value.sameIdentity && value.sameUnit,
                        reason: value.reason.trim(),
                        idempotency_key: operationId,
                    },
                })
                onClose()
            } catch {
                /* 服务端错误在下方显示；冲突后须重新读取，不自动重试。 */
            }
        },
    })
    return (
        <form
            id="supplier-catalog-identity-form"
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <dl className="grid grid-cols-2 gap-3 rounded-md border p-3 text-sm">
                <div>
                    <dt className="text-muted-foreground">供应商当前规格</dt>
                    <dd>{view.source_specification}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">公司当前规格</dt>
                    <dd>{view.company_specification}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">供应商单位</dt>
                    <dd>{view.source_unit ?? "未提供，需人工核实"}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">公司基础单位</dt>
                    <dd>
                        {view.company_unit_name}（{view.company_unit_code}）
                    </dd>
                </div>
            </dl>
            <form.AppField name="sameIdentity">
                {(field) => (
                    <Field>
                        <label
                            className="flex items-start gap-2"
                            htmlFor="supplier-catalog-identity-same"
                        >
                            <NativeCheckbox
                                id="supplier-catalog-identity-same"
                                checked={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.checked)
                                }
                                onBlur={field.handleBlur}
                            />
                            已核实实际商品、规格及包装数量与公司当前规格相同
                        </label>
                        <FieldError errors={field.state.meta.errors} />
                    </Field>
                )}
            </form.AppField>
            <form.AppField name="sameUnit">
                {(field) => (
                    <Field>
                        <label
                            className="flex items-start gap-2"
                            htmlFor="supplier-catalog-identity-unit"
                        >
                            <NativeCheckbox
                                id="supplier-catalog-identity-unit"
                                checked={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.checked)
                                }
                                onBlur={field.handleBlur}
                            />
                            已核实一份供应商商品对应一个公司基础单位，无需数量换算
                        </label>
                        <FieldError errors={field.state.meta.errors} />
                    </Field>
                )}
            </form.AppField>
            <form.AppField name="reason">
                {(field) => (
                    <Field>
                        <FieldLabel htmlFor="supplier-catalog-identity-reason">
                            核对依据
                        </FieldLabel>
                        <Textarea
                            id="supplier-catalog-identity-reason"
                            value={field.state.value}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                            onBlur={field.handleBlur}
                            placeholder="填写已核实的规格、包装和单位依据"
                        />
                        <FieldError errors={field.state.meta.errors} />
                    </Field>
                )}
            </form.AppField>
            {mutation.error ? (
                <p role="alert" className="text-sm text-destructive">
                    {mutation.error.message}。资料有变化时，请重新读取后核对。
                </p>
            ) : null}
            <DialogFooter>
                <Button
                    id="supplier-catalog-identity-cancel"
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending}
                    onClick={onClose}
                >
                    取消
                </Button>
                <LoadingButton
                    id="supplier-catalog-identity-confirm"
                    type="submit"
                    loading={mutation.isPending}
                    disabled={!view.requires_identity_review}
                >
                    确认核对结果
                </LoadingButton>
            </DialogFooter>
        </form>
    )
}
