"use client"

import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { QualityCaliber } from "../dual-types"

const CALIBER_TABS: ReadonlyArray<{
    value: QualityCaliber
    title: string
    basis: string
}> = [
    {
        value: "current",
        title: "当前负责客户的经营情况",
        basis: "现任主责归属",
    },
    {
        value: "history",
        title: "历史负责订单的贡献",
        basis: "首次生效冻结归属",
    },
]

/**
 * 双口径切换：同一时刻只展示一个口径，各自独立查询与导出，
 * 永不合并排名或合计。切换时清除对方口径的范围版本与分页。
 */
export function DualCaliberSwitch({
    caliber,
    onChange,
}: {
    caliber: QualityCaliber
    onChange: (next: QualityCaliber) => void
}) {
    return (
        <div
            role="tablist"
            aria-label="经营质量统计口径"
            className="flex min-w-0 flex-col gap-2 sm:flex-row"
        >
            {CALIBER_TABS.map((tab) => {
                const active = tab.value === caliber
                return (
                    <button
                        key={tab.value}
                        id={`customers-quality-caliber-${toAutomationIdSegment(tab.value)}`}
                        role="tab"
                        aria-selected={active}
                        type="button"
                        onClick={() => {
                            if (!active) onChange(tab.value)
                        }}
                        className={cn(
                            "min-w-0 flex-1 rounded-xl border px-4 py-3 text-left transition-colors",
                            active
                                ? "border-primary bg-primary/5"
                                : "border-border hover:border-muted-foreground/40",
                        )}
                    >
                        <div className="truncate text-sm font-semibold">
                            {tab.title}
                        </div>
                        <div className="mt-0.5 truncate text-xs text-muted-foreground">
                            归属口径：{tab.basis}
                        </div>
                    </button>
                )
            })}
        </div>
    )
}
