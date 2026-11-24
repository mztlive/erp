"use client"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { useState } from "react"
import {
    ChevronDownIcon,
    ChevronUpIcon,
    FileTextIcon,
    TriangleAlertIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { SalesLinePriceComparison } from "@/components/business/sales-line-price-comparison"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableHeader,
    TableBody,
    TableRow,
    TableHead,
    TableCell,
} from "@/components/ui/table"
import type { ApprovalSalesSubmission } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import {
    approvalSalesUnit,
    displayReadableName,
    displayUnixDate,
} from "@/features/approval-workflow/display"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { welfareScenarioLabel } from "@/lib/business-options"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { displayBusinessText } from "@/lib/display-name"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

const dateLabel = (value: number | null) => displayUnixDate(value)

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
        const line = negativeLines[0]
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
                        {submission.lines.length ? (
                            <div
                                data-slot="detail-inline-table"
                                className={
                                    detailWorkspaceStyles.inlineTableWrapper
                                }
                            >
                                <Table
                                    aria-label="销售明细"
                                    className="w-full [&_:is(th,td):first-child]:ps-0"
                                >
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead className="w-9">
                                                序号
                                            </TableHead>
                                            <TableHead>商品 / 规格</TableHead>
                                            <TableHead data-align="end">
                                                数量
                                            </TableHead>
                                            <TableHead>
                                                {isCard
                                                    ? "含税单价 / 面额"
                                                    : "含税单价 / 供给价"}
                                            </TableHead>
                                            <TableHead data-align="end">
                                                金额 / 税额
                                            </TableHead>
                                            <TableHead data-align="end">
                                                收货分配
                                            </TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {submission.lines.map((line) => (
                                            <SalesLine
                                                key={line.id}
                                                line={line}
                                                isCard={isCard}
                                                unit={approvalSalesUnit(
                                                    line,
                                                    isCard,
                                                    names,
                                                )}
                                                idPrefix={`${prefix}-line-${toAutomationIdSegment(line.id)}`}
                                            />
                                        ))}
                                    </TableBody>
                                </Table>
                            </div>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                本次提交没有销售明细。
                            </p>
                        )}
                        {submission.business_remark && (
                            <dl className="border-t border-grid pt-4">
                                <DescriptionItem>
                                    <DescriptionTerm>业务备注</DescriptionTerm>
                                    <DescriptionDetails className="whitespace-pre-wrap wrap-anywhere">
                                        {submission.business_remark}
                                    </DescriptionDetails>
                                </DescriptionItem>
                            </dl>
                        )}
                    </>
                )}
            </DetailWorkspaceSection>
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

const ALLOCATION_PAGE_SIZE = 20

