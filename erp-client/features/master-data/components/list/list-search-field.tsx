"use client"

import * as React from "react"
import { ArrowRightIcon, SearchIcon } from "lucide-react"

import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function ListSearchField({
    searchInputRef,
    value,
    onChange,
    placeholder,
    showSubmit = false,
    submitLabel = "应用搜索与筛选",
}: {
    searchInputRef: React.RefObject<HTMLInputElement | null>
    value: string
    onChange: (value: string) => void
    placeholder: string
    showSubmit?: boolean
    submitLabel?: string
}) {
    return (
        <InputGroup>
            <InputGroupAddon>
                <SearchIcon aria-hidden="true" />
            </InputGroupAddon>
            <InputGroupInput
                ref={searchInputRef}
                value={value}
                onChange={(event) => onChange(event.target.value)}
                placeholder={placeholder}
                aria-label={masterDataCopy.searchAria}
            />
            {showSubmit ? (
                <InputGroupAddon align="inline-end" className="pr-1">
                    <InputGroupButton
                        type="submit"
                        variant="default"
                        size="icon-xs"
                        className="rounded-md"
                        aria-label={submitLabel}
                        title={submitLabel}
                    >
                        <ArrowRightIcon aria-hidden="true" />
                    </InputGroupButton>
                </InputGroupAddon>
            ) : null}
        </InputGroup>
    )
}
