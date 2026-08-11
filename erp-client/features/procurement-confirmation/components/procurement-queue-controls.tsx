"use client"

import type { Ref } from "react"
import { SearchIcon } from "lucide-react"

import { ListToolbar, surfacePanelClassName } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

type ProcurementQueueControlsProps = {
    scope: "mine" | "role_pool"
    due: "active" | "today" | "overdue"
    orderNoInputRef: Ref<HTMLInputElement>
    orderNoDraft: string
    onOrderNoDraftChange: (value: string) => void
    onCommitOrderNo: () => void
    hasActiveFilter: boolean
    onClearFilters: () => void
    autoNext: boolean
    onToggleAutoNext: (value: boolean) => void
    onScopeChange: (value: "mine" | "role_pool") => void
    onDueChange: (value: "active" | "today" | "overdue") => void
}

export function ProcurementQueueControls({
    scope,
    due,
    orderNoInputRef,
    orderNoDraft,
    onOrderNoDraftChange,
    onCommitOrderNo,
    hasActiveFilter,
    onClearFilters,
    autoNext,
    onToggleAutoNext,
    onScopeChange,
    onDueChange,
}: ProcurementQueueControlsProps) {
    return (
        <div
            className={cn(
                surfacePanelClassName,
                "sticky top-0 z-10 space-y-2.5 px-3 py-2.5 text-sm",
            )}
        >
            <div className="flex flex-wrap items-center gap-3">
                <div
                    role="group"
                    aria-label="责任范围"
                    className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                >
                    {(
                        [
                            { value: "mine" as const, label: "我的待办" },
                            {
                                value: "role_pool" as const,
                                label: "团队待认领",
                            },
                        ] as const
                    ).map((option) => (
                        <button
                            key={option.value}
                            type="button"
                            aria-pressed={scope === option.value}
                            onClick={() => onScopeChange(option.value)}
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                scope === option.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {option.label}
                        </button>
                    ))}
                </div>
                <div
                    role="group"
                    aria-label="到期时限"
                    className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                >
                    {(
                        [
                            { value: "active" as const, label: "全部时限" },
                            { value: "today" as const, label: "今日到期" },
                            { value: "overdue" as const, label: "已超期" },
                        ] as const
                    ).map((option) => (
                        <button
                            key={option.value}
                            type="button"
                            aria-pressed={due === option.value}
                            onClick={() => onDueChange(option.value)}
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                due === option.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {option.label}
                        </button>
                    ))}
                </div>
                <span className="ml-auto hidden text-xs text-muted-foreground md:inline">
                    快捷键：j / k 切换 · ⌘S 保存 · ⌘↵ 通过确认 · / 按单号搜索
                </span>
            </div>
            <ListToolbar
                aria-label="二次确认筛选"
                search={
                    <form
                        onSubmit={(event) => {
                            event.preventDefault()
                            onCommitOrderNo()
                        }}
                    >
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={orderNoInputRef}
                                value={orderNoDraft}
                                onChange={(event) =>
                                    onOrderNoDraftChange(event.target.value)
                                }
                                placeholder="按销售单号搜索"
                                aria-label="按单号搜索队列"
                            />
                        </InputGroup>
                    </form>
                }
                actions={
                    <>
                        {hasActiveFilter ? (
                            <Button
                                type="button"
                                variant="ghost"
                                size="sm"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                        <div className="flex items-center gap-2">
                            <Label
                                htmlFor="auto-next"
                                className="text-muted-foreground"
                            >
                                自动下一项
                            </Label>
                            <Switch
                                id="auto-next"
                                checked={autoNext}
                                onCheckedChange={onToggleAutoNext}
                                aria-describedby="auto-next-hint"
                            />
                            <span id="auto-next-hint" className="sr-only">
                                该偏好仅在本次操作内生效
                            </span>
                        </div>
                        <Badge variant="outline" className="font-normal">
                            仅本次会话
                        </Badge>
                    </>
                }
            />
        </div>
    )
}
