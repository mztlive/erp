"use client"

import {
    useEffect,
    useImperativeHandle,
    useRef,
    useState,
    type ReactNode,
    type Ref,
} from "react"
import { cn } from "@/lib/utils"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    CircleHelpIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"
import { WorkspaceFactGrid, WorkspaceFactItem } from "@/components/business"
import { MoneyValue } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"

export const returnFinanceDecimal = { maxScale: 2, outputScale: 2 }

type FinanceAmount = { label: string; value: string }

function FinanceMoney({
    value,
    className,
}: {
    value: string
    className?: string
}) {
    return (
        <MoneyValue
            value={value}
            display="exact"
            className={cn(
                "max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal",
                className,
            )}
        />
    )
}

export function ReturnFinanceAmounts({
    amounts,
}: {
    amounts: readonly FinanceAmount[]
}) {
    return (
        <dl className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-0.5 text-xs">
            {amounts.map(({ label, value }) => (
                <div
                    key={label}
                    className="flex min-w-0 flex-wrap items-baseline gap-x-1.5"
                >
                    <dt className="text-muted-foreground">{label}</dt>
                    <dd className="font-medium">
                        <FinanceMoney value={value} />
                    </dd>
                </div>
            ))}
        </dl>
    )
}

export function ReturnFinanceSummary({
    idPrefix,
    invoiceTask,
    pending,
    total,
    settled,
    balances,
    physicalPending,
    decisionStatus,
    targetGross,
    returnNumbers,
    confirmation,
    refundPlan,
}: {
    idPrefix: string
    invoiceTask: boolean
    pending: string
    total: string
    settled: string
    balances: readonly FinanceAmount[]
    physicalPending: boolean | undefined
    decisionStatus: { label: string; tone: StatusTone }
    targetGross?: string
    returnNumbers: readonly string[]
    confirmation?: string
    refundPlan: readonly FinanceAmount[]
}) {
    const pendingLabel = invoiceTask ? "待冲红金额" : "待退款金额"
    const totalLabel = invoiceTask ? "原蓝票分配总额" : "应退／留抵总额"
    const settledLabel = invoiceTask ? "已冲红" : "已结清"
    const safeSettled = clampZeroFixed(settled, returnFinanceDecimal)
    const fill =
        compareDecimal(total, "0", 2) > 0
            ? multiplyFixed(
                  divideFixed(
                      minFixed(safeSettled, total, returnFinanceDecimal),
                      total,
                      {
                          numeratorMaxScale: 2,
                          denominatorMaxScale: 2,
                          outputScale: 5,
                      },
                  ),
                  "100",
                  { leftMaxScale: 5, rightMaxScale: 0, outputScale: 3 },
              )
            : "0"
    const fillPercent =
        compareDecimal(total, "0", 2) > 0
            ? multiplyFixed(
                  divideFixed(
                      minFixed(safeSettled, total, returnFinanceDecimal),
                      total,
                      {
                          numeratorMaxScale: 2,
                          denominatorMaxScale: 2,
                          outputScale: 4,
                      },
                  ),
                  "100",
                  { leftMaxScale: 4, rightMaxScale: 0, outputScale: 0 },
              )
            : "0"
    const availableTotal = sumFixed(
        balances.map(({ value }) => value),
        returnFinanceDecimal,
    )
    const hasPending = compareDecimal(pending, "0", 2) > 0
    const shortfall =
        hasPending && compareDecimal(pending, availableTotal, 2) > 0
            ? subtractFixed(pending, availableTotal, returnFinanceDecimal)
            : null

    return (
        <section className="space-y-3" aria-label="退货财务金额与办理状态">
            <TooltipProvider>
                <dl className="grid grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/return-finance:grid-cols-2 @[840px]/return-finance:grid-cols-4">
                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>{pendingLabel}</span>
                            <span className="text-tiny text-muted-foreground/80">
                                含税
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    render={
                                        <Button
                                            id={`${idPrefix}-amount-formula`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${pendingLabel}计算口径`}
                                        />
                                    }
                                >
                                    <CircleHelpIcon aria-hidden="true" />
                                </TooltipTrigger>
                                <TooltipContent>
                                    {invoiceTask
                                        ? "待冲红金额 = max(0，本采购已分配进项票额 − 退货后采购含税目标额)"
                                        : "待退款金额 = max(0，本采购已付分配额 − 退货后采购含税目标额)，含退款与留抵尚未入账的部分"}
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd>
                            <FinanceMoney
                                value={pending}
                                className="text-value-section font-semibold tracking-tight"
                            />
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>{totalLabel}</span>
                            <span className="text-tiny text-muted-foreground/80">
                                结清进度
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    render={
                                        <Button
                                            id={`${idPrefix}-progress-formula`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${totalLabel}与结清进度口径`}
                                        />
                                    }
                                >
                                    <CircleHelpIcon aria-hidden="true" />
                                </TooltipTrigger>
                                <TooltipContent>
                                    {invoiceTask
                                        ? "结清进度 = 本采购蓝票已冲红金额 ÷ 原蓝票分配总额"
                                        : "结清进度 = 当前方案已入账退款及留抵 ÷ 方案应退与留抵总额"}
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="space-y-1">
                            <FinanceMoney
                                value={total}
                                className="text-value-compact font-semibold"
                            />
                            <div
                                className="h-1.5 overflow-hidden rounded-full bg-muted"
                                aria-hidden="true"
                            >
                                <div
                                    className="h-full rounded-full bg-success"
                                    style={{ width: `${fill}%` }}
                                />
                            </div>
                            <p className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                                <span className="inline-flex items-center gap-1">
                                    <span
                                        className="size-1.5 rounded-full bg-success"
                                        aria-hidden="true"
                                    />
                                    {settledLabel}{" "}
                                    <FinanceMoney value={safeSettled} />
                                </span>
                                <span className="num text-tiny">
                                    {fillPercent}%
                                </span>
                            </p>
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>
                                {invoiceTask ? "蓝票可冲余额" : "可提交余额"}
                            </span>
                            <span className="text-tiny text-muted-foreground/80">
                                {invoiceTask ? "未红冲蓝票" : "退款 + 留抵"}
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    render={
                                        <Button
                                            id={`${idPrefix}-balance-formula`}
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label="可用余额计算口径"
                                        />
                                    }
                                >
                                    <CircleHelpIcon aria-hidden="true" />
                                </TooltipTrigger>
                                <TooltipContent>
                                    {invoiceTask
                                        ? "蓝票可冲余额 = 本采购有效蓝票剩余可冲金额之和；若低于待冲红金额需核对原票分配。"
                                        : "可提交余额 = 当前退货方案剩余现金退款余额 + 剩余转留抵余额（均不含审批中金额）。"}
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="space-y-1">
                            <FinanceMoney
                                value={availableTotal}
                                className="text-value-compact font-semibold"
                            />
                            {!invoiceTask ? (
                                <p className="flex flex-wrap items-baseline gap-x-2.5 gap-y-0.5 text-xs text-muted-foreground">
                                    {balances.map(({ label, value }) => (
                                        <span key={label}>
                                            {label}{" "}
                                            <FinanceMoney value={value} />
                                        </span>
                                    ))}
                                </p>
                            ) : null}
                            <p
                                className={cn(
                                    "num text-xs font-medium",
                                    !hasPending
                                        ? "text-success-soft-foreground"
                                        : shortfall
                                          ? "text-warning-soft-foreground"
                                          : "text-success-soft-foreground",
                                )}
                            >
                                {!hasPending ? (
                                    "已结清，无需再办"
                                ) : shortfall ? (
                                    <>
                                        缺口 <FinanceMoney value={shortfall} />
                                    </>
                                ) : (
                                    "余额充足，可覆盖待办"
                                )}
                            </p>
                        </dd>
                    </div>
                    <div className="min-w-0 space-y-1">
                        <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                            <span>实物与处理方案</span>
                            <span className="text-tiny text-muted-foreground/80">
                                办理门禁
                            </span>
                        </dt>
                        <dd className="flex flex-wrap items-start gap-x-3 gap-y-1 pt-0.5 text-body-compact">
                            <StatusBadge
                                appearance="text"
                                label={
                                    physicalPending === undefined
                                        ? "实物待核对"
                                        : physicalPending
                                          ? "实物待退"
                                          : "实物退货已核对"
                                }
                                tone={
                                    physicalPending === false
                                        ? "success"
                                        : "warning"
                                }
                            />
                            <StatusBadge
                                appearance="text"
                                {...decisionStatus}
                            />
                        </dd>
                    </div>
                </dl>
            </TooltipProvider>

            {targetGross !== undefined ||
            refundPlan.length > 0 ||
            returnNumbers.length > 0 ||
            Boolean(confirmation)
                ? (() => {
                      const shortCount =
                          (targetGross !== undefined ? 1 : 0) +
                          refundPlan.length +
                          (returnNumbers.length > 0 && returnNumbers.length <= 2
                              ? 1
                              : 0) +
                          (confirmation &&
                          !(
                              confirmation.length > 30 ||
                              confirmation.includes("\n")
                          )
                              ? 1
                              : 0)
                      return (
                          <div className="border-b border-border/60 pb-3.5">
                              <WorkspaceFactGrid
                                  columns={
                                      shortCount > 0 && shortCount <= 2 ? 2 : 3
                                  }
                                  ariaLabel="退货方案与单据依据"
                              >
                                  {targetGross !== undefined ? (
                                      <WorkspaceFactItem
                                          label="退货后采购含税目标额"
                                          numeric
                                      >
                                          <FinanceMoney value={targetGross} />
                                      </WorkspaceFactItem>
                                  ) : null}
                                  {refundPlan.map(({ label, value }) => (
                                      <WorkspaceFactItem
                                          key={label}
                                          label={label}
                                          numeric
                                      >
                                          <FinanceMoney value={value} />
                                      </WorkspaceFactItem>
                                  ))}
                                  {returnNumbers.length > 0 ? (
                                      <WorkspaceFactItem
                                          label="退货单"
                                          value={returnNumbers.join("、")}
                                          numeric
                                          selectable
                                          span={
                                              returnNumbers.length > 2
                                                  ? "full"
                                                  : 1
                                          }
                                      />
                                  ) : null}
                                  {confirmation ? (
                                      <WorkspaceFactItem
                                          label="供应商确认"
                                          value={confirmation}
                                          span={
                                              confirmation.length > 30 ||
                                              confirmation.includes("\n")
                                                  ? "full"
                                                  : 1
                                          }
                                      />
                                  ) : null}
                              </WorkspaceFactGrid>
                          </div>
                      )
                  })()
                : null}
        </section>
    )
}

