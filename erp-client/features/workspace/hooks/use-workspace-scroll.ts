"use client"
import { useLayoutEffect, useRef } from "react"
const positions = new Map<string, number>()
/** 返回总览或队列时，恢复对应筛选条件的滚动位置。 */
export function useWorkspaceScroll<T extends HTMLElement>(key: string) {
    const ref = useRef<T>(null)
    useLayoutEffect(() => {
        const element = ref.current
        if (!element) return
        element.scrollTop = positions.get(key) ?? 0
        const remember = () => {
            positions.set(key, element.scrollTop)
            const oldest = positions.keys().next().value
            if (positions.size > 40 && oldest) positions.delete(oldest)
        }
        element.addEventListener("scroll", remember, { passive: true })
        return () => element.removeEventListener("scroll", remember)
    }, [key])
    return ref
}
