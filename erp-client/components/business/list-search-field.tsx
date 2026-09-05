"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"

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
    return (
        <InputGroup>
            <InputGroupAddon>
                <SearchIcon aria-hidden="true" />
            </InputGroupAddon>
            <InputGroupInput
                id={id}
                ref={searchInputRef}
                data-slot={dataSlot}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                aria-label={ariaLabel}
            />
        </InputGroup>
    )
}
