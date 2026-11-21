"use client"

import type { ReactNode } from "react"
import { InfoIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"

export function workbenchTime(value?: string | number, full = false) {
    if (value == null) return "更新时间待返回"
    const date = new Date(typeof value === "number" ? value * 1000 : value)
    if (Number.isNaN(date.getTime())) return "更新时间待返回"
    const day = new Intl.DateTimeFormat("en-CA", { timeZone: "Asia/Shanghai" })
    const showDate = full || day.format(date) !== day.format(new Date())
    return new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        ...(showDate
            ? { year: "numeric", month: "2-digit", day: "2-digit" }
            : {}),
        hour: "2-digit",
        minute: "2-digit",
        hour12: false,
    }).format(date)
}

export function workbenchUpdatedAt(values: (string | number | undefined)[]) {
    const timestamps = values.map((value) =>
        value == null
            ? NaN
            : new Date(
                  typeof value === "number" ? value * 1000 : value,
              ).getTime(),
    )
    const valid = timestamps.filter(Number.isFinite)
    if (!valid.length) return "更新时间待返回"
    if (valid.length !== timestamps.length) return "部分更新时间待返回"
    const earliest = Math.min(...valid)
    return `${earliest === Math.max(...valid) ? "更新于" : "最早更新于"} ${workbenchTime(new Date(earliest).toISOString())}`
}

export function WorkbenchDataInfo({
    id,
    title,
    children,
}: {
    id: string
    title: string
    children: ReactNode
}) {
    return (
        <Popover>
            <PopoverTrigger
                id={id}
                aria-label={`${title}：统计口径`}
                render={
                    <Button
                        variant="ghost"
                        size="icon-sm"
                        className="text-muted-foreground"
                    />
                }
            >
                <InfoIcon aria-hidden="true" />
            </PopoverTrigger>
            <PopoverContent
                align="end"
                className="w-80 max-w-[calc(100vw-2rem)] gap-2"
            >
                <PopoverTitle className="text-section-title font-semibold tracking-tight">
                    {title}
                </PopoverTitle>
                <div className="space-y-2 text-xs text-muted-foreground">
                    {children}
                </div>
                <p className="text-xs text-muted-foreground">
                    业务时区：北京时间（Asia/Shanghai）
                </p>
            </PopoverContent>
        </Popover>
    )
}
