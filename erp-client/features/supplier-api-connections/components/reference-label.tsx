"use client"

import { BusinessStatusBadge } from "@/components/business"
import type { ReferenceState } from "@/features/supplier-api-connections/types"
import { REFERENCE_STATE_LABEL } from "@/features/supplier-api-connections/types"

/** 安全引用状态展示：只显示绑定状态、别名与版本，不接触引用正文。 */
export function RefLabel({
    state,
    alias,
    version,
    visible,
}: {
    state: ReferenceState
    alias?: string
    version?: string
    visible: boolean
}) {
    const label = REFERENCE_STATE_LABEL[state]
    return (
        <div
            className="space-y-0.5"
            aria-label={`引用状态 ${label}${
                visible && alias ? ` 别名 ${alias} 版本 ${version}` : ""
            }`}
        >
            <BusinessStatusBadge
                context="list"
                label={label}
                tone={
                    state === "BOUND"
                        ? "success"
                        : state === "ROTATION_DUE"
                          ? "warning"
                          : "neutral"
                }
            />
            {visible && alias ? (
                <div className="font-mono text-xs text-muted-foreground">
                    {alias}
                    {version ? ` · ${version}` : ""}
                </div>
            ) : (
                <div className="text-xs text-muted-foreground">
                    {state === "BOUND"
                        ? "配置已绑定"
                        : state === "ROTATION_DUE"
                          ? "需轮换"
                          : "待绑定"}
                </div>
            )}
        </div>
    )
}
