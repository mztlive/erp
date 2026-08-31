"use client"

import * as React from "react"
import type { DateRange as CalendarDateRange, Matcher } from "react-day-picker"
import { CalendarDaysIcon, ClockIcon, XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Calendar } from "@/components/ui/calendar"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"

type DateRangeValue = {
    from?: string
    to?: string
}

type ZonedDateTimeValue = {
    date: string
    time: string
    timeZone: string
}

function parseDateValue(value?: string) {
    if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return undefined

    const [year, month, day] = value.split("-").map(Number)
    const date = new Date(year, month - 1, day)

    if (
        date.getFullYear() !== year ||
        date.getMonth() !== month - 1 ||
        date.getDate() !== day
    ) {
        return undefined
    }

    return date
}

function formatDateValue(date: Date) {
    const year = date.getFullYear()
    const month = String(date.getMonth() + 1).padStart(2, "0")
    const day = String(date.getDate()).padStart(2, "0")

    return `${year}-${month}-${day}`
}

/** Normalize time to `HH:mm:ss` (seconds default to 00). */
function normalizeTimeValue(time?: string) {
    if (!time) return "00:00:00"
    const parts = time.split(":")
    const hours = (parts[0] ?? "00").padStart(2, "0")
    const minutes = (parts[1] ?? "00").padStart(2, "0")
    const seconds = (parts[2] ?? "00").padStart(2, "0").slice(0, 2)
    return `${hours}:${minutes}:${seconds}`
}

/**
 * Parse a datetime-local / local ISO-ish string into {@link ZonedDateTimeValue}.
 * Accepts `YYYY-MM-DD`, `YYYY-MM-DDTHH:mm`, `YYYY-MM-DDTHH:mm:ss`, and space separator.
 */
function parseDatetimeLocalValue(
    value?: string,
    timeZone = "Asia/Shanghai",
): ZonedDateTimeValue | undefined {
    if (!value) return undefined

    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return { date: value, time: "00:00:00", timeZone }
    }

    const match = value.match(
        /^(\d{4}-\d{2}-\d{2})[T\s](\d{2}:\d{2})(?::(\d{2}))?/,
    )
    if (!match) return undefined

    return {
        date: match[1],
        time: normalizeTimeValue(`${match[2]}:${match[3] ?? "00"}`),
        timeZone,
    }
}

/** Format {@link ZonedDateTimeValue} as `YYYY-MM-DDTHH:mm:ss` (local, no offset). */
function formatDatetimeLocalValue(value?: ZonedDateTimeValue) {
    if (!value?.date) return undefined
    return `${value.date}T${normalizeTimeValue(value.time)}`
}

type DatePickerSize = "sm" | "default" | "lg"

const DATE_PICKER_CLEAR_SIZE = {
    sm: "icon-sm",
    default: "icon",
    lg: "icon-lg",
} as const satisfies Record<DatePickerSize, "icon-sm" | "icon" | "icon-lg">

