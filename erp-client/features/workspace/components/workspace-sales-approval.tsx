"use client"

import { useState, type ReactNode } from "react"
import { FileTextIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    RateValue,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { SalesLinePriceComparison } from "@/components/business/sales-line-price-comparison"
import { Alert, AlertDescription } from "@/components/ui/alert"
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
import {
    Dialog,
    DialogTrigger,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import type { ApprovalSalesSubmission } from "@/features/approval-workflow/api/materials"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { displayReadableName } from "@/features/approval-workflow/display"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { displayBusinessText } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

const dateLabel = (value: number | null) =>
    value == null ? "—" : new Date(value * 1000).toLocaleDateString("zh-CN")

/** 全部明细与纸质预览读取同一审批提交，不回退到当前销售单。 */
export function WorkspaceSalesApproval({
    instanceId,
    item,
    onPreview,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
}) {
    const query = useApprovalMaterials(instanceId, true, item)
    const submission = query.data?.sales_order
    const prefix = `workspace-sales-approval-${toAutomationIdSegment(instanceId)}`

    return (
        <DetailWorkspaceSection
            title="销售明细"
            subtitle={
                submission
                    ? `共 ${submission.lines.length} 行 · 本次提交的全部明细`
                    : undefined
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
                    {submission.lines.length ? (
                        <div
                            data-slot="detail-inline-table"
                            className={detailWorkspaceStyles.inlineTableWrapper}
                        >
                            <Table
                                aria-label="销售明细"
                                className="min-w-[1080px] [&_:is(th,td):first-child]:ps-0"
                            >
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>序号</TableHead>
                                        <TableHead>商品 / 规格</TableHead>
                                        <TableHead className="text-right">
                                            数量
                                        </TableHead>
                                        <TableHead className="text-right">
                                            {submission.business_type ===
                                            "VOUCHER"
                                                ? "面额 / 形态"
                                                : "含税单价"}
                                        </TableHead>
                                        {submission.business_type !==
                                            "VOUCHER" && (
                                            <TableHead>
                                                <div>采购供给价</div>
                                                <div className="mt-0.5 text-xs font-normal">
                                                    销售填写的含税单价
                                                </div>
                                            </TableHead>
                                        )}
                                        {submission.business_type !==
                                            "VOUCHER" && (
                                            <TableHead>交付日期</TableHead>
                                        )}
                                        <TableHead className="text-right">
                                            税率 / 税额
                                        </TableHead>
                                        <TableHead className="text-right">
                                            销售金额
                                        </TableHead>
                                        <TableHead className="text-right">
                                            收货分配
                                        </TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {submission.lines.map((line) => (
                                        <SalesLine
                                            key={line.id}
                                            line={line}
                                            isCard={
                                                submission.business_type ===
                                                "VOUCHER"
                                            }
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
                    {submission.business_type === "VOUCHER" ||
                    submission.business_remark ? (
                        <dl className="grid grid-cols-2 gap-4 border-t border-grid pt-4 @min-[640px]/sales-lines:grid-cols-3">
                            {submission.business_type === "VOUCHER" && (
                                <Fact label="卡券履约期限">
                                    {dateLabel(submission.voucher_expiry_at)}
                                </Fact>
                            )}
                            {submission.business_remark && (
                                <DescriptionItem className="col-span-full">
                                    <DescriptionTerm>业务备注</DescriptionTerm>
                                    <DescriptionDetails className="whitespace-pre-wrap wrap-anywhere">
                                        {submission.business_remark}
                                    </DescriptionDetails>
                                </DescriptionItem>
                            )}
                        </dl>
                    ) : null}
                </>
            )}
        </DetailWorkspaceSection>
    )
}

type SalesApprovalLine = ApprovalSalesSubmission["lines"][number]

function SalesLine({
    line,
    isCard,
    idPrefix,
}: {
    line: SalesApprovalLine
    isCard: boolean
    idPrefix: string
}) {
    const unit =
        displayReadableName(line.unit_snapshot) || (isCard ? "张" : undefined)
    const spec = displayBusinessText(line.spec_snapshot, line.sku_id)
    const quantity =
        line.quantity ??
        (line.card_count == null ? null : String(line.card_count))
    const name =
        displayReadableName(line.item_name_snapshot, line.sku_id) ||
        "未记录商品名称"
    return (
        <TableRow>
            <TableCell className="num text-muted-foreground">
                {line.line_no}
            </TableCell>
            <TableCell className="min-w-40 max-w-64 whitespace-normal">
                <p className="font-medium wrap-anywhere">{name}</p>
                {spec && spec !== "无规格" && (
                    <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                        {spec}
                    </p>
                )}
            </TableCell>
            <TableCell className="text-right">
                {quantity == null ? (
                    "—"
                ) : (
                    <QuantityValue value={quantity} unit={unit} />
                )}
            </TableCell>
            <TableCell className="text-right">
                <MoneyValue
                    display="exact"
                    value={isCard ? line.face_value : line.unit_price_gross}
                />
                {isCard && (
                    <p className="mt-1 text-xs text-muted-foreground">
                        {line.card_form === "PHYSICAL"
                            ? "实体卡"
                            : line.card_form === "ELECTRONIC"
                              ? "电子券"
                              : "—"}
                    </p>
                )}
            </TableCell>
            {!isCard && (
                <TableCell className="w-72 min-w-64 max-w-80 whitespace-normal py-3">
                    <SalesLinePriceComparison
                        prices={line}
                        showSalesPrice={false}
                        compact
                        unit={unit}
                    />
                </TableCell>
            )}
            {!isCard && (
                <TableCell>{dateLabel(line.fulfillment_due_at)}</TableCell>
            )}
            <TableCell className="text-right">
                <RateValue
                    value={multiplyFixed(line.sales_tax_rate, "100", {
                        leftMaxScale: 6,
                        rightMaxScale: 0,
                        outputScale: 2,
                    })}
                    precision={2}
                />
                <p className="mt-1 text-xs text-muted-foreground">
                    税额 <MoneyValue display="exact" value={line.tax_amount} />
                </p>
            </TableCell>
            <TableCell className="text-right">
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
            </TableCell>
            <TableCell className="text-right">
                {line.delivery_allocations?.length ? (
                    <DeliveryAllocations
                        line={line}
                        name={name}
                        unit={unit}
                        idPrefix={idPrefix}
                    />
                ) : (
                    <span className="text-muted-foreground">0 条</span>
                )}
            </TableCell>
        </TableRow>
    )
}

const ALLOCATION_PAGE_SIZE = 20

/** 收货分配按提交顺序分页，只在弹窗打开时渲染当前页。 */
function DeliveryAllocations({
    line,
    name,
    unit,
    idPrefix,
}: {
    line: SalesApprovalLine
    name: string
    unit?: string
    idPrefix: string
}) {
    const [open, setOpen] = useState(false)
    const [page, setPage] = useState(0)
    const allocations = line.delivery_allocations ?? []
    const pageCount = Math.max(
        1,
        Math.ceil(allocations.length / ALLOCATION_PAGE_SIZE),
    )
    const currentPage = Math.min(page, pageCount - 1)
    const start = currentPage * ALLOCATION_PAGE_SIZE
    return (
        <Dialog
            open={open}
            onOpenChange={(value) => {
                setOpen(value)
                setPage(0)
            }}
        >
            <DialogTrigger
                id={`${idPrefix}-allocations`}
                render={
                    <Button
                        type="button"
                        variant="link"
                        size="sm"
                        className="h-auto p-0"
                    />
                }
                aria-label={`查看${name}的 ${allocations.length} 条收货分配`}
            >
                {allocations.length} 条
            </DialogTrigger>
            <DialogContent
                className="flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-4xl"
                closeButtonId={`${idPrefix}-allocations-close`}
            >
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle>收货分配</DialogTitle>
                    <DialogDescription className="wrap-anywhere">
                        {name} · 共 {allocations.length} 条
                    </DialogDescription>
                </DialogHeader>
                <div
                    key={currentPage}
                    id={`${idPrefix}-allocations-list`}
                    role="region"
                    aria-label="收货分配列表"
                    className="min-h-0 flex-1 overflow-auto rounded-lg border border-grid"
                >
                    <Table className="min-w-[640px]" aria-label="收货分配明细">
                        <TableHeader>
                            <TableRow>
                                <TableHead>序号</TableHead>
                                <TableHead className="text-right">
                                    数量
                                </TableHead>
                                <TableHead>收货人</TableHead>
                                <TableHead>电话</TableHead>
                                <TableHead>收货地址</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {open &&
                                allocations
                                    .slice(start, start + ALLOCATION_PAGE_SIZE)
                                    .map((allocation, index) => (
                                        <TableRow key={start + index}>
                                            <TableCell className="num text-muted-foreground">
                                                {start + index + 1}
                                            </TableCell>
                                            <TableCell className="text-right">
                                                <QuantityValue
                                                    value={allocation.quantity}
                                                    unit={unit}
                                                />
                                            </TableCell>
                                            <TableCell className="max-w-32 whitespace-normal wrap-anywhere">
                                                {allocation.recipient}
                                            </TableCell>
                                            <TableCell>
                                                {allocation.phone}
                                            </TableCell>
                                            <TableCell className="min-w-64 whitespace-normal wrap-anywhere">
                                                {fullDeliveryAddress(
                                                    allocation,
                                                )}
                                            </TableCell>
                                        </TableRow>
                                    ))}
                        </TableBody>
                    </Table>
                </div>
                <DialogFooter className="shrink-0 flex-row flex-wrap items-center sm:justify-between">
                    <p className="mr-auto text-xs text-muted-foreground">
                        第 {start + 1}–
                        {Math.min(
                            start + ALLOCATION_PAGE_SIZE,
                            allocations.length,
                        )}{" "}
                        条，共 {allocations.length} 条
                    </p>
                    {pageCount > 1 && (
                        <div className="flex items-center gap-2">
                            <Button
                                id={`${idPrefix}-allocations-previous`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={currentPage === 0}
                                onClick={() => setPage(currentPage - 1)}
                            >
                                上一页
                            </Button>
                            <span
                                className="num text-xs text-muted-foreground"
                                aria-live="polite"
                            >
                                {currentPage + 1} / {pageCount}
                            </span>
                            <Button
                                id={`${idPrefix}-allocations-next`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={currentPage + 1 >= pageCount}
                                onClick={() => setPage(currentPage + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    )}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
    return (
        <DescriptionItem>
            <DescriptionTerm>{label}</DescriptionTerm>
            <DescriptionDetails>{children}</DescriptionDetails>
        </DescriptionItem>
    )
}
