"use client"

import * as React from "react"

export type RoleOption = {
    id: string
    name: string
}

/** 角色选项关键词过滤：名称或 ID 命中即保留；空关键词返回原数组（引用不变）。 */
export function useRoleFilter(options: readonly RoleOption[]) {
    const [keyword, setKeyword] = React.useState("")

    const filtered = React.useMemo(() => {
        const q = keyword.trim().toLowerCase()
        if (!q) return options
        return options.filter((role) =>
            [role.name, role.id].join(" ").toLowerCase().includes(q),
        )
    }, [keyword, options])

    return { keyword, setKeyword, filtered }
}