function DatePicker({
    id,
    value,
    onValueChange,
    placeholder = "选择日期",
    disabled,
    disabledDates,
    clearable = true,
    size = "lg",
    className,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedby,
}: {
    id?: string
    value?: string
    onValueChange?: (value?: string) => void
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    clearable?: boolean
    size?: DatePickerSize
    className?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
}) {
    const [open, setOpen] = React.useState(false)
    const selected = parseDateValue(value)

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <div className={cn("flex min-w-0 items-center gap-1", className)}>
                <PopoverTrigger
                    render={
                        <Button
                            id={id}
                            type="button"
                            variant="outline"
                            size={size}
                            className="min-w-0 flex-1 justify-start rounded-lg bg-surface-control shadow-xs hover:border-foreground/25 hover:bg-card"
                            disabled={disabled}
                            aria-invalid={ariaInvalid}
                            aria-describedby={ariaDescribedby}
                            aria-label={
                                value ? `已选日期 ${value}` : placeholder
                            }
                        />
                    }
                >
                    <CalendarDaysIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    <span
                        className={cn(
                            "truncate",
                            !value && "text-muted-foreground",
                        )}
                    >
                        {value ?? placeholder}
                    </span>
                </PopoverTrigger>
                {clearable && value ? (
                    <Button
                        id={id ? `${id}-clear` : undefined}
                        type="button"
                        variant="ghost"
                        size={DATE_PICKER_CLEAR_SIZE[size]}
                        onClick={() => onValueChange?.(undefined)}
                        disabled={disabled}
                        aria-label="清除日期"
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
            <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                    idPrefix={id ? `${id}-calendar` : undefined}
                    mode="single"
                    selected={selected}
                    disabled={disabledDates}
                    onSelect={(next) => {
                        if (!next) return
                        onValueChange?.(formatDateValue(next))
                        setOpen(false)
                    }}
                />
            </PopoverContent>
        </Popover>
    )
}

function DateRangePicker({
    value,
    onValueChange,
    placeholder = "选择日期范围",
    disabled,
    disabledDates,
    className,
}: {
    value?: DateRangeValue
    onValueChange?: (value?: DateRangeValue) => void
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    className?: string
}) {
    const [open, setOpen] = React.useState(false)
    const selected: CalendarDateRange | undefined = value
        ? {
              from: parseDateValue(value.from),
              to: parseDateValue(value.to),
          }
        : undefined
    const label = value?.from
        ? value.to
            ? `${value.from} — ${value.to}`
            : `${value.from} —`
        : placeholder

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <div className={cn("flex min-w-0 items-center gap-1", className)}>
                <PopoverTrigger
                    render={
                        <Button
                            type="button"
                            variant="outline"
                            size="lg"
                            className="min-w-0 flex-1 justify-start rounded-lg bg-surface-control shadow-xs hover:border-foreground/25 hover:bg-card"
                            disabled={disabled}
                            aria-label={label}
                        />
                    }
                >
                    <CalendarDaysIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    <span
                        className={cn(
                            "truncate",
                            !value?.from && "text-muted-foreground",
                        )}
                    >
                        {label}
                    </span>
                </PopoverTrigger>
                {value?.from ? (
                    <Button
                        type="button"
                        variant="ghost"
                        size="icon-lg"
                        onClick={() => onValueChange?.(undefined)}
                        disabled={disabled}
                        aria-label="清除日期范围"
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
            <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                    mode="range"
                    selected={selected}
                    disabled={disabledDates}
                    onSelect={(next) =>
                        onValueChange?.(
                            next?.from
                                ? {
                                      from: formatDateValue(next.from),
                                      to: next.to
                                          ? formatDateValue(next.to)
                                          : undefined,
                                  }
                                : undefined,
                        )
                    }
                />
            </PopoverContent>
        </Popover>
    )
}

function DateTimePicker({
    id,
    value,
    onValueChange,
    timeZone = "Asia/Shanghai",
    showTimeZone = true,
    placeholder = "选择日期和时间",
    disabled,
    disabledDates,
    clearable = true,
    className,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedby,
}: {
    id?: string
    value?: ZonedDateTimeValue
    onValueChange?: (value?: ZonedDateTimeValue) => void
    timeZone?: string
    showTimeZone?: boolean
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    clearable?: boolean
    className?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
}) {
    const [open, setOpen] = React.useState(false)
    const selected = parseDateValue(value?.date)
    const label = value
        ? `${value.date} ${normalizeTimeValue(value.time)}${
              showTimeZone ? ` · ${value.timeZone}` : ""
          }`
        : placeholder

    const update = (next: Partial<ZonedDateTimeValue>) => {
        const date = next.date ?? value?.date
        if (!date) return

        onValueChange?.({
            date,
            time: normalizeTimeValue(next.time ?? value?.time ?? "00:00:00"),
            timeZone,
        })
    }

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <div className={cn("flex min-w-0 items-center gap-1", className)}>
                <PopoverTrigger
                    render={
                        <Button
                            id={id}
                            type="button"
                            variant="outline"
                            size="lg"
                            className="min-w-0 flex-1 justify-start rounded-lg bg-surface-control shadow-xs hover:border-foreground/25 hover:bg-card"
                            disabled={disabled}
                            aria-invalid={ariaInvalid}
                            aria-describedby={ariaDescribedby}
                            aria-label={label}
                        />
                    }
                >
                    <ClockIcon data-icon="inline-start" aria-hidden="true" />
                    <span
                        className={cn(
                            "truncate",
                            !value && "text-muted-foreground",
                        )}
                    >
                        {label}
                    </span>
                </PopoverTrigger>
                {clearable && value ? (
                    <Button
                        id={id ? `${id}-clear` : undefined}
                        type="button"
                        variant="ghost"
                        size="icon-lg"
                        onClick={() => onValueChange?.(undefined)}
                        disabled={disabled}
                        aria-label="清除日期时间"
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
            <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                    idPrefix={id ? `${id}-calendar` : undefined}
                    mode="single"
                    selected={selected}
                    disabled={disabledDates}
                    onSelect={(next) => {
                        if (!next) return
                        const now = new Date()
                        const pad = (n: number) => String(n).padStart(2, "0")
                        update({
                            date: formatDateValue(next),
                            time:
                                value?.time ||
                                `${pad(now.getHours())}:${pad(now.getMinutes())}:00`,
                        })
                    }}
                />
                <div className="flex items-center gap-2 border-t p-3">
                    <ClockIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden="true"
                    />
                    <Input
                        type="time"
                        step={60}
                        value={normalizeTimeValue(value?.time)}
                        onChange={(event) =>
                            update({ time: event.target.value })
                        }
                        disabled={disabled || !value?.date}
                        aria-label="时间，精确到秒"
                    />
                    {showTimeZone ? (
                        <span className="whitespace-nowrap text-xs text-muted-foreground">
                            {timeZone}
                        </span>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        onClick={() => setOpen(false)}
                        disabled={!value}
                    >
                        完成
                    </Button>
                </div>
            </PopoverContent>
        </Popover>
    )
}

/**
 * DateTimePicker that binds to datetime-local style strings
 * (`YYYY-MM-DDTHH:mm` / `YYYY-MM-DDTHH:mm:ss`).
 */
function DateTimeLocalPicker({
    id,
    value,
    onValueChange,
    timeZone = "Asia/Shanghai",
    showTimeZone = true,
    placeholder = "选择日期和时间",
    disabled,
    disabledDates,
    clearable = true,
    className,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedby,
}: {
    id?: string
    value?: string
    onValueChange?: (value?: string) => void
    timeZone?: string
    showTimeZone?: boolean
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    clearable?: boolean
    className?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
}) {
    return (
        <DateTimePicker
            id={id}
            value={parseDatetimeLocalValue(value, timeZone)}
            onValueChange={(next) =>
                onValueChange?.(
                    next ? formatDatetimeLocalValue(next) : undefined,
                )
            }
            timeZone={timeZone}
            showTimeZone={showTimeZone}
            placeholder={placeholder}
            disabled={disabled}
            disabledDates={disabledDates}
            clearable={clearable}
            className={className}
            aria-invalid={ariaInvalid}
            aria-describedby={ariaDescribedby}
        />
    )
}

/**
 * 日期时间范围。`from` / `to` 为 `YYYY-MM-DDTHH:mm[:ss]` 本地字符串。
 */
type DateTimeRangeValue = {
    from?: string
    to?: string
}

/**
 * 返回当前本地时钟时刻。
 *
 * @returns `HH:mm:00`。
 */
function currentClockTime() {
    const now = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    return `${pad(now.getHours())}:${pad(now.getMinutes())}:00`
}

/**
 * 把起止日期时间收成一行展示文案。
 *
 * @param from 开始日期时间。
 * @param to 结束日期时间。
 * @returns 同一天只重复一次日期；缺结束时带未完成破折号。
 */
function formatZonedRangeLabel(
    from?: ZonedDateTimeValue,
    to?: ZonedDateTimeValue,
) {
    if (!from) return undefined
    const fromTime = normalizeTimeValue(from.time).slice(0, 5)
    if (!to) return `${from.date} ${fromTime} —`
    const toTime = normalizeTimeValue(to.time).slice(0, 5)
    if (from.date === to.date) return `${from.date} ${fromTime} — ${toTime}`
    return `${from.date} ${fromTime} — ${to.date} ${toTime}`
}

/**
 * 日期时间范围选择：日历选起止日期，底部补开始/结束时刻。
 */
function DateTimeRangePicker({
    id,
    value,
    onValueChange,
    timeZone = "Asia/Shanghai",
    showTimeZone = true,
    placeholder = "选择时间范围",
    disabled,
    disabledDates,
    clearable = true,
    className,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedby,
}: {
    id?: string
    value?: { from?: ZonedDateTimeValue; to?: ZonedDateTimeValue }
    onValueChange?: (value?: {
        from?: ZonedDateTimeValue
        to?: ZonedDateTimeValue
    }) => void
    timeZone?: string
    showTimeZone?: boolean
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    clearable?: boolean
    className?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
}) {
    const [open, setOpen] = React.useState(false)
    const selected: CalendarDateRange | undefined = value?.from
        ? {
              from: parseDateValue(value.from.date),
              to: parseDateValue(value.to?.date),
          }
        : undefined
    const label = formatZonedRangeLabel(value?.from, value?.to) ?? placeholder

    /** 把当前范围写回调用方；没有开始日期时视为清空。 */
    const emit = (next: {
        from?: ZonedDateTimeValue
        to?: ZonedDateTimeValue
    }) => {
        if (!next.from) {
            onValueChange?.(undefined)
            return
        }
        onValueChange?.(next)
    }

    return (
        <Popover open={open} onOpenChange={setOpen}>
            <div className={cn("flex min-w-0 items-center gap-1", className)}>
                <PopoverTrigger
                    render={
                        <Button
                            id={id}
                            type="button"
                            variant="outline"
                            size="lg"
                            className="min-w-0 flex-1 justify-start rounded-lg bg-surface-control shadow-xs hover:border-foreground/25 hover:bg-card"
                            disabled={disabled}
                            aria-invalid={ariaInvalid}
                            aria-describedby={ariaDescribedby}
                            aria-label={label}
                        />
                    }
                >
                    <CalendarDaysIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    <span
                        className={cn(
                            "truncate",
                            !value?.from && "text-muted-foreground",
                        )}
                    >
                        {label}
                    </span>
                </PopoverTrigger>
                {clearable && value?.from ? (
                    <Button
                        type="button"
                        variant="ghost"
                        size="icon-lg"
                        onClick={() => onValueChange?.(undefined)}
                        disabled={disabled}
                        aria-label="清除时间范围"
                    >
                        <XIcon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
            <PopoverContent className="w-auto p-0" align="start">
                <Calendar
                    mode="range"
                    selected={selected}
                    disabled={disabledDates}
                    onSelect={(next) => {
                        if (!next?.from) {
                            emit({})
                            return
                        }
                        emit({
                            from: {
                                date: formatDateValue(next.from),
                                time: normalizeTimeValue(
                                    value?.from?.time || currentClockTime(),
                                ),
                                timeZone,
                            },
                            to: next.to
                                ? {
                                      date: formatDateValue(next.to),
                                      time: normalizeTimeValue(
                                          value?.to?.time ||
                                              value?.from?.time ||
                                              currentClockTime(),
                                      ),
                                      timeZone,
                                  }
                                : undefined,
                        })
                    }}
                />
                <div className="grid gap-2 border-t p-3 sm:grid-cols-2">
                    <div className="flex items-center gap-2">
                        <ClockIcon
                            className="size-4 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <Input
                            type="time"
                            step={60}
                            value={normalizeTimeValue(value?.from?.time)}
                            onChange={(event) => {
                                if (!value?.from) return
                                emit({
                                    from: {
                                        ...value.from,
                                        time: event.target.value,
                                    },
                                    to: value.to,
                                })
                            }}
                            disabled={disabled || !value?.from}
                            aria-label="开始时间"
                        />
                    </div>
                    <div className="flex items-center gap-2">
                        <ClockIcon
                            className="size-4 shrink-0 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <Input
                            type="time"
                            step={60}
                            value={normalizeTimeValue(value?.to?.time)}
                            onChange={(event) => {
                                if (!value?.to) return
                                emit({
                                    from: value.from,
                                    to: {
                                        ...value.to,
                                        time: event.target.value,
                                    },
                                })
                            }}
                            disabled={disabled || !value?.to}
                            aria-label="结束时间"
                        />
                    </div>
                    {showTimeZone ? (
                        <p className="text-xs text-muted-foreground sm:col-span-2">
                            {timeZone}
                        </p>
                    ) : null}
                    <Button
                        type="button"
                        size="sm"
                        className="sm:col-span-2"
                        onClick={() => setOpen(false)}
                        disabled={!value?.from || !value?.to}
                    >
                        完成
                    </Button>
                </div>
            </PopoverContent>
        </Popover>
    )
}

/**
 * 日期时间范围选择，绑定 `YYYY-MM-DDTHH:mm[:ss]` 本地字符串。
 */
function DateTimeRangeLocalPicker({
    id,
    value,
    onValueChange,
    timeZone = "Asia/Shanghai",
    showTimeZone = true,
    placeholder = "选择时间范围",
    disabled,
    disabledDates,
    clearable = true,
    className,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedby,
}: {
    id?: string
    value?: DateTimeRangeValue
    onValueChange?: (value?: DateTimeRangeValue) => void
    timeZone?: string
    showTimeZone?: boolean
    placeholder?: string
    disabled?: boolean
    disabledDates?: Matcher | Matcher[]
    clearable?: boolean
    className?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
}) {
    return (
        <DateTimeRangePicker
            id={id}
            value={{
                from: parseDatetimeLocalValue(value?.from, timeZone),
                to: parseDatetimeLocalValue(value?.to, timeZone),
            }}
            onValueChange={(next) =>
                onValueChange?.(
                    next?.from
                        ? {
                              from: formatDatetimeLocalValue(next.from),
                              to: formatDatetimeLocalValue(next.to),
                          }
                        : undefined,
                )
            }
            timeZone={timeZone}
            showTimeZone={showTimeZone}
            placeholder={placeholder}
            disabled={disabled}
            disabledDates={disabledDates}
            clearable={clearable}
            className={className}
            aria-invalid={ariaInvalid}
            aria-describedby={ariaDescribedby}
        />
    )
}

export {
    DatePicker,
    DateRangePicker,
    DateTimePicker,
    DateTimeLocalPicker,
    DateTimeRangeLocalPicker,
    parseDatetimeLocalValue,
    formatDatetimeLocalValue,
    type DateRangeValue,
    type DateTimeRangeValue,
    type ZonedDateTimeValue,
}
