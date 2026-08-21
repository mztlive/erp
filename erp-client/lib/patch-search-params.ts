/**
 * URL 查询参数修补统一出口。
 *
 * 覆盖各页面原 patchUrl 副本的差异：
 * - 空值删除语义：value == null || value === "" → delete(key)，否则 set(key, value)
 * - replace 行为：options.replace → router.replace，否则 router.push
 * - keep-view：context.view 提供时，若结果缺少 view 参数则回填该值
 *   （access-audit / mall-sync / inventory / customer-receivables / supplier-payables 原实现）
 * - cursor 清理：context.clearCursor 且 patch 未含 cursor/pageSize 时删除 cursor
 *   （inventory 原实现）
 *
 * 注意：supplier-settlements / history-backfill / import-opening / supplier-api-connections
 * 的 patchUrl 是「结构化 URL state 合并 + 领域序列化 + 恒 replace」语义，与本函数不兼容，
 * 保留原实现（见任务报告）。
 */
"use client"

import type { useRouter } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"

export type PatchUrlParams = Record<string, string | null | undefined>

export interface PatchUrlOptions {
    replace?: boolean
    /** 传入 false 时保持当前滚动位置（筛选写入 URL 不跳动；不传则保持原行为）。 */
    scroll?: boolean
}

export interface PatchUrlContext {
    router: ReturnType<typeof useRouter>
    pathname: string
    searchParams: ReadonlyURLSearchParams
    /** 提供时：结果缺少 view 参数则回填该值（keep-view 语义） */
    view?: string
    /** 非 cursor/pageSize 变更时清除 cursor（inventory 语义） */
    clearCursor?: boolean
}

export function patchUrl(
    context: PatchUrlContext,
    patch: PatchUrlParams,
    options?: PatchUrlOptions,
): void {
    const { router, pathname, searchParams, view, clearCursor } = context
    const next = new URLSearchParams(searchParams.toString())
    for (const [key, value] of Object.entries(patch)) {
        if (value == null || value === "") next.delete(key)
        else next.set(key, value)
    }
    if (clearCursor && !("cursor" in patch) && !("pageSize" in patch)) {
        next.delete("cursor")
    }
    if (view !== undefined && !next.get("view")) {
        next.set("view", view)
    }
    const qs = next.toString()
    const href = qs ? `${pathname}?${qs}` : pathname
    if (options?.scroll !== undefined) {
        const scrollOption = { scroll: options.scroll }
        if (options?.replace) router.replace(href, scrollOption)
        else router.push(href, scrollOption)
        return
    }
    if (options?.replace) router.replace(href)
    else router.push(href)
}
