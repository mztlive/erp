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
  timeZone = "Asia/Shanghai"
): ZonedDateTimeValue | undefined {
  if (!value) return undefined

  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
    return { date: value, time: "00:00:00", timeZone }
  }

  const match = value.match(
    /^(\d{4}-\d{2}-\d{2})[T\s](\d{2}:\d{2})(?::(\d{2}))?/
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

function DatePicker({
  value,
  onValueChange,
  placeholder = "选择日期",
  disabled,
  disabledDates,
  clearable = true,
  className,
  "aria-invalid": ariaInvalid,
}: {
  value?: string
  onValueChange?: (value?: string) => void
  placeholder?: string
  disabled?: boolean
  disabledDates?: Matcher | Matcher[]
  clearable?: boolean
  className?: string
  "aria-invalid"?: boolean
}) {
  const [open, setOpen] = React.useState(false)
  const selected = parseDateValue(value)

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <div className={cn("flex min-w-0 items-center gap-1", className)}>
        <PopoverTrigger
          render={
            <Button
              type="button"
              variant="outline"
              className="min-w-0 flex-1 justify-start"
              disabled={disabled}
              aria-invalid={ariaInvalid}
              aria-label={value ? `已选日期 ${value}` : placeholder}
            />
          }
        >
          <CalendarDaysIcon data-icon="inline-start" aria-hidden="true" />
          <span className={cn("truncate", !value && "text-muted-foreground")}>
            {value ?? placeholder}
          </span>
        </PopoverTrigger>
        {clearable && value ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
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
          mode="single"
          selected={selected}
          disabled={disabledDates}
          onSelect={(next) => {
            onValueChange?.(next ? formatDateValue(next) : undefined)
            if (next) setOpen(false)
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
              className="min-w-0 flex-1 justify-start"
              disabled={disabled}
              aria-label={label}
            />
          }
        >
          <CalendarDaysIcon data-icon="inline-start" aria-hidden="true" />
          <span className={cn("truncate", !value?.from && "text-muted-foreground")}>
            {label}
          </span>
        </PopoverTrigger>
        {value?.from ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
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
                    to: next.to ? formatDateValue(next.to) : undefined,
                  }
                : undefined
            )
          }
        />
      </PopoverContent>
    </Popover>
  )
}

function DateTimePicker({
  value,
  onValueChange,
  timeZone = "Asia/Shanghai",
  placeholder = "选择日期和时间",
  disabled,
  disabledDates,
  clearable = true,
  className,
  "aria-invalid": ariaInvalid,
}: {
  value?: ZonedDateTimeValue
  onValueChange?: (value?: ZonedDateTimeValue) => void
  timeZone?: string
  placeholder?: string
  disabled?: boolean
  disabledDates?: Matcher | Matcher[]
  clearable?: boolean
  className?: string
  "aria-invalid"?: boolean
}) {
  const [open, setOpen] = React.useState(false)
  const selected = parseDateValue(value?.date)
  const label = value
    ? `${value.date} ${normalizeTimeValue(value.time)} · ${value.timeZone}`
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
              type="button"
              variant="outline"
              className="min-w-0 flex-1 justify-start"
              disabled={disabled}
              aria-invalid={ariaInvalid}
              aria-label={label}
            />
          }
        >
          <ClockIcon data-icon="inline-start" aria-hidden="true" />
          <span className={cn("truncate", !value && "text-muted-foreground")}>
            {label}
          </span>
        </PopoverTrigger>
        {clearable && value ? (
          <Button
            type="button"
            variant="ghost"
            size="icon"
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
          mode="single"
          selected={selected}
          disabled={disabledDates}
          onSelect={(next) => {
            if (next) update({ date: formatDateValue(next) })
          }}
        />
        <div className="flex items-center gap-2 border-t p-3">
          <ClockIcon className="size-4 text-muted-foreground" aria-hidden="true" />
          <Input
            type="time"
            step={1}
            value={normalizeTimeValue(value?.time)}
            onChange={(event) => update({ time: event.target.value })}
            disabled={disabled || !value?.date}
            aria-label="时间，精确到秒"
          />
          <span className="whitespace-nowrap text-xs text-muted-foreground">
            {timeZone}
          </span>
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
  value,
  onValueChange,
  timeZone = "Asia/Shanghai",
  placeholder = "选择日期和时间",
  disabled,
  disabledDates,
  clearable = true,
  className,
  "aria-invalid": ariaInvalid,
}: {
  value?: string
  onValueChange?: (value?: string) => void
  timeZone?: string
  placeholder?: string
  disabled?: boolean
  disabledDates?: Matcher | Matcher[]
  clearable?: boolean
  className?: string
  "aria-invalid"?: boolean
}) {
  return (
    <DateTimePicker
      value={parseDatetimeLocalValue(value, timeZone)}
      onValueChange={(next) =>
        onValueChange?.(next ? formatDatetimeLocalValue(next) : undefined)
      }
      timeZone={timeZone}
      placeholder={placeholder}
      disabled={disabled}
      disabledDates={disabledDates}
      clearable={clearable}
      className={className}
      aria-invalid={ariaInvalid}
    />
  )
}

export {
  DatePicker,
  DateRangePicker,
  DateTimePicker,
  DateTimeLocalPicker,
  parseDatetimeLocalValue,
  formatDatetimeLocalValue,
  type DateRangeValue,
  type ZonedDateTimeValue,
}
