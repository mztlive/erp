"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"

import { useEligibleAssigneesQuery } from "../queries"
import type { DocumentType, EligibleAssignee } from "../types"

/**
 * 定义期审批人选择器。只查询 eligible-assignees，不下载全量账号。
 */
export function AssigneeCombobox({
    documentType,
    copies = false,
    value,
    selectedName,
    onChange,
    disabled = false,
    assignees,
    id,
    excludedIds = [],
    placeholder = "搜索并选择一位审批人",
}: {
    copies?: boolean
    documentType: DocumentType
    value: string
    selectedName: string
    onChange: (assignee: EligibleAssignee | null) => void
    disabled?: boolean
    /** 测试可注入候选人，生产路径走 Query。 */
    assignees?: readonly EligibleAssignee[]
    id?: string
    excludedIds?: readonly string[]
    placeholder?: string
}) {
    const [search, setSearch] = React.useState("")
    const [debounced, setDebounced] = React.useState("")

    React.useEffect(() => {
        const handle = window.setTimeout(() => setDebounced(search.trim()), 250)
        return () => window.clearTimeout(handle)
    }, [search])

    const query = useEligibleAssigneesQuery(
        documentType,
        debounced,
        assignees == null && !disabled,
        copies,
    )
    const rows = (assignees ?? query.data ?? []).filter(
        (person) => !excludedIds.includes(person.user_id),
    )
    // 候选人已经过服务端资格过滤，主行只显示人名；资格说明见空状态。
    const options = rows.map((item) => ({
        value: item.user_id,
        label: item.name,
        keywords: item.name,
    }))
    if (
        value &&
        selectedName &&
        !options.some((item) => item.value === value)
    ) {
        options.unshift({
            value,
            label: selectedName,
            keywords: selectedName,
        })
    }

    return (
        <OptionCombobox
            id={id}
            aria-label={copies ? "选择抄送人" : "选择审批人"}
            options={options}
            value={value || null}
            onValueChange={(next) => {
                if (!next) {
                    onChange(null)
                    return
                }
                const matched = rows.find((item) => item.user_id === next)
                onChange(
                    matched ?? {
                        user_id: next,
                        name: selectedName,
                    },
                )
            }}
            onSearchChange={setSearch}
            filterMode="remote"
            loading={assignees == null && query.isFetching}
            placeholder={placeholder}
            emptyLabel={
                copies ? "没有匹配的有效内部账号" : "没有符合定义期资格的账号"
            }
            disabled={disabled}
            allowClear
        />
    )
}
