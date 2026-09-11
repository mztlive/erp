"use client"

import { useMemo, useState } from "react"
import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
    Combobox,
    ComboboxChip,
    ComboboxChips,
    ComboboxChipsInput,
    ComboboxContent,
    ComboboxItem,
    ComboboxList,
    ComboboxValue,
    useComboboxAnchor,
} from "@/components/ui/combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { splitValues } from "../lib/offering-forms"

// 仅作录入候选；保留原有文本区域值，不推导或扩展实际供货范围。
const suggestions = [
    "全国",
    "华东",
    "华南",
    "华北",
    "西南",
    "华中",
    "西北",
    "东北",
]

export function SupplyRegionField({ id }: { id: string }) {
    const field = useFieldContext<string>()
    const [query, setQuery] = useState("")
    const anchor = useComboboxAnchor()
    const values = useMemo(
        () => [...new Set(splitValues(field.state.value))],
        [field.state.value],
    )
    const items = useMemo(
        () => [
            ...new Set(
                [...suggestions, ...values, query.trim()].filter(Boolean),
            ),
        ],
        [values, query],
    )
    const invalid = field.state.meta.isTouched && !field.state.meta.isValid
    return (
        <Field className="gap-2 min-w-0" data-invalid={invalid || undefined}>
            <FieldLabel htmlFor={id}>
                可供区域<span className="text-destructive">*</span>
            </FieldLabel>
            <Combobox
                multiple
                items={items}
                value={values}
                onInputValueChange={setQuery}
                onValueChange={(next) => {
                    field.handleChange(
                        [...new Set(next.flatMap(splitValues))].join("、"),
                    )
                    field.handleBlur()
                }}
            >
                <div ref={anchor} className="min-w-0">
                    <ComboboxChips className="min-h-9">
                        <ComboboxValue>
                            {(selected: string[]) =>
                                selected.map((value) => (
                                    <ComboboxChip
                                        key={value}
                                        removeId={`${id}-chip-${toAutomationIdSegment(value)}-remove`}
                                        aria-label={`移除${value}`}
                                        removeLabel={`移除${value}`}
                                        className="max-w-full"
                                    >
                                        <span className="truncate">
                                            {value}
                                        </span>
                                    </ComboboxChip>
                                ))
                            }
                        </ComboboxValue>
                        <ComboboxChipsInput
                            id={id}
                            aria-label="可供区域"
                            aria-required="true"
                            aria-invalid={invalid || undefined}
                            aria-describedby={
                                invalid ? `${id}-error` : undefined
                            }
                            placeholder={
                                values.length
                                    ? "继续添加区域"
                                    : "搜索或输入区域，如全国、上海"
                            }
                            onBlur={field.handleBlur}
                        />
                    </ComboboxChips>
                </div>
                <ComboboxContent anchor={anchor}>
                    <ComboboxList>
                        {(item: string) => (
                            <ComboboxItem
                                key={item}
                                id={`${id}-option-${toAutomationIdSegment(item)}`}
                                value={item}
                            >
                                {suggestions.includes(item) ||
                                values.includes(item)
                                    ? item
                                    : `添加“${item}”`}
                            </ComboboxItem>
                        )}
                    </ComboboxList>
                </ComboboxContent>
            </Combobox>
            {invalid && (
                <FieldError
                    id={`${id}-error`}
                    errors={toFieldErrors(field.state.meta.errors)}
                />
            )}
        </Field>
    )
}
