"use client"

import * as React from "react"
import { SearchIcon } from "lucide-react"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function ListSearchField({
    searchInputRef,
    value,
    onChange,
    placeholder,
    id = "master-data-list-search-input",
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    value: string
    onChange: (value: string) => void
    placeholder: string
    id?: string
}) {
    return (
        <InputGroup>
            <InputGroupAddon>
                <SearchIcon aria-hidden="true" />
            </InputGroupAddon>
            <InputGroupInput
                id={id}
                ref={searchInputRef}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                aria-label={masterDataCopy.searchAria}
            />
        </InputGroup>
    )
}
