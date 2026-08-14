"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

export const SECTIONS = [
    { id: "overview", label: "概览" },
    { id: "content", label: "发布内容" },
    { id: "media", label: "媒体" },
    { id: "offering", label: "固定供给" },
    { id: "delivery", label: "发送与版本" },
    { id: "audit", label: "审计" },
] as const

export type SectionId = (typeof SECTIONS)[number]["id"]

export function parseSection(raw: string | null): SectionId {
    const found = SECTIONS.find((s) => s.id === raw)
    return found?.id ?? "overview"
}

/**
 * 发布中心 URL 状态：section 锚点与 revision 历史修订参数。
 * 写操作统一走 router.replace，参数变化后由调用方决定是否归一。
 */
export function usePublicationCenterUrlState(options: {
    dirty: boolean
    clearSessionEdit: () => void
}) {
    const { dirty, clearSessionEdit } = options
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const section = parseSection(searchParams.get("section"))
    const revisionParam = searchParams.get("revision") ?? undefined

    const setSection = (id: SectionId) => {
        const sp = new URLSearchParams(searchParams.toString())
        if (id === "overview") sp.delete("section")
        else sp.set("section", id)
        const qs = sp.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname)
    }

    const selectRevision = (revisionId: string) => {
        if (dirty) {
            if (
                !window.confirm(
                    "切换历史修订将放弃本次未提交输入。输入仅存在于当前页签，不会保存草稿。",
                )
            ) {
                return
            }
            clearSessionEdit()
        }
        const sp = new URLSearchParams(searchParams.toString())
        sp.set("section", "delivery")
        sp.set("revision", revisionId)
        router.replace(`${pathname}?${sp.toString()}`)
    }

    const clearRevision = React.useCallback(() => {
        const sp = new URLSearchParams(searchParams.toString())
        sp.delete("revision")
        const qs = sp.toString()
        router.replace(qs ? `${pathname}?${qs}` : pathname)
    }, [pathname, router, searchParams])

    return {
        section,
        revisionParam,
        setSection,
        selectRevision,
        clearRevision,
    }
}
