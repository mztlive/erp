"use client"

import * as React from "react"

/** §10: 375 窄屏只读；平板仍可发起调整 */
export function usePhoneNarrow(): boolean {
    return React.useSyncExternalStore(
        (onChange) => {
            const mq = window.matchMedia("(max-width: 480px)")
            mq.addEventListener("change", onChange)
            return () => mq.removeEventListener("change", onChange)
        },
        () => window.matchMedia("(max-width: 480px)").matches,
        () => false,
    )
}
