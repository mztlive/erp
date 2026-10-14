"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"

import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldError } from "@/components/ui/field"
import { RegionCascadeSelect } from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"

import { useSaveDeliveryTemplateMutation } from "../../hooks/queries"
import {
    BULK_TERM_LABEL,
    DROPSHIP_TERM_LABEL,
    type DeliveryTemplateView,
    type DeliveryTerm,
} from "../../types"

const MAX_RULES = 1000
const TERMS = ["FREIGHT_INCLUDED", "FREIGHT_EXTRA", "NOT_DELIVERABLE"] as const

type Mode = "dropship" | "bulk"

const TERM_OPTIONS: Record<Mode, { value: DeliveryTerm; label: string }[]> = {
    dropship: (
        Object.keys(DROPSHIP_TERM_LABEL) as (keyof typeof DROPSHIP_TERM_LABEL)[]
    ).map((value) => ({ value, label: DROPSHIP_TERM_LABEL[value] })),
    bulk: (Object.keys(BULK_TERM_LABEL) as DeliveryTerm[]).map((value) => ({
        value,
        label: BULK_TERM_LABEL[value],
    })),
}

const ruleSetSchema = z
    .object({
        defaultTerm: z.enum(TERMS),
        rules: z
            .array(
                z.object({
                    rowId: z.string(),
                    regionCode: z.string().min(1, "请选择地区"),
                    term: z.enum(TERMS),
                }),
            )
            .max(MAX_RULES, `每种方式最多 ${MAX_RULES} 个地区`),
    })
    .superRefine((set, context) => {
        const seen = new Set<string>()
        set.rules.forEach((rule, index) => {
            if (!rule.regionCode) return
            if (seen.has(rule.regionCode))
                context.addIssue({
                    code: "custom",
                    path: ["rules", index, "regionCode"],
                    message: "该地区已设置，请合并为一条",
                })
            seen.add(rule.regionCode)
        })
    })

const schema = z.object({
    name: z
        .string()
        .trim()
        .min(1, "请填写模板名称")
        .max(64, "模板名称不超过 64 字"),
    isDefault: z.boolean(),
    dropship: ruleSetSchema,
    bulk: ruleSetSchema,
})

type FormValues = z.input<typeof schema>

function blankValues(): FormValues {
    return {
        name: "",
        isDefault: false,
        dropship: { defaultTerm: "FREIGHT_INCLUDED", rules: [] },
        bulk: { defaultTerm: "FREIGHT_INCLUDED", rules: [] },
    }
}

function valuesOf(template: DeliveryTemplateView): FormValues {
    const rules = (set: DeliveryTemplateView["dropship"]) =>
        set.rules.map((rule) => ({
            rowId: crypto.randomUUID(),
            regionCode: rule.region_code,
            term: rule.term,
        }))
    return {
        name: template.name,
        isDefault: template.is_default,
        dropship: {
            defaultTerm: template.dropship.default_term,
            rules: rules(template.dropship),
        },
        bulk: {
            defaultTerm: template.bulk.default_term,
            rules: rules(template.bulk),
        },
    }
}

function useTemplateForm(
    supplierId: string,
    template: DeliveryTemplateView | null,
    onSaved: () => void,
    onFailure: (message: string | null) => void,
) {
    const mutation = useSaveDeliveryTemplateMutation(supplierId)
    const form = useAppForm({
        defaultValues: template ? valuesOf(template) : blankValues(),
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            onFailure(null)
            const ruleSet = (set: FormValues["dropship"]) => ({
                defaultTerm: set.defaultTerm,
                rules: set.rules.map((rule) => ({
                    regionCode: rule.regionCode,
                    term: rule.term,
                })),
            })
            try {
                await mutation.mutateAsync({
                    id: template?.id,
                    value: {
                        name: value.name,
                        isDefault: value.isDefault,
                        dropship: ruleSet(value.dropship),
                        bulk: ruleSet(value.bulk),
                        expectedVersion: template?.version,
                    },
                })
                onSaved()
            } catch (error) {
                onFailure(getErrorMessage(error, "保存失败，请稍后重试。"))
            }
        },
    })
    return { form, pending: mutation.isPending }
}

type TemplateForm = ReturnType<typeof useTemplateForm>["form"]

