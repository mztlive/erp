"use client"

import * as React from "react"
import {
    useDayPicker,
    type MonthCaptionProps,
    type MonthsProps,
} from "react-day-picker"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

type CalendarView = "days" | "months" | "years"

const CalendarIdContext = React.createContext<string | undefined>(undefined)
const CalendarNavigationContext = React.createContext<{
    open: (view: "months" | "years", date: Date) => void
} | null>(null)

function CalendarNavigationProvider({
    idPrefix,
    children,
}: {
    idPrefix?: string
    children: React.ReactNode
}) {
    return <CalendarIdContext value={idPrefix}>{children}</CalendarIdContext>
}

function CalendarMonthCaption({
    calendarMonth,
    displayIndex: _displayIndex,
    children,
    ...props
}: MonthCaptionProps) {
    const idPrefix = React.useContext(CalendarIdContext)
    const navigation = React.useContext(CalendarNavigationContext)
    const { dayPickerProps } = useDayPicker()
    const date = calendarMonth.date
    const year = date.getFullYear()
    const month = date.getMonth()
    const disabled =
        dayPickerProps.disableNavigation || dayPickerProps.hideNavigation

    return (
        <div {...props}>
            <span className="sr-only">{children}</span>
            <Button
                id={
                    idPrefix
                        ? `${idPrefix}-caption-${year}-${month + 1}-year`
                        : undefined
                }
                type="button"
                variant="ghost"
                size="sm"
                className="h-(--cell-size) px-1.5"
                disabled={disabled}
                aria-label={`选择年份，当前 ${year} 年`}
                onClick={() => navigation?.open("years", date)}
            >
                {year}年
            </Button>
            <Button
                id={
                    idPrefix
                        ? `${idPrefix}-caption-${year}-${month + 1}-month`
                        : undefined
                }
                type="button"
                variant="ghost"
                size="sm"
                className="h-(--cell-size) px-1.5"
                data-calendar-month-trigger
                disabled={disabled}
                aria-label={`选择月份，当前 ${month + 1} 月`}
                onClick={() => navigation?.open("months", date)}
            >
                {month + 1}月
            </Button>
        </div>
    )
}

