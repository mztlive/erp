"use client"

import { useAppForm } from "@/components/form"
import { ListWorkspaceFilterBar } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { PaymentListFilterInput } from "./payment-list-filter-input"

type TextFilters = Record<string, string>

/**
 * 可申请采购单与付款申请共用的文本筛选条。
 * 查询时去掉首尾空白；未传入重置回调时，重置直接提交空条件。
 */
export function PaymentTextFilterBar<T extends TextFilters>({
    idPrefix,
    formAriaLabel,
    fields,
    applied,
    empty,
    onApply,
    onReset,
    chips,
    idleHint,
}: {
    idPrefix: string
    formAriaLabel: string
    fields: readonly (readonly [keyof T & string, string, string])[]
    applied: T
    empty: T
    onApply: (filters: T) => void
    onReset?: () => void
    chips?: { key: string; label: string; onClear: () => void }[]
    idleHint: string
}) {
    const form = useAppForm({
        defaultValues: applied,
        onSubmit: ({ value }) =>
            onApply(
                Object.fromEntries(
                    Object.entries(value).map(([key, text]) => [
                        key,
                        text.trim(),
                    ]),
                ) as T,
            ),
    })
    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => (
                <ListWorkspaceFilterBar
                    idPrefix={idPrefix}
                    formAriaLabel={formAriaLabel}
                    onSubmit={() => void form.handleSubmit()}
                    search={null}
                    className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:items-end"
                    primaryFilters={fields.map(([key, label, placeholder]) => (
                        <form.Field key={key} name={key}>
                            {(field) => (
                                <div className="flex w-52 max-w-full min-w-0 flex-col gap-1.5">
                                    <label
                                        className="text-xs text-muted-foreground"
                                        htmlFor={`${idPrefix.replace(/-filters$/, "")}-filter-${key.replaceAll("_", "-")}`}
                                    >
                                        {label}
                                    </label>
                                    <PaymentListFilterInput
                                        id={`${idPrefix.replace(/-filters$/, "")}-filter-${key.replaceAll("_", "-")}`}
                                        name={key}
                                        value={field.state.value as string}
                                        onValueChange={(next) =>
                                            field.handleChange(
                                                next as typeof field.state.value,
                                            )
                                        }
                                        onBlur={field.handleBlur}
                                        placeholder={placeholder}
                                    />
                                </div>
                            )}
                        </form.Field>
                    ))}
                    extraPrimary={
                        <Button
                            id={`${idPrefix}-reset`}
                            type="button"
                            variant="outline"
                            onClick={() => {
                                form.reset(empty)
                                if (onReset) onReset()
                                else onApply(empty)
                            }}
                        >
                            重置
                        </Button>
                    }
                    {...(chips !== undefined ? { chips } : {})}
                    hasPendingChanges={fields.some(
                        ([key]) => values[key].trim() !== applied[key],
                    )}
                    resultStatus={null}
                    idleHint={idleHint}
                />
            )}
        </form.Subscribe>
    )
}
