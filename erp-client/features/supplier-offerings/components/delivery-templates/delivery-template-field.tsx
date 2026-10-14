"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import { OptionCombobox } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import { useSupplierDeliveryTemplatesQuery } from "../../hooks/queries"
import { DeliveryRulesPreview } from "./delivery-rules-preview"
import { DeliveryTemplateEditorDialog } from "./delivery-template-editor-dialog"

/** 跟随供应商默认模板的选项值；表单内以空字符串保存。 */
const FOLLOW_DEFAULT = "__supplier_default__"

/** 新增供给时选择配送模板，并即时展示生效规则。 */
export function DeliveryTemplateField({
    id,
    supplierId,
    value,
    onChange,
    disabled = false,
}: {
    id: string
    supplierId: string
    /** 模板主键；空字符串表示跟随供应商默认模板。 */
    value: string
    onChange: (value: string) => void
    disabled?: boolean
}) {
    const templates = useSupplierDeliveryTemplatesQuery(supplierId)
    const account = useAccountProfileQuery()
    const canCreate = hasPermission(
        account.data?.permissions,
        "supplier_offering:update",
    )
    const [creating, setCreating] = React.useState(false)
    const list = templates.data ?? []
    const fallback = list.find((template) => template.is_default) ?? null
    const selected = value
        ? (list.find((template) => template.id === value) ?? null)
        : fallback
    const options = [
        {
            value: FOLLOW_DEFAULT,
            label: fallback
                ? `跟随供应商默认（${fallback.name}）`
                : "跟随供应商默认（未设置，全国配送、包邮）",
        },
        ...list.map((template) => ({
            value: template.id,
            label: template.is_default
                ? `${template.name}（默认）`
                : template.name,
        })),
    ]
    return (
        <Field className="min-w-0 gap-2">
            <div className="flex items-center justify-between gap-3">
                <FieldLabel htmlFor={id}>配送模板</FieldLabel>
                {canCreate ? (
                    <Button
                        id={`${id}-create`}
                        type="button"
                        variant="link"
                        size="xs"
                        disabled={!supplierId || disabled}
                        onClick={() => setCreating(true)}
                    >
                        <PlusIcon data-icon="inline-start" aria-hidden />
                        新建模板
                    </Button>
                ) : null}
            </div>
            <OptionCombobox
                id={id}
                aria-label="配送模板"
                value={supplierId ? value || FOLLOW_DEFAULT : null}
                disabled={!supplierId || disabled}
                loading={Boolean(supplierId) && templates.isPending}
                allowClear={false}
                placeholder={supplierId ? "请选择配送模板" : "请先选择供应商"}
                options={options}
                onValueChange={(next) =>
                    onChange(!next || next === FOLLOW_DEFAULT ? "" : next)
                }
                className="w-full"
            />
            {supplierId ? (
                <div
                    aria-live="polite"
                    className="rounded-lg border bg-muted/30 px-4 py-3"
                >
                    {templates.isError ? (
                        <p className="text-sm text-muted-foreground">
                            模板读取失败，保存后可在供给资料页再设置。
                        </p>
                    ) : templates.isPending ? (
                        <p className="text-sm text-muted-foreground">
                            正在读取配送模板…
                        </p>
                    ) : (
                        <DeliveryRulesPreview template={selected} />
                    )}
                </div>
            ) : (
                <FieldDescription>
                    决定哪些地区能配送、集采是否另付运费。未设置时按全国配送、包邮判定。
                </FieldDescription>
            )}
            {creating ? (
                <DeliveryTemplateEditorDialog
                    supplierId={supplierId}
                    template={null}
                    open
                    onOpenChange={(open) => {
                        if (!open) setCreating(false)
                    }}
                />
            ) : null}
        </Field>
    )
}
