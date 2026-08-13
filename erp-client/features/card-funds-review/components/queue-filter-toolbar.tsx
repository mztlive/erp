"use client"

import { SearchIcon } from "lucide-react"

import { ListToolbar, surfacePanelClassName } from "@/components/business"
import { cn } from "@/lib/utils"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

export function QueueFilterToolbar({
    scope,
    type,
    due,
    status,
    searchInput,
    onSearchInputChange,
    autoNext,
    setAutoNext,
    replaceUrl,
}: {
    scope: "mine" | "role_pool"
    type: "all" | "opening" | "delta"
    due: "all" | "today" | "overdue"
    status: "pending" | "held"
    searchInput: string
    onSearchInputChange: (value: string) => void
    autoNext: boolean
    setAutoNext: (on: boolean) => void
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
}) {
    return (
        <div
            className={`${surfacePanelClassName} sticky top-0 z-10 space-y-2.5 px-3 py-2.5 text-sm`}
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
                    ).map((opt) => (
                        <button
                            key={opt.value}
                            type="button"
                            aria-pressed={scope === opt.value}
                            onClick={() =>
                                replaceUrl({
                                    scope:
                                        opt.value === "mine" ? null : opt.value,
                                    queueContextId: null,
                                    currentWorkItemId: null,
                                })
                            }
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                scope === opt.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {opt.label}
                        </button>
                    ))}
                </div>
                <div
                    role="group"
                    aria-label="任务类型"
                    className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                >
                    {(
                        [
                            { value: "all" as const, label: "全部类型" },
                            { value: "opening" as const, label: "期初" },
                            { value: "delta" as const, label: "同步差额" },
                        ] as const
                    ).map((opt) => (
                        <button
                            key={opt.value}
                            type="button"
                            aria-pressed={type === opt.value}
                            onClick={() =>
                                replaceUrl({
                                    type: opt.value,
                                    currentWorkItemId: null,
                                })
                            }
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                type === opt.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {opt.label}
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
                            { value: "all" as const, label: "全部时限" },
                            { value: "today" as const, label: "今日到期" },
                            { value: "overdue" as const, label: "已超期" },
                        ] as const
                    ).map((opt) => (
                        <button
                            key={opt.value}
                            type="button"
                            aria-pressed={due === opt.value}
                            onClick={() =>
                                replaceUrl({
                                    due: opt.value === "all" ? null : opt.value,
                                    currentWorkItemId: null,
                                })
                            }
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                due === opt.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {opt.label}
                        </button>
                    ))}
                </div>
                <div
                    role="group"
                    aria-label="队列范围"
                    className="inline-flex items-center rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                >
                    {(
                        [
                            { value: "pending" as const, label: "待处理" },
                            { value: "held" as const, label: "已跳过" },
                        ] as const
                    ).map((opt) => (
                        <button
                            key={opt.value}
                            type="button"
                            aria-pressed={status === opt.value}
                            onClick={() =>
                                replaceUrl({
                                    status:
                                        opt.value === "pending"
                                            ? null
                                            : opt.value,
                                    currentWorkItemId: null,
                                })
                            }
                            className={cn(
                                "inline-flex h-7 items-center gap-1.5 rounded-md px-2.5 text-sm transition-all outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                status === opt.value
                                    ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                                    : "font-normal text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
                            )}
                        >
                            {opt.label}
                        </button>
                    ))}
                </div>
            </div>
            <ListToolbar
                aria-label="票款复核筛选"
                search={
                    <InputGroup>
                        <InputGroupAddon>
                            <SearchIcon className="size-4" aria-hidden="true" />
                        </InputGroupAddon>
                        <InputGroupInput
                            value={searchInput}
                            onChange={(e) =>
                                onSearchInputChange(e.target.value)
                            }
                            placeholder="搜索单号 / 客户 / 往来主体"
                            aria-label="搜索复核队列"
                        />
                    </InputGroup>
                }
                actions={
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
                            onCheckedChange={setAutoNext}
                        />
                    </div>
                }
            />
        </div>
    )
}
