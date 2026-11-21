"use client"

import * as React from "react"
import { SearchIcon, XIcon } from "lucide-react"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"

function assignRef<T>(ref: React.Ref<T> | undefined, value: T | null) {
    if (!ref) return
    if (typeof ref === "function") ref(value)
    else (ref as React.MutableRefObject<T | null>).current = value
}

export function ListSearchField({
    id,
    searchInputRef,
    value,
    onChange,
    placeholder,
    "aria-label": ariaLabel = "搜索",
    "data-slot": dataSlot,
}: {
    id: string
    searchInputRef?: React.Ref<HTMLInputElement>
    value: string
    onChange: (value: string) => void
    placeholder: string
    "aria-label"?: string
    "data-slot"?: string
}) {
    const internalRef = React.useRef<HTMLInputElement | null>(null)
    const setRefs = React.useCallback(
        (node: HTMLInputElement | null) => {
            internalRef.current = node
            assignRef(searchInputRef, node)
        },
        [searchInputRef],
    )

    return (
        <InputGroup data-has-value={value.trim() ? "true" : undefined}>
            <InputGroupAddon>
                <SearchIcon aria-hidden="true" />
            </InputGroupAddon>
            <InputGroupInput
                id={id}
                ref={setRefs}
                data-slot={dataSlot}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                aria-label={ariaLabel}
            />
            {value.length > 0 ? (
                <InputGroupAddon align="inline-end">
                    <InputGroupButton
                        id={`${id}-clear`}
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        aria-label="清除搜索关键词"
                        title="清除"
                        className="rounded-full text-muted-foreground/75 hover:bg-muted hover:text-foreground"
                        onClick={() => {
                            onChange("")
                            internalRef.current?.focus()
                        }}
                    >
                        <XIcon aria-hidden="true" className="size-3.5" />
                    </InputGroupButton>
                </InputGroupAddon>
            ) : null}
        </InputGroup>
    )
}
