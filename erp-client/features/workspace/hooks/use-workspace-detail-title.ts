"use client"

import { useEffect } from "react"

import { stripDocumentNumberPrefix } from "@/features/workspace/lib/stable-number"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

const PLACEHOLDER_NUMBERS = new Set(["单号待补全", "业务单号待补全"])

/** 页签名：任务类型加单据号，足以在多个页签之间区分同一类任务。 */
export function workspaceDetailTitle(
    item: Pick<
        WorkspaceWorkItem,
        "workItemTypeLabel" | "stableNumber" | "objectTitle"
    >,
): string {
    const name = item.workItemTypeLabel.trim() || item.objectTitle.trim()
    const number = stripDocumentNumberPrefix(item.stableNumber)
    if (!name || !number || number === name || PLACEHOLDER_NUMBERS.has(number))
        return name
    return `${name} · ${number}`
}

/**
 * 独立详情页不渲染 h1，工作页签会退回 document.title。
 * 详情到达后改成具体任务名称，保留根布局的站点后缀。
 */
export function useWorkspaceDetailDocumentTitle(item?: WorkspaceWorkItem) {
    const title = item ? workspaceDetailTitle(item) : ""
    useEffect(() => {
        if (!title) return
        document.title = `${title} · 福尚云 ERP`
    }, [title])
}
