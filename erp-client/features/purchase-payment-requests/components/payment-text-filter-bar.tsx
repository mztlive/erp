"use client"

import { useRef, useState } from "react"
import {
    ListFilterSection,
    ListFilterSections,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    listWorkspaceStyles,
    type ListWorkspaceFilterChip,
} from "@/components/business/list-workspace"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { listFilterText } from "@/lib/ui-text"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { PaymentListFilterInput } from "./payment-list-filter-input"

type TextFilters = Record<string, string>
type FilterField<T> = readonly [keyof T & string, string, string]

const isEntity = (key: string) =>
    key === "supplier_id" || key === "payer_party_id"

export function PaymentTextFilterBar<T extends TextFilters>({
    idPrefix,
    formAriaLabel,
    fields,
    searchKey,
    applied,
    empty,
    moreLegend,
    onApply,
    onReset,
    scopeChip,
    resultStatus,
}: {
    idPrefix: string
    formAriaLabel: string
    fields: readonly FilterField<T>[]
    searchKey: keyof T & string
    applied: T
    empty: T
    moreLegend: string
    onApply: (filters: T, clearScope?: boolean) => void
    onReset: () => void
    scopeChip?: ListWorkspaceFilterChip
    resultStatus?: string | null
}) {
    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const [moreOpen, setMoreOpen] = useState(false)
    const form = useAppForm({
        defaultValues: { ...applied } as TextFilters,
        onSubmit: ({ value }) => {
            setMoreOpen(false)
            const filters = { ...applied }
            for (const [key] of fields)
                filters[key] = value[key].trim() as T[typeof key]
            onApply(filters)
        },
    })
    const searchName: string = searchKey
    const searchField = fields.find(([key]) => key === searchKey)
    const primaryFields = fields.filter(([key]) => isEntity(key))
    const moreFields = fields.filter(
        ([key]) => key !== searchKey && !isEntity(key),
    )
    const idFor = (key: string) =>
        `${idPrefix.replace(/-filters$/, "")}-filter-${key.replaceAll("_", "-")}`
    const moreChips = moreFields.flatMap(([key, label]) =>
        applied[key]
            ? [
                  {
                      key,
                      label: `${label}：${applied[key]}`,
                      onClear: () => onApply({ ...applied, [key]: "" }),
                  },
              ]
            : [],
    )
    const chips = scopeChip ? [scopeChip, ...moreChips] : moreChips

    function renderField([key, label, placeholder]: FilterField<T>) {
        const id = idFor(key)
        const name: string = key
        return (
            <form.Field key={key} name={name}>
                {(field) => {
                    const input = (
                        <PaymentListFilterInput
                            id={id}
                            name={key}
                            label={label}
                            value={field.state.value}
                            onValueChange={field.handleChange}
                            onBlur={field.handleBlur}
                            placeholder={placeholder}
                        />
                    )
                    return isEntity(key) ? (
                        <div
                            className="w-44 max-w-full min-w-0"
                            data-has-value={Boolean(field.state.value)}
                        >
                            {input}
                        </div>
                    ) : (
                        <ListWorkspaceFilterField htmlFor={id} label={label}>
                            {input}
                        </ListWorkspaceFilterField>
                    )
                }}
            </form.Field>
        )
    }

    return (
        <form.Subscribe selector={(state) => state.values}>
            {(values) => (
                <ListWorkspaceFilterBar
                    idPrefix={idPrefix}
                    formAriaLabel={formAriaLabel}
                    onSubmit={() => void form.handleSubmit()}
                    search={
                        searchField ? (
                            <form.Field name={searchName}>
                                {(field) => (
                                    <ListSearchField
                                        id={idFor(searchKey)}
                                        searchInputRef={searchInputRef}
                                        value={field.state.value}
                                        onChange={(value) =>
                                            field.handleChange(
                                                value.slice(0, 200),
                                            )
                                        }
                                        placeholder={`搜索${searchField[2]}`}
                                        aria-label={searchField[1]}
                                    />
                                )}
                            </form.Field>
                        ) : null
                    }
                    primaryFilters={primaryFields.map(renderField)}
                    extraPrimary={
                        <Button
                            id={`${idPrefix}-reset`}
                            type="button"
                            variant="ghost"
                            className={listWorkspaceStyles.quietButton}
                            onClick={() => {
                                form.reset(empty)
                                setMoreOpen(false)
                                onReset()
                            }}
                        >
                            {listFilterText.reset}
                        </Button>
                    }
                    morePresentation="popover"
                    moreOpen={moreOpen}
                    moreCount={moreChips.length}
                    onToggleMore={() => {
                        if (moreOpen) {
                            for (const [key] of moreFields)
                                form.setFieldValue<string>(key, applied[key])
                        }
                        setMoreOpen(!moreOpen)
                    }}
                    onResetMore={() => {
                        for (const [key] of moreFields)
                            form.setFieldValue<string>(key, empty[key])
                    }}
                    morePanel={
                        <ListFilterSections>
                            <ListFilterSection legend={moreLegend}>
                                {moreFields.map(renderField)}
                            </ListFilterSection>
                        </ListFilterSections>
                    }
                    chips={chips}
                    onClearAll={() => {
                        const filters = { ...applied }
                        for (const [key] of moreFields)
                            filters[key] = empty[key]
                        onApply(filters, Boolean(scopeChip))
                    }}
                    hasPendingChanges={fields.some(
                        ([key]) => values[key].trim() !== applied[key],
                    )}
                    resultStatus={resultStatus}
                />
            )}
        </form.Subscribe>
    )
}
