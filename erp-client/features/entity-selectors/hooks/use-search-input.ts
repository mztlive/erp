"use client"

import * as React from "react"

import { useDebouncedSearch } from "@/features/entity-selectors/hooks/queries"

/** 组合框输入态：即时回显 + 防抖后的查询词。 */
export function useSearchInput() {
    const [input, setInput] = React.useState("")
    return { input: useDebouncedSearch(input), onSearchChange: setInput }
}