function SalesLine({
    line,
    unit,
    isCard,
    idPrefix,
}: {
    line: SalesApprovalLine
    unit?: string
    isCard: boolean
    idPrefix: string
}) {
    const [allocationsOpen, setAllocationsOpen] = useState(false)
    const [allocationPage, setAllocationPage] = useState(0)

    const spec = displayBusinessText(line.spec_snapshot, line.sku_id)
    const quantity =
        line.quantity ??
        (line.card_count == null ? null : String(line.card_count))
    const name =
        displayReadableName(line.item_name_snapshot, line.sku_id) ||
        "未记录商品名称"
    const welfareScenario = welfareScenarioLabel(line.welfare_scenario ?? "")
    const negativeMargin = isNegativeMarginLine(line, isCard)
    const allocations = line.delivery_allocations ?? []
    const uniqueAddressCount = allocations.length
        ? new Set(
              allocations.map((allocation) => fullDeliveryAddress(allocation)),
          ).size
        : 0
    const allocationPageCount = Math.max(
        1,
        Math.ceil(allocations.length / ALLOCATION_PAGE_SIZE),
    )
    const currentPage = Math.min(allocationPage, allocationPageCount - 1)
    const start = currentPage * ALLOCATION_PAGE_SIZE

    return (
        <>
            <TableRow
                className={cn(
                    negativeMargin &&
                        "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                    allocationsOpen && !negativeMargin && "bg-muted/30",
                )}
            >
                <TableCell className="num text-muted-foreground">
                    {line.line_no}
                </TableCell>
                <TableCell className="min-w-36 max-w-64 whitespace-normal">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <p className="font-medium wrap-anywhere">{name}</p>
                        {line.pricing_mode === "GIFT" ? (
                            <Badge variant="neutral">赠品</Badge>
                        ) : null}
                        {negativeMargin ? (
                            <Badge variant="destructive">负毛利</Badge>
                        ) : null}
                    </div>
                    {spec && spec !== "无规格" ? (
                        <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                            {spec}
                        </p>
                    ) : null}
                    {welfareScenario ? (
                        <p className="mt-0.5 text-xs text-muted-foreground wrap-anywhere">
                            {welfareScenario}
                        </p>
                    ) : null}
                    {!isCard && line.fulfillment_due_at != null ? (
                        <p className="num mt-0.5 text-xs text-muted-foreground">
                            交付：{dateLabel(line.fulfillment_due_at)}
                        </p>
                    ) : null}
                </TableCell>
                <TableCell data-align="end">
                    {quantity == null ? (
                        "—"
                    ) : (
                        <QuantityValue value={quantity} unit={unit} />
                    )}
                </TableCell>
                <TableCell className="whitespace-normal">
                    <div className="space-y-1">
                        <div className="flex flex-wrap items-baseline gap-x-1.5">
                            <span className="text-xs text-muted-foreground">
                                售
                            </span>
                            <MoneyValue
                                display="exact"
                                value={line.unit_price_gross}
                                className={
                                    negativeMargin
                                        ? "text-destructive-soft-foreground"
                                        : undefined
                                }
                            />
                        </div>
                        {isCard ? (
                            <div className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
                                <span>
                                    面额{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={line.face_value}
                                    />
                                </span>
                                <span>
                                    ·{" "}
                                    {line.card_form === "PHYSICAL"
                                        ? "实体卡"
                                        : line.card_form === "ELECTRONIC"
                                          ? "电子券"
                                          : "—"}
                                </span>
                            </div>
                        ) : (
                            <div className="border-t border-border/50 pt-1">
                                <p className="text-tiny text-muted-foreground">
                                    采购供给价
                                </p>
                                <SalesLinePriceComparison
                                    prices={line}
                                    showSalesPrice={false}
                                    compact
                                    unit={unit}
                                />
                            </div>
                        )}
                    </div>
                </TableCell>
                <TableCell data-align="end">
                    <p className="font-medium">
                        <span className="mr-1 text-xs font-normal text-muted-foreground">
                            含税
                        </span>
                        <MoneyValue display="exact" value={line.gross_amount} />
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        不含税{" "}
                        <MoneyValue display="exact" value={line.net_amount} />
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        税{" "}
                        <RateValue
                            value={multiplyFixed(line.sales_tax_rate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                        {" · "}
                        <MoneyValue display="exact" value={line.tax_amount} />
                    </p>
                </TableCell>
                <TableCell data-align="end">
                    {allocations.length > 0 ? (
                        <Button
                            id={`${idPrefix}-allocations`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            aria-expanded={allocationsOpen}
                            aria-controls={
                                allocationsOpen
                                    ? `${idPrefix}-allocations-list`
                                    : undefined
                            }
                            aria-label={`${allocationsOpen ? "收起" : "展开"}${name}的 ${allocations.length} 条收货分配`}
                            className={cn(
                                detailWorkspaceStyles.subtleButton,
                                "num h-7 px-2",
                            )}
                            onClick={() => {
                                setAllocationsOpen((open) => !open)
                                setAllocationPage(0)
                            }}
                        >
                            <span>
                                {uniqueAddressCount > 1
                                    ? `${uniqueAddressCount} 地 / ${allocations.length} 条`
                                    : `${allocations.length} 条`}
                            </span>
                            {allocationsOpen ? (
                                <ChevronUpIcon
                                    aria-hidden="true"
                                    className="size-3.5"
                                />
                            ) : (
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className="size-3.5"
                                />
                            )}
                        </Button>
                    ) : (
                        <span className="text-muted-foreground">0 条</span>
                    )}
                </TableCell>
            </TableRow>
            {allocationsOpen && allocations.length > 0 ? (
                <TableRow className="bg-muted/25 hover:bg-muted/25">
                    <TableCell colSpan={6} className="whitespace-normal p-4">
                        <div
                            id={`${idPrefix}-allocations-list`}
                            role="region"
                            aria-label={`${name}收货分配明细`}
                            className="space-y-2.5"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                                <span>
                                    收货分配 · {name} · 共 {allocations.length}{" "}
                                    条
                                    {uniqueAddressCount > 1
                                        ? `（${uniqueAddressCount} 个收货地址）`
                                        : ""}
                                </span>
                                {allocationPageCount > 1 ? (
                                    <span className="num">
                                        第 {start + 1}–
                                        {Math.min(
                                            start + ALLOCATION_PAGE_SIZE,
                                            allocations.length,
                                        )}{" "}
                                        条
                                    </span>
                                ) : null}
                            </div>
                            <div className="overflow-x-auto border-y border-border/60 bg-card">
                                <Table
                                    aria-label="收货分配明细"
                                    className="w-full"
                                >
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead className="w-10">
                                                序号
                                            </TableHead>
                                            <TableHead data-align="end">
                                                数量
                                            </TableHead>
                                            <TableHead>收货人 / 电话</TableHead>
                                            <TableHead>收货地址</TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {allocations
                                            .slice(
                                                start,
                                                start + ALLOCATION_PAGE_SIZE,
                                            )
                                            .map((allocation, index) => (
                                                <TableRow key={start + index}>
                                                    <TableCell className="num text-muted-foreground">
                                                        {start + index + 1}
                                                    </TableCell>
                                                    <TableCell data-align="end">
                                                        <QuantityValue
                                                            value={
                                                                allocation.quantity
                                                            }
                                                            unit={unit}
                                                        />
                                                    </TableCell>
                                                    <TableCell className="whitespace-normal wrap-anywhere">
                                                        <p className="font-medium">
                                                            {
                                                                allocation.recipient
                                                            }
                                                        </p>
                                                        <p className="num text-xs text-muted-foreground">
                                                            {allocation.phone}
                                                        </p>
                                                    </TableCell>
                                                    <TableCell className="whitespace-normal wrap-anywhere">
                                                        {fullDeliveryAddress(
                                                            allocation,
                                                        )}
                                                    </TableCell>
                                                </TableRow>
                                            ))}
                                    </TableBody>
                                </Table>
                            </div>
                            {allocationPageCount > 1 ? (
                                <div className="flex items-center justify-end gap-2">
                                    <Button
                                        id={`${idPrefix}-allocations-previous`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={currentPage === 0}
                                        onClick={() =>
                                            setAllocationPage(currentPage - 1)
                                        }
                                    >
                                        上一页
                                    </Button>
                                    <span
                                        className="num text-xs text-muted-foreground"
                                        aria-live="polite"
                                    >
                                        {currentPage + 1} /{" "}
                                        {allocationPageCount}
                                    </span>
                                    <Button
                                        id={`${idPrefix}-allocations-next`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={
                                            currentPage + 1 >=
                                            allocationPageCount
                                        }
                                        onClick={() =>
                                            setAllocationPage(currentPage + 1)
                                        }
                                    >
                                        下一页
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                    </TableCell>
                </TableRow>
            ) : null}
        </>
    )
}
