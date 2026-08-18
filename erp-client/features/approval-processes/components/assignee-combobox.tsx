"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"

import { assigneeEligibilityLabel } from "../labels"
import { useEligibleAssigneesQuery } from "../queries"
import type { DocumentType, EligibleAssignee } from "../types"

/**
 * 定义期审批人选择器。只查询 eligible-assignees，不下载全量账号。
 */
export function AssigneeCombobox({
    documentType,
    value,
    selectedName,
    onChange,
    disabled = false,
    assignees,
}: {
    documentType: DocumentType
    value: string
    selectedName: string
    onChange: (assignee: EligibleAssignee | null) => void
    disabled?: boolean
    /** 测试可注入候选人，生产路径走 Query。 */
    assignees?: readonly EligibleAssignee[]
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
    )
    const rows = assignees ?? query.data ?? []
    const options = rows.map((item) => ({
        value: item.user_id,
        label: assigneeEligibilityLabel(item.name),
        keywords: item.name,
    }))
    if (
        value &&
        selectedName &&
        !options.some((item) => item.value === value)
    ) {
        options.unshift({
            value,
            label: assigneeEligibilityLabel(selectedName),
            keywords: selectedName,
        })
    }

    return (
        <OptionCombobox
            aria-label="选择审批人"
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
            placeholder="搜索并选择一位审批人"
            emptyLabel="没有符合定义期资格的账号"
            disabled={disabled}
            allowClear
        />
    )
}
