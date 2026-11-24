"use client"

import { useMemo, useState } from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    FileTextIcon,
    MapPinIcon,
    SearchIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DeliveryAllocationOverview,
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { ApprovalSalesSubmission } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import {
    approvalSalesUnit,
    displayReadableName,
    displayUnixDate,
} from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { welfareScenarioLabel } from "@/lib/business-options"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    buildDeliveryAllocationSummary,
    formatLineDeliveryHint,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { displayBusinessText } from "@/lib/display-name"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import { cn } from "@/lib/utils"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

const dateLabel = (value: number | null) => displayUnixDate(value)
const PAGE_SIZE = 20

type SalesApprovalLine = ApprovalSalesSubmission["lines"][number]

function isNegativeMarginLine(
    line: SalesApprovalLine,
    isCard: boolean,
): boolean {
    if (isCard || line.pricing_mode === "GIFT") return false
    const unitPrice = line.unit_price_gross
    const supplyPrice = line.procurement_supply_price_gross
    if (
        unitPrice == null ||
        unitPrice === "" ||
        supplyPrice == null ||
        supplyPrice === ""
    ) {
        return false
    }
    try {
        return compareDecimal(unitPrice, supplyPrice, 6) < 0
    } catch {
        return false
    }
}

function calculateMarginRate(
    sellingPrice: string | null | undefined,
    costPrice: string | null | undefined,
): { percent: string; negative: boolean } | null {
    if (!sellingPrice || !costPrice) return null
    try {
        if (compareDecimal(sellingPrice, "0", 6) <= 0) return null
        const diff = subtractFixed(sellingPrice, costPrice, {
            maxScale: 6,
            outputScale: 6,
        })
        const negative = compareDecimal(diff, "0", 6) < 0
        const absDiff = negative ? diff.slice(1) : diff
        const rate = divideFixed(
            multiplyFixed(absDiff, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 6,
            }),
            sellingPrice,
            {
                numeratorMaxScale: 6,
                denominatorMaxScale: 6,
                outputScale: 1,
            },
        )
        return {
            percent: negative ? `-${rate}` : rate,
            negative,
        }
    } catch {
        return null
    }
}

/** 判定销售审批提交是否存在负毛利等高危复核信号（严格使用定点十进制比较）。 */
export function assessSalesApprovalRisk(
    submission?: ApprovalSalesSubmission | null,
): { highRisk: boolean; reasons: string[] } {
    if (!submission || submission.business_type === "VOUCHER") {
        return { highRisk: false, reasons: [] }
    }
    const negativeLines = submission.lines.filter((line) =>
        isNegativeMarginLine(line, false),
    )
    if (negativeLines.length === 0) {
        return { highRisk: false, reasons: [] }
    }
    const reasons: string[] = []
    if (negativeLines.length === 1) {
        const line = negativeLines[0]!
        const name =
            displayReadableName(line.item_name_snapshot, line.sku_id) ||
            "未命名商品"
        reasons.push(
            `第 ${line.line_no} 行「${name}」含税售价低于采购供给价（负毛利风险）`,
        )
    } else {
        const lineNos = negativeLines
            .slice(0, 3)
            .map((line) => `第 ${line.line_no} 行`)
            .join("、")
        const suffix =
            negativeLines.length > 3 ? `等 ${negativeLines.length} 行` : ""
        reasons.push(
            `共 ${negativeLines.length} 行商品含税售价低于采购供给价（负毛利风险：${lineNos}${suffix}）`,
        )
    }
    return { highRisk: true, reasons }
}