export type ReturnFinanceTableRow = {
    key: string
    documentNo: string
    documentLink?: ReactNode
    detail?: string
    status?: { label: string; tone: StatusTone }
    amount: string
    balance?: string
    action?: ReactNode
}

export type ReturnFinanceTableHandle = {
    locateFirst: () => void
}

const PAGE_SIZE = 20
const PULSE_DURATION_MS = 2200
const amountCellClass = "wrap-anywhere whitespace-normal"

export function ReturnFinanceTable({
    id,
    title,
    amountLabel,
    balanceLabel,
    showActions = false,
    rows,
    emptyText,
    ref,
}: {
    id: string
    title: string
    amountLabel: string
    balanceLabel?: string
    showActions?: boolean
    rows: readonly ReturnFinanceTableRow[]
    emptyText: string
    ref?: Ref<ReturnFinanceTableHandle>
}) {
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(1)
    const [pulse, setPulse] = useState(false)
    const [locateSeq, setLocateSeq] = useState(0)
    const firstRowRef = useRef<HTMLTableRowElement>(null)
    const searchInputRef = useRef<HTMLInputElement>(null)
    const showFilter = rows.length > 8
    const query = showFilter ? search.trim().toLowerCase() : ""
    const filtered = query
        ? rows.filter((row) =>
              [row.documentNo, row.detail ?? "", row.status?.label ?? ""].some(
                  (value) => value.toLowerCase().includes(query),
              ),
          )
        : rows
    const pageCount = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
    const currentPage = Math.min(pageCount, page)
    const visible = filtered.slice(
        (currentPage - 1) * PAGE_SIZE,
        currentPage * PAGE_SIZE,
    )
    const total = sumFixed(
        rows.map((row) => row.amount),
        returnFinanceDecimal,
    )
    const balanceTotal = balanceLabel
        ? sumFixed(
              rows.map((row) => row.balance ?? "0"),
              returnFinanceDecimal,
          )
        : null
    const columns = 2 + (balanceLabel ? 1 : 0) + (showActions ? 1 : 0)

    useImperativeHandle(
        ref,
        () => ({
            locateFirst() {
                setSearch("")
                setPage(1)
                setPulse(true)
                setLocateSeq((seq) => seq + 1)
            },
        }),
        [],
    )

    useEffect(() => {
        if (locateSeq === 0) return
        let frame2: number | null = null
        const frame1 = requestAnimationFrame(() => {
            frame2 = requestAnimationFrame(() => {
                const target =
                    firstRowRef.current ?? document.getElementById(id)
                if (!target) return
                const reducedMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                target.focus({ preventScroll: true })
                target.scrollIntoView({
                    behavior: reducedMotion ? "instant" : "smooth",
                    block: "center",
                })
            })
        })
        const timer = window.setTimeout(
            () => setPulse(false),
            PULSE_DURATION_MS,
        )
        return () => {
            cancelAnimationFrame(frame1)
            if (frame2 !== null) cancelAnimationFrame(frame2)
            window.clearTimeout(timer)
        }
    }, [id, locateSeq])

    return (
        <section className="min-w-0 space-y-2" aria-labelledby={id}>
            <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
                <h3
                    id={id}
                    tabIndex={-1}
                    className="text-section-title font-semibold"
                >
                    {title}
                </h3>
                {showFilter && (
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                        {query ? (
                            <Button
                                id={`${id}-clear-filter`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                onClick={() => {
                                    setSearch("")
                                    setPage(1)
                                }}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                        <div className="relative flex w-full max-w-56 items-center">
                            <SearchIcon
                                className="pointer-events-none absolute left-2.5 size-3.5 text-muted-foreground"
                                aria-hidden="true"
                            />
                            <Input
                                ref={searchInputRef}
                                id={`${id}-filter`}
                                type="text"
                                className="h-control-sm w-full pr-7 pl-8 text-xs"
                                aria-label={`筛选${title}`}
                                placeholder="按单号或状态筛选"
                                value={search}
                                onChange={(event) => {
                                    setSearch(event.target.value)
                                    setPage(1)
                                }}
                            />
                            {search ? (
                                <Button
                                    id={`${id}-filter-clear`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={`清除${title}筛选`}
                                    className="absolute right-0.5"
                                    onClick={() => {
                                        setSearch("")
                                        setPage(1)
                                        searchInputRef.current?.focus()
                                    }}
                                >
                                    <XIcon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                )}
            </div>
            {rows.length === 0 ? (
                <p className="py-2 text-xs text-muted-foreground">
                    {emptyText}
                </p>
            ) : (
                <>
                    <Table aria-labelledby={id} className="table-fixed">
                        <TableHeader className="bg-surface-sunken [&_tr]:border-0">
                            <TableRow>
                                <TableHead
                                    scope="col"
                                    className="rounded-l-md bg-surface-sunken whitespace-normal"
                                >
                                    单号
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    data-align="end"
                                    className="w-[23%] bg-surface-sunken whitespace-normal last:rounded-r-md"
                                >
                                    {amountLabel}
                                </TableHead>
                                {balanceLabel && (
                                    <TableHead
                                        scope="col"
                                        data-align="end"
                                        className="w-[23%] bg-surface-sunken whitespace-normal last:rounded-r-md"
                                    >
                                        {balanceLabel}
                                    </TableHead>
                                )}
                                {showActions && (
                                    <TableHead
                                        scope="col"
                                        data-align="end"
                                        className="w-[26%] rounded-r-md bg-surface-sunken whitespace-normal"
                                    >
                                        操作
                                    </TableHead>
                                )}
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {visible.map((row, index) => {
                                const isTarget = pulse && index === 0
                                return (
                                    <TableRow
                                        key={row.key}
                                        ref={
                                            index === 0
                                                ? firstRowRef
                                                : undefined
                                        }
                                        tabIndex={isTarget ? -1 : undefined}
                                        className={cn(
                                            isTarget &&
                                                "bg-info-soft/50 ring-1 ring-inset ring-info-border motion-safe:animate-pulse",
                                        )}
                                    >
                                        <TableCell className="py-1.5 wrap-anywhere whitespace-normal">
                                            {row.documentLink ?? (
                                                <span className="num select-all">
                                                    {row.documentNo}
                                                </span>
                                            )}
                                            {row.detail && (
                                                <p className="mt-0.5 text-tiny text-muted-foreground">
                                                    {row.detail}
                                                </p>
                                            )}
                                            {row.status && (
                                                <StatusBadge
                                                    appearance="text"
                                                    className="mt-0.5 flex w-fit whitespace-normal"
                                                    {...row.status}
                                                />
                                            )}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className={amountCellClass}
                                        >
                                            <FinanceMoney value={row.amount} />
                                        </TableCell>
                                        {balanceLabel && (
                                            <TableCell
                                                data-align="end"
                                                className={amountCellClass}
                                            >
                                                <FinanceMoney
                                                    value={row.balance ?? "0"}
                                                />
                                            </TableCell>
                                        )}
                                        {showActions && (
                                            <TableCell
                                                data-align="end"
                                                className="whitespace-normal"
                                            >
                                                {row.action}
                                            </TableCell>
                                        )}
                                    </TableRow>
                                )
                            })}
                            {visible.length === 0 && (
                                <TableRow>
                                    <TableCell
                                        colSpan={columns}
                                        className="text-center text-muted-foreground"
                                    >
                                        未找到匹配记录
                                    </TableCell>
                                </TableRow>
                            )}
                        </TableBody>
                        <TableFooter className="border-0 bg-surface-sunken [&_tr]:border-b-0">
                            <TableRow className="hover:bg-transparent">
                                <TableCell className="rounded-l-md text-xs whitespace-normal last:rounded-r-md">
                                    全部合计
                                    {query ? (
                                        <span className="num block text-tiny text-muted-foreground">
                                            （当前显示 {visible.length} 项
                                            {filtered.length > visible.length
                                                ? `，筛选命中 ${filtered.length} 项`
                                                : ""}
                                            ，合计按全部）
                                        </span>
                                    ) : null}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="wrap-anywhere whitespace-normal last:rounded-r-md"
                                >
                                    <FinanceMoney value={total} />
                                </TableCell>
                                {balanceTotal !== null && (
                                    <TableCell
                                        data-align="end"
                                        className="wrap-anywhere whitespace-normal last:rounded-r-md"
                                    >
                                        <FinanceMoney value={balanceTotal} />
                                    </TableCell>
                                )}
                                {showActions && (
                                    <TableCell className="rounded-r-md" />
                                )}
                            </TableRow>
                        </TableFooter>
                    </Table>
                    {showFilter && (
                        <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                            <span className="num">
                                {query ? `匹配 ${filtered.length} / ` : ""}共{" "}
                                {rows.length} 条
                            </span>
                            {pageCount > 1 && (
                                <nav
                                    aria-label={`${title}分页`}
                                    className="flex items-center gap-2"
                                >
                                    <Button
                                        id={`${id}-prev`}
                                        type="button"
                                        variant="outline"
                                        size="icon-xs"
                                        aria-label="上一页"
                                        disabled={currentPage === 1}
                                        onClick={() => setPage(currentPage - 1)}
                                    >
                                        <ChevronLeftIcon aria-hidden="true" />
                                    </Button>
                                    <span className="num" aria-live="polite">
                                        第 {currentPage} / {pageCount} 页
                                    </span>
                                    <Button
                                        id={`${id}-next`}
                                        type="button"
                                        variant="outline"
                                        size="icon-xs"
                                        aria-label="下一页"
                                        disabled={currentPage === pageCount}
                                        onClick={() => setPage(currentPage + 1)}
                                    >
                                        <ChevronRightIcon aria-hidden="true" />
                                    </Button>
                                </nav>
                            )}
                        </div>
                    )}
                </>
            )}
        </section>
    )
}
