"use client"

import { UserIcon, UsersIcon } from "lucide-react"

import { cn } from "@/lib/utils"
import { sequentialText } from "@/lib/ui-text"

/*
  分段切换：轨道 + 图标 + 选中白底浮起 / 未选中弱字色，
  明确是「二选一控件」而不是静态标签。
*/
export function ScopeSwitcher({
    scope,
    onScopeChange,
}: {
    scope: "mine" | "team"
    onScopeChange: (scope: "mine" | "team") => void
}) {
    return (
        <div
            role="group"
            aria-label="责任范围"
            className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
        >
            <button
                type="button"
                aria-pressed={scope === "mine"}
                onClick={() => onScopeChange("mine")}
                className={cn(
                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    scope === "mine"
                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                )}
            >
                <UserIcon className="size-3.5 shrink-0" aria-hidden="true" />
                我的待办
            </button>
            <button
                type="button"
                aria-pressed={scope === "team"}
                onClick={() => onScopeChange("team")}
                className={cn(
                    "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    scope === "team"
                        ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                        : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                )}
            >
                <UsersIcon className="size-3.5 shrink-0" aria-hidden="true" />
                {sequentialText.teamPending}
            </button>
        </div>
    )
}