/** 全部明细与纸质预览读取同一审批提交，不回退到当前销售单。 */
export function WorkspaceSalesApproval({
    instanceId,
    item,
    onPreview,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const query = useApprovalMaterials(instanceId, true, item)
    const submission = query.data?.sales_order
    const attachments = query.data?.attachments ?? []
    const prefix = `workspace-sales-approval-${toAutomationIdSegment(instanceId)}`
    const risk = assessSalesApprovalRisk(submission)
    const isCard = submission?.business_type === "VOUCHER"
    const { names } = useProductUnitNames(
        submission?.lines.map((line) => line.sku_id),
        Boolean(!isCard && submission?.lines.length),
    )

    const [search, setSearch] = useState("")
    const [riskOnly, setRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [focusedDeliveryLineId, setFocusedDeliveryLineId] = useState<
        string | null
    >(null)

    const deliveryLines = useMemo<
        readonly DeliveryAllocationLineInput[]
    >(() => {
        if (!submission?.lines.length) return []
        return submission.lines.map((line) => ({
            id: line.id,
            lineNo: line.line_no,
            itemName:
                displayReadableName(line.item_name_snapshot, line.sku_id) ||
                "未记录商品名称",
            specification: displayBusinessText(line.spec_snapshot, line.sku_id),
            unit: approvalSalesUnit(line, isCard, names),
            quantity:
                line.quantity ??
                (line.card_count == null ? null : String(line.card_count)),
            allocations: line.delivery_allocations,
        }))
    }, [isCard, names, submission?.lines])

    const deliverySummary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )
    const deliveryItemById = useMemo(
        () =>
            new Map(
                deliverySummary.itemGroups.map((group) => [
                    group.lineId,
                    group,
                ]),
            ),
        [deliverySummary.itemGroups],
    )

    const evaluatedLines = useMemo(() => {
        if (!submission?.lines.length) return []
        return submission.lines.map((line) => {
            const unit = approvalSalesUnit(line, isCard, names)
            const name =
                displayReadableName(line.item_name_snapshot, line.sku_id) ||
                "未记录商品名称"
            const spec = displayBusinessText(line.spec_snapshot, line.sku_id)
            const welfare = welfareScenarioLabel(line.welfare_scenario ?? "")
            const negativeMargin = isNegativeMarginLine(line, isCard)
            const priceCheck = isCard ? null : salesLinePriceComparison(line)
            const isGift = line.pricing_mode === "GIFT"
            const margin =
                !isCard && !isGift
                    ? calculateMarginRate(
                          line.unit_price_gross,
                          line.procurement_supply_price_gross,
                      )
                    : null
            const hasRisk =
                negativeMargin ||
                Boolean(
                    priceCheck &&
                    (priceCheck.change != null ||
                        priceCheck.warnings.length > 0),
                )
            return {
                line,
                unit,
                name,
                spec: spec && spec !== "无规格" ? spec : undefined,
                welfare: welfare || undefined,
                negativeMargin,
                priceCheck,
                isGift,
                margin,
                hasRisk,
            }
        })
    }, [isCard, names, submission?.lines])

    const riskCount = evaluatedLines.filter((entry) => entry.hasRisk).length
    const needle = search.trim().toLocaleLowerCase()
    const filteredLines = useMemo(() => {
        return evaluatedLines.filter((entry) => {
            if (riskOnly && !entry.hasRisk) return false
            if (!needle) return true
            const baseText = [
                String(entry.line.line_no),
                entry.name,
                entry.spec,
                entry.welfare,
            ]
                .filter(Boolean)
                .join(" ")
                .toLocaleLowerCase()
            if (baseText.includes(needle)) return true
            return (entry.line.delivery_allocations ?? []).some((row) =>
                `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                    .toLocaleLowerCase()
                    .includes(needle),
            )
        })
    }, [evaluatedLines, needle, riskOnly])

    const showToolbar =
        Boolean(submission) &&
        (evaluatedLines.length > 10 ||
            riskCount > 0 ||
            Boolean(search) ||
            riskOnly)
    const pageCount = Math.max(1, Math.ceil(filteredLines.length / PAGE_SIZE))
    const currentPage = Math.min(page, pageCount)
    const visibleLines = filteredLines.slice(
        (currentPage - 1) * PAGE_SIZE,
        currentPage * PAGE_SIZE,
    )

    const footerTotals = useMemo(() => {
        if (!submission?.lines.length) return null
        const units = new Set<string>()
        const quantities: string[] = []
        let allPricedGoodsHaveSupply = !isCard
        let totalRevenue = "0"
        let totalSupplyCost = "0"
        let pricedGoodsCount = 0

        for (const entry of evaluatedLines) {
            const qty =
                entry.line.quantity ??
                (entry.line.card_count == null
                    ? null
                    : String(entry.line.card_count))
            if (qty != null) {
                quantities.push(qty)
                units.add(entry.unit ?? "")
            }
            if (!isCard && !entry.isGift) {
                pricedGoodsCount += 1
                const supply = entry.line.procurement_supply_price_gross
                if (supply != null && supply !== "" && qty != null) {
                    try {
                        const lineCost = multiplyFixed(supply, qty, {
                            leftMaxScale: 6,
                            rightMaxScale: 6,
                            outputScale: 2,
                        })
                        totalRevenue = sumFixed(
                            [totalRevenue, entry.line.gross_amount],
                            { maxScale: 2, outputScale: 2 },
                        )
                        totalSupplyCost = sumFixed(
                            [totalSupplyCost, lineCost],
                            { maxScale: 2, outputScale: 2 },
                        )
                    } catch {
                        allPricedGoodsHaveSupply = false
                    }
                } else {
                    allPricedGoodsHaveSupply = false
                }
            }
        }

        const totalQuantity =
            units.size === 1 && quantities.length === evaluatedLines.length
                ? {
                      value: compactFixed(
                          sumFixed(quantities, { maxScale: 6, outputScale: 6 }),
                      ),
                      unit: [...units][0] || undefined,
                  }
                : null

        const overallMargin =
            allPricedGoodsHaveSupply && pricedGoodsCount > 0
                ? calculateMarginRate(totalRevenue, totalSupplyCost)
                : null

        return { totalQuantity, overallMargin }
    }, [evaluatedLines, isCard, submission?.lines.length])

    const deliverySectionId = `${prefix}-delivery-section`
    const handleFocusDelivery = (lineId: string) => {
        setFocusedDeliveryLineId(lineId)
        window.requestAnimationFrame(() => {
            document
                .getElementById(deliverySectionId)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
    }

    const showDeliverySection = Boolean(
        submission && (!isCard || deliverySummary.totalRows > 0),
    )

    return (
        <>
            <DetailWorkspaceSection
                title="销售明细"
                subtitle={
                    submission ? `共 ${submission.lines.length} 行` : undefined
                }
                ariaLabel="销售明细"
                className="@container/sales-lines"
                actions={
                    <Button
                        id={`${prefix}-preview`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                title: query.data?.document_no,
                            })
                        }
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览完整销售单
                    </Button>
                }
            >
                {query.isPending ? (
                    <p
                        role="status"
                        className="flex items-center gap-2 text-xs text-muted-foreground"
                    >
                        <Spinner />
                        正在读取销售明细…
                    </p>
                ) : query.isError ? (
                    <BusinessFailureState
                        id={`${prefix}-retry`}
                        title="销售明细读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                ) : !submission ? (
                    <Alert variant="warning">
                        <AlertDescription>
                            本次提交未保留完整销售明细，请查看提交资料及附件核对。
                        </AlertDescription>
                    </Alert>
                ) : (
                    <>
                        {risk.highRisk ? (
                            <Alert variant="destructive" role="alert">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertTitle>存在负毛利风险</AlertTitle>
                                <AlertDescription>
                                    {risk.reasons.join("；")}
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        {showToolbar ? (
                            <div
                                className="flex flex-wrap items-center justify-between gap-2.5"
                                aria-label="销售明细筛选"
                            >
                                <div
                                    role="group"
                                    aria-label="风险筛选"
                                    className={detailWorkspaceStyles.subTabs}
                                >
                                    <button
                                        id={`${prefix}-filter-all`}
                                        type="button"
                                        aria-pressed={!riskOnly}
                                        data-active={
                                            !riskOnly ? "true" : undefined
                                        }
                                        className={detailWorkspaceStyles.subTab}
                                        onClick={() => {
                                            setRiskOnly(false)
                                            setPage(1)
                                        }}
                                    >
                                        <span>全部</span>
                                        <span className="num text-xs text-muted-foreground">
                                            {evaluatedLines.length}
                                        </span>
                                    </button>
                                    {riskCount > 0 ? (
                                        <button
                                            id={`${prefix}-filter-risk`}
                                            type="button"
                                            aria-pressed={riskOnly}
                                            data-active={
                                                riskOnly ? "true" : undefined
                                            }
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
                                            onClick={() => {
                                                setRiskOnly(true)
                                                setPage(1)
                                            }}
                                        >
                                            <span>仅看风险项</span>
                                            <Badge
                                                variant={
                                                    risk.highRisk
                                                        ? "destructive"
                                                        : "warning"
                                                }
                                                className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                                            >
                                                {riskCount}
                                            </Badge>
                                        </button>
                                    ) : null}
                                </div>
                                <div className="relative w-full sm:w-60">
                                    <SearchIcon
                                        aria-hidden="true"
                                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                    />
                                    <Input
                                        id={`${prefix}-search`}
                                        value={search}
                                        onChange={(event) => {
                                            setSearch(event.target.value)
                                            setPage(1)
                                        }}
                                        placeholder="搜索商品、规格或收货人"
                                        aria-label="搜索商品、规格或收货人"
                                        className="h-control-sm pr-8 pl-8 text-xs"
                                    />
                                    {search ? (
                                        <Button
                                            id={`${prefix}-search-clear`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-xs"
                                            aria-label="清除搜索"
                                            className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                            onClick={() => {
                                                setSearch("")
                                                setPage(1)
                                            }}
                                        >
                                            <XIcon aria-hidden="true" />
                                        </Button>
                                    ) : null}
                                </div>
                            </div>
                        ) : null}

                        {submission.lines.length ? (
                            <div
                                data-slot="detail-inline-table"
                                className={
                                    detailWorkspaceStyles.inlineTableWrapper
                                }
                            >
                                <Table
                                    data-density="comfortable"
                                    aria-label="销售明细"
                                    className="w-full [&_:is(th,td):first-child]:ps-0"
                                >
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead className="w-8">
                                                #
                                            </TableHead>
                                            <TableHead className="min-w-44">
                                                商品 / 规格
                                            </TableHead>
                                            <TableHead
                                                data-align="end"
                                                className="w-20"
                                            >
                                                数量
                                            </TableHead>
                                            <TableHead
                                                data-align="end"
                                                className="w-28"
                                            >
                                                {isCard
                                                    ? "含税单价 / 面额"
                                                    : "含税售价"}
                                            </TableHead>
                                            {!isCard ? (
                                                <TableHead
                                                    data-align="end"
                                                    className="w-36"
                                                >
                                                    供给价 / 毛利
                                                </TableHead>
                                            ) : null}
                                            <TableHead
                                                data-align="end"
                                                className="w-32"
                                            >
                                                含税小计
                                            </TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {visibleLines.length > 0 ? (
                                            visibleLines.map((entry) => {
                                                const deliveryHint =
                                                    formatLineDeliveryHint(
                                                        deliveryItemById.get(
                                                            entry.line.id,
                                                        ),
                                                        deliverySummary,
                                                    )
                                                return (
                                                    <SalesLineRow
                                                        key={entry.line.id}
                                                        line={entry.line}
                                                        name={entry.name}
                                                        spec={entry.spec}
                                                        welfare={entry.welfare}
                                                        unit={entry.unit}
                                                        isCard={isCard}
                                                        isGift={entry.isGift}
                                                        negativeMargin={
                                                            entry.negativeMargin
                                                        }
                                                        margin={entry.margin}
                                                        priceCheck={
                                                            entry.priceCheck
                                                        }
                                                        deliveryHint={
                                                            deliveryHint
                                                        }
                                                        idPrefix={`${prefix}-line-${toAutomationIdSegment(entry.line.id)}`}
                                                        onFocusDelivery={() =>
                                                            handleFocusDelivery(
                                                                entry.line.id,
                                                            )
                                                        }
                                                    />
                                                )
                                            })
                                        ) : (
                                            <TableRow>
                                                <TableCell
                                                    colSpan={isCard ? 5 : 6}
                                                    className="py-10 text-center text-xs text-muted-foreground"
                                                >
                                                    没有匹配的销售明细，请调整搜索条件。
                                                </TableCell>
                                            </TableRow>
                                        )}
                                    </TableBody>
                                    <TableFooter>
                                        <TableRow className="hover:bg-transparent">
                                            <TableCell
                                                colSpan={2}
                                                className="py-2.5 align-middle text-xs font-medium text-muted-foreground"
                                            >
                                                合计 · 共{" "}
                                                <span className="num text-foreground">
                                                    {submission.lines.length}
                                                </span>{" "}
                                                项
                                                {filteredLines.length !==
                                                evaluatedLines.length ? (
                                                    <span className="num ml-1.5 font-normal">
                                                        （当前显示{" "}
                                                        {filteredLines.length}{" "}
                                                        项，合计按整单）
                                                    </span>
                                                ) : null}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="py-2.5 align-middle text-xs font-medium text-foreground"
                                            >
                                                {footerTotals?.totalQuantity ? (
                                                    <QuantityValue
                                                        value={
                                                            footerTotals
                                                                .totalQuantity
                                                                .value
                                                        }
                                                        unit={
                                                            footerTotals
                                                                .totalQuantity
                                                                .unit
                                                        }
                                                    />
                                                ) : (
                                                    "—"
                                                )}
                                            </TableCell>
                                            <TableCell className="py-2.5" />
                                            {!isCard ? (
                                                <TableCell
                                                    data-align="end"
                                                    className="py-2.5 align-middle text-xs"
                                                >
                                                    {footerTotals?.overallMargin ? (
                                                        <span
                                                            className={cn(
                                                                "num font-medium",
                                                                footerTotals
                                                                    .overallMargin
                                                                    .negative
                                                                    ? "text-destructive-soft-foreground"
                                                                    : "text-foreground/85",
                                                            )}
                                                        >
                                                            整单毛利{" "}
                                                            {
                                                                footerTotals
                                                                    .overallMargin
                                                                    .percent
                                                            }
                                                            %
                                                        </span>
                                                    ) : (
                                                        <span className="text-muted-foreground">
                                                            —
                                                        </span>
                                                    )}
                                                </TableCell>
                                            ) : null}
                                            <TableCell
                                                data-align="end"
                                                className="py-2.5 align-middle"
                                            >
                                                <div className="font-semibold text-foreground">
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            submission.gross_amount
                                                        }
                                                    />
                                                </div>
                                                <div className="mt-0.5 text-xs font-normal text-muted-foreground">
                                                    不含税{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            submission.net_amount
                                                        }
                                                    />
                                                </div>
                                            </TableCell>
                                        </TableRow>
                                    </TableFooter>
                                </Table>
                            </div>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                本次提交没有销售明细。
                            </p>
                        )}

                        {pageCount > 1 ? (
                            <div className="flex items-center justify-between gap-2 pt-1 text-xs text-muted-foreground">
                                <span className="num">
                                    第 {currentPage} / {pageCount} 页 · 共{" "}
                                    {filteredLines.length} 项
                                </span>
                                <div className="flex items-center gap-1">
                                    <Button
                                        id={`${prefix}-page-prev`}
                                        type="button"
                                        variant="outline"
                                        size="icon-sm"
                                        aria-label="上一页销售明细"
                                        disabled={currentPage === 1}
                                        onClick={() => setPage(currentPage - 1)}
                                    >
                                        <ChevronLeftIcon aria-hidden="true" />
                                    </Button>
                                    <Button
                                        id={`${prefix}-page-next`}
                                        type="button"
                                        variant="outline"
                                        size="icon-sm"
                                        aria-label="下一页销售明细"
                                        disabled={currentPage === pageCount}
                                        onClick={() => setPage(currentPage + 1)}
                                    >
                                        <ChevronRightIcon aria-hidden="true" />
                                    </Button>
                                </div>
                            </div>
                        ) : null}

                        {submission.business_remark ? (
                            <div className="border-t border-border/60 pt-3.5 text-body-compact">
                                <p className="text-xs font-medium text-muted-foreground">
                                    业务备注
                                </p>
                                <p className="mt-1 whitespace-pre-wrap wrap-anywhere text-foreground">
                                    {submission.business_remark}
                                </p>
                            </div>
                        ) : null}
                    </>
                )}
            </DetailWorkspaceSection>

            {showDeliverySection ? (
                <DeliveryAllocationOverview
                    idPrefix={`${prefix}-delivery`}
                    sectionId={deliverySectionId}
                    lines={deliveryLines}
                    summary={deliverySummary}
                    focusedLineId={focusedDeliveryLineId}
                    onClearFocus={() => setFocusedDeliveryLineId(null)}
                />
            ) : null}

            {query.data && !hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    title="合同、凭证及附件"
                    subtitle={
                        attachments.length
                            ? `共 ${attachments.length} 个文件`
                            : undefined
                    }
                    ariaLabel="合同、凭证及附件"
                >
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                        idPrefix={`${prefix}-file`}
                        emptyMessage="本次提交未附带合同或凭证文件。"
                        onSelectFile={onSelectFile}
                        activeFileId={activeFileId}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}

function SalesLineRow({
    line,
    name,
    spec,
    welfare,
    unit,
    isCard,
    isGift,
    negativeMargin,
    margin,
    priceCheck,
    deliveryHint,
    idPrefix,
    onFocusDelivery,
}: {
    line: SalesApprovalLine
    name: string
    spec?: string
    welfare?: string
    unit?: string
    isCard: boolean
    isGift: boolean
    negativeMargin: boolean
    margin: { percent: string; negative: boolean } | null
    priceCheck: ReturnType<typeof salesLinePriceComparison> | null
    deliveryHint: string | null
    idPrefix: string
    onFocusDelivery: () => void
}) {
    const quantity =
        line.quantity ??
        (line.card_count == null ? null : String(line.card_count))
    const dueDate =
        !isCard && line.fulfillment_due_at != null
            ? dateLabel(line.fulfillment_due_at)
            : null
    const metaParts = [
        spec,
        welfare,
        dueDate ? `交付 ${dueDate}` : null,
    ].filter(Boolean)

    const supplyPrice = line.procurement_supply_price_gross
    const originalPrice = line.original_procurement_supply_price_gross
    const hasOriginal = originalPrice != null && originalPrice !== ""
    const basisLabel =
        line.procurement_price_basis === "BULK"
            ? "集采"
            : line.procurement_price_basis === "DROPSHIP"
              ? "一件代发"
              : null
    const priceAdjusted = Boolean(priceCheck && priceCheck.change != null)

    const taxRatePercent = multiplyFixed(line.sales_tax_rate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })

    return (
        <TableRow
            className={cn(
                negativeMargin &&
                    "bg-destructive-soft/35 hover:bg-destructive-soft/50",
            )}
        >
            <TableCell className="num py-3 align-top text-muted-foreground">
                {line.line_no}
            </TableCell>

            <TableCell className="min-w-44 py-3 align-top whitespace-normal">
                <div className="flex flex-wrap items-center gap-1.5">
                    <span className="font-medium wrap-anywhere text-foreground">
                        {name}
                    </span>
                    {isGift ? <Badge variant="neutral">赠品</Badge> : null}
                    {negativeMargin ? (
                        <Badge variant="destructive">负毛利</Badge>
                    ) : null}
                </div>
                {metaParts.length > 0 ? (
                    <p className="num mt-1 text-xs wrap-anywhere text-muted-foreground">
                        {metaParts.join(" · ")}
                    </p>
                ) : null}
                {deliveryHint ? (
                    <button
                        id={`${idPrefix}-delivery-jump`}
                        type="button"
                        onClick={onFocusDelivery}
                        className="mt-1 inline-flex max-w-full cursor-pointer items-center gap-1 text-left text-xs text-primary/90 hover:text-primary hover:underline"
                    >
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3 shrink-0"
                        />
                        <span className="truncate">{deliveryHint}</span>
                    </button>
                ) : null}
            </TableCell>

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                {quantity == null ? (
                    "—"
                ) : (
                    <QuantityValue value={quantity} unit={unit} />
                )}
            </TableCell>

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                <MoneyValue
                    display="exact"
                    value={line.unit_price_gross}
                    className={cn(
                        "font-medium",
                        negativeMargin && "text-destructive-soft-foreground",
                    )}
                />
                {isCard ? (
                    <p className="mt-1 text-xs text-muted-foreground">
                        面额{" "}
                        <MoneyValue display="exact" value={line.face_value} />
                        {" · "}
                        {line.card_form === "PHYSICAL"
                            ? "实体卡"
                            : line.card_form === "ELECTRONIC"
                              ? "电子券"
                              : "—"}
                    </p>
                ) : null}
            </TableCell>

            {!isCard ? (
                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-normal"
                >
                    <div className="flex flex-wrap items-center justify-end gap-1.5">
                        {supplyPrice != null && supplyPrice !== "" ? (
                            <MoneyValue
                                display="exact"
                                value={supplyPrice}
                                className="font-medium text-foreground/90"
                            />
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                未填写
                            </span>
                        )}
                        {priceAdjusted && priceCheck ? (
                            <Badge variant="warning" className="text-tiny">
                                {priceCheck.adjustment === "increased"
                                    ? "调高"
                                    : "调低"}
                            </Badge>
                        ) : null}
                    </div>
                    {isGift ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            赠品成本
                        </p>
                    ) : margin ? (
                        <p
                            className={cn(
                                "num mt-1 text-xs",
                                margin.negative
                                    ? "font-medium text-destructive-soft-foreground"
                                    : "text-muted-foreground",
                            )}
                        >
                            {margin.negative ? "亏 " : "毛利 "}
                            {margin.percent}%
                        </p>
                    ) : null}
                    {priceAdjusted && hasOriginal ? (
                        <p className="mt-0.5 text-tiny text-warning-soft-foreground">
                            原底价{" "}
                            <MoneyValue
                                display="exact"
                                value={originalPrice}
                                className="font-normal [&_[data-slot=money-value-number]]:text-inherit"
                            />
                            {basisLabel ? ` · ${basisLabel}` : ""}
                        </p>
                    ) : !hasOriginal &&
                      supplyPrice != null &&
                      supplyPrice !== "" ? (
                        <p className="mt-0.5 text-tiny text-muted-foreground">
                            {basisLabel ? `${basisLabel} · ` : ""}无历史底价
                        </p>
                    ) : null}
                </TableCell>
            ) : null}

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                <div className="font-medium text-foreground">
                    <MoneyValue display="exact" value={line.gross_amount} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                    税 <RateValue value={taxRatePercent} precision={2} />
                    {" · "}
                    <MoneyValue display="exact" value={line.tax_amount} />
                </p>
            </TableCell>
        </TableRow>
    )
}