function CalendarMonths({ children, className, ...props }: MonthsProps) {
    const idPrefix = React.useContext(CalendarIdContext)
    const { dayPickerProps, goToMonth } = useDayPicker()
    const [view, setView] = React.useState<CalendarView>("days")
    const [cursor, setCursor] = React.useState({ year: 0, month: 0 })
    const [yearStart, setYearStart] = React.useState(0)
    const rootRef = React.useRef<HTMLDivElement>(null)
    const panelRef = React.useRef<HTMLDivElement>(null)
    const restoreFocus = React.useRef(false)
    const minYear = dayPickerProps.startMonth?.getFullYear() ?? 1
    const maxYear = dayPickerProps.endMonth?.getFullYear() ?? 9999
    const panelOpen = view !== "days"
    const id = (suffix: string) =>
        idPrefix ? `${idPrefix}-${suffix}` : undefined

    React.useEffect(() => {
        if (view !== "days") {
            const panel = panelRef.current
            const selected = panel?.querySelector<HTMLButtonElement>(
                'button[data-current="true"]:not(:disabled)',
            )
            const first = panel?.querySelector<HTMLButtonElement>(
                "button[data-calendar-option]:not(:disabled)",
            )
            const target = selected ?? first
            target?.focus()
        } else if (restoreFocus.current) {
            rootRef.current
                ?.querySelector<HTMLButtonElement>(
                    "button[data-calendar-month-trigger]",
                )
                ?.focus()
            restoreFocus.current = false
        }
    }, [view])

    function open(nextView: "months" | "years", date: Date) {
        setCursor({ year: date.getFullYear(), month: date.getMonth() })
        setYearStart(Math.floor(date.getFullYear() / 12) * 12)
        setView(nextView)
    }

    function close() {
        restoreFocus.current = true
        setView("days")
    }

    function selectMonth(month: number) {
        // Only change the visible month; DayPicker retains ownership of selection.
        const date = new Date(0)
        date.setFullYear(cursor.year, month, 1)
        date.setHours(0, 0, 0, 0)
        goToMonth(date)
        close()
    }

    function handleOptionKeyDown(
        event: React.KeyboardEvent<HTMLButtonElement>,
    ) {
        const options = Array.from(
            panelRef.current?.querySelectorAll<HTMLButtonElement>(
                "button[data-calendar-option]",
            ) ?? [],
        )
        const index = options.indexOf(event.currentTarget)
        if (index < 0) return
        const direction = dayPickerProps.dir === "rtl" ? -1 : 1
        const step = {
            ArrowLeft: -direction,
            ArrowRight: direction,
            ArrowUp: -3,
            ArrowDown: 3,
        }[event.key]
        if (step === undefined && event.key !== "Home" && event.key !== "End")
            return
        event.preventDefault()
        let next =
            event.key === "Home"
                ? 0
                : event.key === "End"
                  ? options.length - 1
                  : index + (step ?? 0)
        const increment = event.key === "End" ? -1 : (step ?? 1)
        while (next >= 0 && next < options.length) {
            if (!options[next].disabled) {
                options[next].focus()
                break
            }
            next += increment
        }
    }

    const previousDisabled =
        view === "years" ? yearStart <= minYear : cursor.year <= minYear
    const nextDisabled =
        view === "years" ? yearStart + 11 >= maxYear : cursor.year >= maxYear

    return (
        <CalendarNavigationContext value={{ open }}>
            <div ref={rootRef} className="relative">
                <div
                    {...props}
                    className={cn(className, panelOpen && "invisible")}
                    inert={panelOpen || undefined}
                    aria-hidden={panelOpen || undefined}
                >
                    {children}
                </div>
                {panelOpen && (
                    <div
                        ref={panelRef}
                        className="absolute inset-0 flex flex-col gap-2"
                        role="group"
                        aria-label={view === "years" ? "选择年份" : "选择月份"}
                        onKeyDownCapture={(event) => {
                            if (event.key !== "Escape") return
                            event.preventDefault()
                            event.stopPropagation()
                            if (view === "years") setView("months")
                            else close()
                        }}
                    >
                        <div className="flex h-(--cell-size) items-center justify-between gap-1">
                            <Button
                                id={id("panel-previous")}
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className="size-(--cell-size)"
                                disabled={previousDisabled}
                                aria-label={
                                    view === "years" ? "上一组年份" : "上一年"
                                }
                                onClick={() => {
                                    if (view === "years")
                                        setYearStart(yearStart - 12)
                                    else
                                        setCursor({
                                            ...cursor,
                                            year: cursor.year - 1,
                                        })
                                }}
                            >
                                <ChevronLeftIcon className="rtl:rotate-180" />
                            </Button>
                            {view === "years" ? (
                                <span
                                    className="text-sm font-medium"
                                    aria-live="polite"
                                >
                                    {Math.max(yearStart, minYear)}—
                                    {Math.min(yearStart + 11, maxYear)}年
                                </span>
                            ) : (
                                <Button
                                    id={id("panel-year")}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className="h-(--cell-size)"
                                    aria-label={`选择年份，当前 ${cursor.year} 年`}
                                    onClick={() => {
                                        setYearStart(
                                            Math.floor(cursor.year / 12) * 12,
                                        )
                                        setView("years")
                                    }}
                                >
                                    {cursor.year}年
                                </Button>
                            )}
                            <Button
                                id={id("panel-next")}
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className="size-(--cell-size)"
                                disabled={nextDisabled}
                                aria-label={
                                    view === "years" ? "下一组年份" : "下一年"
                                }
                                onClick={() => {
                                    if (view === "years")
                                        setYearStart(yearStart + 12)
                                    else
                                        setCursor({
                                            ...cursor,
                                            year: cursor.year + 1,
                                        })
                                }}
                            >
                                <ChevronRightIcon className="rtl:rotate-180" />
                            </Button>
                        </div>
                        <div
                            className="grid min-h-0 flex-1 grid-cols-3 grid-rows-4 gap-1"
                            role="group"
                            aria-label={view === "years" ? "年份" : "月份"}
                        >
                            {Array.from({ length: 12 }, (_, index) => {
                                const year =
                                    view === "years"
                                        ? yearStart + index
                                        : cursor.year
                                const month = index
                                const current =
                                    view === "years"
                                        ? year === cursor.year
                                        : month === cursor.month
                                const disabled =
                                    year < minYear ||
                                    year > maxYear ||
                                    (view === "months" &&
                                        ((year === minYear &&
                                            month <
                                                (dayPickerProps.startMonth?.getMonth() ??
                                                    0)) ||
                                            (year === maxYear &&
                                                month >
                                                    (dayPickerProps.endMonth?.getMonth() ??
                                                        11))))
                                return (
                                    <Button
                                        key={view === "years" ? year : month}
                                        id={id(
                                            view === "years"
                                                ? `year-${year}`
                                                : `month-${year}-${month + 1}`,
                                        )}
                                        type="button"
                                        variant={
                                            current ? "secondary" : "ghost"
                                        }
                                        className="h-full min-h-(--cell-size) px-1"
                                        data-calendar-option
                                        data-current={current}
                                        onKeyDown={handleOptionKeyDown}
                                        disabled={disabled}
                                        aria-label={
                                            view === "years"
                                                ? `${year}年`
                                                : `${year}年${month + 1}月`
                                        }
                                        onClick={() => {
                                            if (view === "years") {
                                                setCursor({ ...cursor, year })
                                                setView("months")
                                            } else selectMonth(month)
                                        }}
                                    >
                                        {view === "years"
                                            ? `${year}年`
                                            : `${month + 1}月`}
                                    </Button>
                                )
                            })}
                        </div>
                        <Button
                            id={id("panel-back")}
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={close}
                        >
                            返回日历
                        </Button>
                    </div>
                )}
            </div>
        </CalendarNavigationContext>
    )
}

export { CalendarNavigationProvider, CalendarMonths, CalendarMonthCaption }