/** 新建或修改供应商配送模板。 */
export function DeliveryTemplateEditorDialog({
    supplierId,
    template,
    open,
    onOpenChange,
}: {
    supplierId: string
    /** 修改的模板；新建时为空。 */
    template: DeliveryTemplateView | null
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const [failure, setFailure] = React.useState<string | null>(null)
    const { form, pending } = useTemplateForm(
        supplierId,
        template,
        () => onOpenChange(false),
        setFailure,
    )
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[90svh] overflow-y-auto sm:max-w-3xl"
                closeButtonId="delivery-template-editor-close"
            >
                <DialogHeader>
                    <DialogTitle>
                        {template ? "修改配送模板" : "新建配送模板"}
                    </DialogTitle>
                    <DialogDescription>
                        代发价已含运费，只设置配送或不配送；集采设置包邮、运费另计或不配送。先设其他地区，再按省、市或区县列出例外，判定取最具体的一条。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-end">
                        <form.AppField name="name">
                            {(field) => (
                                <field.TextField
                                    id="delivery-template-editor-name"
                                    label="模板名称"
                                    required
                                    placeholder="例如：常温全国、冷链江浙沪"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="isDefault">
                            {(field) => (
                                <label
                                    htmlFor="delivery-template-editor-default"
                                    className="flex min-h-9 items-center gap-2 text-sm"
                                >
                                    <Checkbox
                                        id="delivery-template-editor-default"
                                        checked={field.state.value}
                                        onCheckedChange={(checked) =>
                                            field.handleChange(checked === true)
                                        }
                                    />
                                    设为供应商默认模板
                                </label>
                            )}
                        </form.AppField>
                    </div>
                    <RuleSetEditor form={form} mode="dropship" title="代发" />
                    <RuleSetEditor form={form} mode="bulk" title="集采" />
                    {failure ? (
                        <p role="alert" className="text-sm text-destructive">
                            {failure}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="delivery-template-editor-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="delivery-template-editor-submit"
                            type="submit"
                            disabled={pending}
                        >
                            保存模板
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

function RuleSetEditor({
    form,
    mode,
    title,
}: {
    form: TemplateForm
    mode: Mode
    title: string
}) {
    const prefix = `delivery-template-editor-${mode}`
    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-labelledby={`${prefix}-title`}
        >
            <h3 id={`${prefix}-title`} className="text-sm font-semibold">
                {title}
            </h3>
            <form.AppField name={`${mode}.defaultTerm`}>
                {(field) => (
                    <field.SelectField
                        id={`${prefix}-default`}
                        label="其他地区"
                        required
                        allowClear={false}
                        options={TERM_OPTIONS[mode]}
                        className="max-w-60"
                    />
                )}
            </form.AppField>
            <form.AppField name={`${mode}.rules`} mode="array">
                {(array) => (
                    <div className="space-y-2">
                        {array.state.value.map((rule, index) => {
                            const rowId = `${prefix}-rule-${toAutomationIdSegment(rule.rowId)}`
                            return (
                                <div
                                    key={rule.rowId}
                                    className="grid items-start gap-2 sm:grid-cols-[minmax(0,1fr)_9rem_auto]"
                                >
                                    <form.AppField
                                        name={`${mode}.rules[${index}].regionCode`}
                                    >
                                        {(field) => (
                                            <div className="min-w-0 space-y-1">
                                                <RegionCascadeSelect
                                                    idPrefix={`${rowId}-region`}
                                                    ariaLabel={`${title}例外地区`}
                                                    value={field.state.value}
                                                    onValueChange={(code) =>
                                                        field.handleChange(code)
                                                    }
                                                    onBlur={field.handleBlur}
                                                    invalid={
                                                        field.state.meta
                                                            .isTouched &&
                                                        !field.state.meta
                                                            .isValid
                                                    }
                                                />
                                                {field.state.meta.isTouched ? (
                                                    <FieldError
                                                        errors={toFieldErrors(
                                                            field.state.meta
                                                                .errors,
                                                        )}
                                                    />
                                                ) : null}
                                            </div>
                                        )}
                                    </form.AppField>
                                    <form.AppField
                                        name={`${mode}.rules[${index}].term`}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                id={`${rowId}-term`}
                                                label={`${title}例外判定`}
                                                required
                                                hideLabel
                                                allowClear={false}
                                                options={TERM_OPTIONS[mode]}
                                            />
                                        )}
                                    </form.AppField>
                                    <Button
                                        id={`${rowId}-remove`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() => array.removeValue(index)}
                                    >
                                        移除
                                    </Button>
                                </div>
                            )
                        })}
                        <Button
                            id={`${prefix}-add-rule`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={array.state.value.length >= MAX_RULES}
                            onClick={() =>
                                array.pushValue({
                                    rowId: crypto.randomUUID(),
                                    regionCode: "",
                                    term: "NOT_DELIVERABLE",
                                })
                            }
                        >
                            <PlusIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            添加例外地区
                        </Button>
                    </div>
                )}
            </form.AppField>
        </section>
    )
}
