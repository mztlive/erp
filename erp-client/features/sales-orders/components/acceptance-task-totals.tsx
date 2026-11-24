import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import {
    clampZeroFixed,
    compactFixed,
    divideFixed,
    formatCurrencyFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import {
    compareQty,
    isPositiveQty,
    passQuantity,
    type AcceptanceBatchSelection,
    type AcceptanceOrderProgress,
} from "../lib/acceptance-model"
import type { AcceptanceEligibleFact } from "../lib/acceptance-types"
import type { SalesOrderDetailView } from "../api/contracts"

const quantityOptions = { maxScale: 6, outputScale: 6 }
const moneyOptions = { maxScale: 10, outputScale: 10 }
type Quantity = { unitCode: string; unitName?: string; quantity: string }

function quantityLabel(entries: readonly Quantity[]) {
    const groups = new Map<string, { name: string; values: string[] }>()
    for (const entry of entries) {
        const code = entry.unitCode.trim()
        const group = groups.get(code) ?? {
            name: entry.unitName ?? "",
            values: [],
        }
        group.values.push(entry.quantity)
        group.name ||= entry.unitName ?? ""
        groups.set(code, group)
    }
    return (
        [...groups.values()]
            .map(({ name, values }) =>
                [compactFixed(sumFixed(values, quantityOptions)), name]
                    .filter(Boolean)
                    .join(" "),
            )
            .join(" / ") || "0"
    )
}

const nonNegativeDifference = (left: string, right: string) =>
    clampZeroFixed(subtractFixed(left, right, quantityOptions), quantityOptions)

export function buildAcceptanceTaskTotals(
    progress: AcceptanceOrderProgress,
    facts: readonly AcceptanceEligibleFact[],
    selected: AcceptanceBatchSelection,
    order?: SalesOrderDetailView | null,
) {
    const prices = new Map(
        order?.lineItems.map((line) => [line.id, line.unitPriceGross]),
    )
    const drafts = facts.flatMap((fact) => {
        const draft = selected.get(fact.fulfillmentLineId)
        if (!draft) return []
        const validQuantity = (value: string, limit: string) =>
            isPositiveQty(value) && compareQty(value, limit) <= 0
        const quantity = validQuantity(draft.qty, fact.eligibleQuantity)
            ? draft.qty
            : "0"
        const passed =
            quantity !== "0" &&
            (draft.result === "PASS" ||
                validQuantity(draft.exceptionQty, quantity))
                ? passQuantity(draft)
                : "0"
        return [{ fact, quantity, passed }]
    })
    const passedById = new Map(
        drafts.map(({ fact, passed }) => [fact.fulfillmentLineId, passed]),
    )
    const pending = facts.map((fact) => ({
        ...fact,
        quantity: fact.eligibleQuantity,
    }))
    const passed = drafts.map(({ fact, passed }) => ({
        ...fact,
        quantity: passed,
    }))
    const registered = drafts.map(({ fact, quantity }) => ({
        ...fact,
        quantity,
    }))
    const remaining = pending.map((fact) => ({
        ...fact,
        quantity: nonNegativeDifference(
            fact.quantity,
            passedById.get(fact.fulfillmentLineId) ?? "0",
        ),
    }))
    const accepted = progress.lines.map((line) => ({
        ...line,
        quantity: line.acceptedQuantity,
    }))
    const unready = progress.lines.map((line) => ({
        ...line,
        quantity: nonNegativeDifference(
            line.requiredQuantity,
            sumFixed(
                [line.acceptedQuantity, line.pendingQuantity],
                quantityOptions,
            ),
        ),
    }))
    const sum = (values: readonly Quantity[]) =>
        sumFixed(
            values.map(({ quantity }) => quantity),
            quantityOptions,
        )
    const mixedUnits =
        new Set(
            [...pending, ...accepted, ...unready]
                .filter((entry) => isPositiveQty(entry.quantity))
                .map((entry) => entry.unitCode.trim()),
        ).size > 1
    // 混合计量单位时按销售单行统计覆盖，保留分单位数量。
    const coverage = mixedUnits
        ? [
              progress.lines.filter((line) => line.stuckKind === "done").length,
              progress.lines.filter((line) => line.stuckKind === "accept")
                  .length,
              progress.lines.filter(
                  (line) =>
                      line.stuckKind === "deliver" ||
                      line.stuckKind === "exception",
              ).length,
          ].map(String)
        : [sum(accepted), sum(pending), sum(unready)]
    const totalCoverage = sumFixed(coverage, quantityOptions)
    const widths = coverage.map((value) =>
        isPositiveQty(totalCoverage)
            ? divideFixed(value, totalCoverage, {
                  numeratorMaxScale: 6,
                  denominatorMaxScale: 6,
                  outputScale: 6,
              })
            : "0",
    )
    const money = (
        rows: readonly (Quantity & { salesOrderLineId: string })[],
    ) => {
        if (!order || rows.some((row) => !prices.get(row.salesOrderLineId)))
            return "—"
        const amounts = rows.map((row) =>
            multiplyFixed(row.quantity, prices.get(row.salesOrderLineId)!, {
                leftMaxScale: 6,
                rightMaxScale: 4,
                outputScale: 10,
            }),
        )
        return formatCurrencyFixed(sumFixed(amounts, moneyOptions), {
            maxScale: 10,
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
        })
    }
    return {
        totalFactCount: facts.length,
        rowCount: selected.size,
        registered: quantityLabel(registered),
        passed: quantityLabel(passed),
        pending: quantityLabel(pending),
        accepted: quantityLabel(accepted),
        unready: quantityLabel(unready),
        remaining: quantityLabel(remaining),
        pendingAmount: money(pending),
        postedAmount: money(passed),
        widths,
        mixedUnits,
    }
}

type AcceptanceTaskTotals = ReturnType<typeof buildAcceptanceTaskTotals>

export function AcceptanceTaskKpis({
    totals,
    idPrefix,
    fileName,
    acceptedAt,
}: {
    totals: AcceptanceTaskTotals
    idPrefix: string
    fileName?: string
    acceptedAt: string
}) {
    const hasAmount = totals.pendingAmount !== "—"
    return (
        <dl
            aria-label="本次客户验收核心指标"
            className="grid grid-cols-1 items-start gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/acceptance-task:grid-cols-2 @[840px]/acceptance-task:grid-cols-4"
        >
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>本次待验收总额</span>
                    <span className="text-tiny text-muted-foreground">
                        含税估算
                    </span>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${idPrefix}-amount-help`}
                            render={
                                <Button
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="验收金额计算口径"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>
                            待验收总额 = Σ(可验数量 ×
                            销售单含税单价)；本次登记金额按通过数量核算。
                        </TooltipContent>
                    </Tooltip>
                </dt>
                <dd className="num wrap-anywhere text-value-section font-semibold tracking-tight">
                    {totals.pendingAmount}
                </dd>
                <dd className="text-xs text-muted-foreground">
                    {hasAmount ? (
                        <>
                            本次登记金额{" "}
                            <span className="num text-foreground">
                                {totals.postedAmount}
                            </span>
                        </>
                    ) : (
                        "暂无法核算含税额，按数量核对"
                    )}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>验收覆盖进度</span>
                    <span className="text-tiny text-muted-foreground">
                        {totals.mixedUnits ? "按行覆盖" : "按数量覆盖"}
                    </span>
                    <Tooltip>
                        <TooltipTrigger
                            id={`${idPrefix}-coverage-help`}
                            render={
                                <Button
                                    type="button"
                                    size="icon-xs"
                                    variant="ghost"
                                    aria-label="验收覆盖进度统计口径"
                                />
                            }
                        >
                            <span aria-hidden="true">?</span>
                        </TooltipTrigger>
                        <TooltipContent>
                            绿色为已通过验收、蓝色为当前可验、橙色为待交付未就绪；多单位商品按销售单行统计比例。
                        </TooltipContent>
                    </Tooltip>
                </dt>
                <dd className="num wrap-anywhere text-value-compact font-semibold">
                    {totals.accepted}
                </dd>
                <dd className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                    <span className="inline-flex items-center gap-1">
                        <span
                            aria-hidden="true"
                            className="size-1.5 shrink-0 rounded-full bg-success"
                        />
                        已验
                    </span>
                    <span className="inline-flex items-center gap-1">
                        <span
                            aria-hidden="true"
                            className="size-1.5 shrink-0 rounded-full bg-info"
                        />
                        可验 <span className="num">{totals.pending}</span>
                    </span>
                    <span className="inline-flex items-center gap-1">
                        <span
                            aria-hidden="true"
                            className="size-1.5 shrink-0 rounded-full bg-warning"
                        />
                        待交付 <span className="num">{totals.unready}</span>
                    </span>
                </dd>
                <dd>
                    <div
                        aria-hidden="true"
                        className="mt-1.5 flex h-1.5 overflow-hidden rounded-full bg-border"
                    >
                        {["bg-success", "bg-info", "bg-warning"].map(
                            (tone, index) => (
                                <span
                                    key={tone}
                                    className={tone}
                                    style={{
                                        flexBasis: `calc(${totals.widths[index]} * 100%)`,
                                    }}
                                />
                            ),
                        )}
                    </div>
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>本次纳入批次与数量</span>
                    <span className="text-tiny text-muted-foreground">
                        共 <span className="num">{totals.totalFactCount}</span>{" "}
                        批可验
                    </span>
                </dt>
                <dd className="num text-value-compact font-semibold">
                    {totals.rowCount} 行
                </dd>
                <dd className="num wrap-anywhere text-xs text-muted-foreground">
                    合计 {totals.registered}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                    <span>签收单凭证与验收时间</span>
                    <span className="text-tiny text-muted-foreground">
                        必传
                    </span>
                </dt>
                <dd className="mt-1 flex flex-wrap items-center gap-1.5">
                    <StatusBadge
                        tone={fileName ? "success" : "warning"}
                        appearance={fileName ? "quiet" : "badge"}
                        label={fileName ? "已上传凭证" : "待上传凭证"}
                    />
                </dd>
                {fileName ? (
                    <dd
                        className="mt-1 truncate text-xs text-foreground"
                        title={fileName}
                    >
                        {fileName}
                    </dd>
                ) : null}
                <dd className="num mt-0.5 text-xs text-muted-foreground">
                    {acceptedAt.replace("T", " ") || "未填写验收时间"}
                </dd>
            </div>
        </dl>
    )
}

export function AcceptanceTaskFooterTotals({
    totals,
}: {
    totals: AcceptanceTaskTotals
}) {
    return (
        <dl
            aria-live="polite"
            className="flex min-w-0 flex-wrap gap-x-3 gap-y-1 text-xs"
        >
            {[
                ["本次登记行数", totals.rowCount],
                ["本次验收数量", totals.passed],
                ["本次验收金额", totals.postedAmount],
                ["提交后剩余待验", totals.remaining],
            ].map(([label, value]) => (
                <div
                    key={label}
                    className="flex min-w-0 flex-wrap items-baseline gap-x-1"
                >
                    <dt className="text-muted-foreground">{label}</dt>
                    <dd className="num wrap-anywhere font-medium">{value}</dd>
                </div>
            ))}
        </dl>
    )
}
