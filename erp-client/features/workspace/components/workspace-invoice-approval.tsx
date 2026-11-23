"use client"

import { useState, type ReactNode } from "react"
import {
    CheckCircle2Icon,
    FileTextIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    DetailWorkspaceFact,
    DetailWorkspaceFacts,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    DetailWorkspaceSubTabs,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { InvoiceRequestDetails } from "@/features/invoice-requests/components/request-details"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import {
    displayDateString,
    displayUnixDate,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText } from "@/lib/display-name"
import { compareDecimal } from "@/lib/fixed-decimal"
import { useInvoiceApprovalReview } from "../hooks/use-invoice-review"
import type {
    InvoiceApprovalReview,
    InvoiceReviewRecord,
} from "../api/invoice-review"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

type EvidenceTabId = "source-sales" | "receipts" | "invoices"

/** 当前核对依据和原提交材料扁平分区展示，零嵌套折叠，只消费审批实例授权的接口。 */
export function WorkspaceInvoiceApproval({
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
    const review = useInvoiceApprovalReview(instanceId)
    const materials = useApprovalMaterials(instanceId, true, item)
    const [activeTab, setActiveTab] = useState<EvidenceTabId>("source-sales")
    const prefix = `workspace-invoice-approval-${toAutomationIdSegment(instanceId)}`
    const currentBasis = materials.data?.current_sales_basis
    const showCurrent =
        currentBasis &&
        (!materials.data?.display.source_sales?.length ||
            materials.data.display.source_sales.some(
                (source) =>
                    source.revision_id !== currentBasis.source.revision_id,
            ))
    const sourceCount =
        (materials.data?.display.source_sales?.length ?? 0) +
        (showCurrent ? 1 : 0)
    const attachmentCount =
        (materials.data?.attachments.length ?? 0) +
        (currentBasis && (showCurrent || !materials.data?.attachments.length)
            ? currentBasis.attachments.length
            : 0)
    const receiptCount = review.data?.records.receipt_count ?? 0
    const invoiceHistoryCount =
        (review.data?.request_count ?? 0) +
        (review.data?.records.invoice_count ?? 0)

    const preview = (
        view: "source-sales" | "attachments" | "current-source-sales",
        revisionId?: string,
    ) =>
        onPreview({
            kind: "approval_snapshot",
            objectId: instanceId,
            materialsView: view,
            sourceRevisionId: revisionId,
        })

    return (
        <>
            <DetailWorkspaceSection
                divider={false}
                title="实时票款与额度核验"
                ariaLabel="实时票款与额度核验"
                actions={
                    <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        {review.data ? (
                            <time
                                className="num"
                                dateTime={
                                    displayUnixSeconds(review.data.read_at)
                                        ?.dateTime
                                }
                            >
                                核对于{" "}
                                {displayUnixSeconds(review.data.read_at)
                                    ?.label ?? "—"}
                            </time>
                        ) : null}
                        <Button
                            id={`${prefix}-refresh`}
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                            disabled={review.isFetching}
                            onClick={() => {
                                void review.refetch()
                                void materials.refetch()
                            }}
                        >
                            <RefreshCwIcon aria-hidden="true" />
                            刷新票款
                        </Button>
                    </div>
                }
            >
                {review.isPending ? (
                    <Loading>正在读取开票额度与回款…</Loading>
                ) : review.isError ? (
                    <BusinessFailureState
                        id={`${prefix}-retry`}
                        title="票款核对依据读取失败"
                        error={review.error}
                        onRetry={() => void review.refetch()}
                    />
                ) : (
                    <VerificationStrip data={review.data} />
                )}
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                title="本次提交的开票资料"
                ariaLabel="本次提交的开票资料"
            >
                {materials.isPending ? (
                    <Loading>正在读取开票资料…</Loading>
                ) : materials.isError ? (
                    <BusinessFailureState
                        id={`${prefix}-invoice-request-retry`}
                        title="开票资料读取失败"
                        error={materials.error}
                        onRetry={() => void materials.refetch()}
                    />
                ) : materials.data ? (
                    <InvoiceRequestDetails
                        data={submittedInvoiceDetails(materials.data)}
                    />
                ) : null}
            </DetailWorkspaceSection>

            <DetailWorkspaceSection title="核对依据" ariaLabel="开票核对依据">
                <DetailWorkspaceSubTabs
                    value={activeTab}
                    onValueChange={(next) =>
                        setActiveTab(next as EvidenceTabId)
                    }
                    idPrefix={`${prefix}-tab`}
                    ariaLabel="开票核对依据切换"
                    className="pt-0"
                    items={[
                        {
                            id: "source-sales",
                            label: "来源销售单",
                            badge:
                                sourceCount > 0 ? (
                                    <Badge
                                        variant="neutral"
                                        className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                                    >
                                        {sourceCount}
                                    </Badge>
                                ) : undefined,
                        },
                        {
                            id: "receipts",
                            label: "关联回款",
                            badge: review.data ? (
                                <Badge
                                    variant="neutral"
                                    className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                                >
                                    {receiptCount}
                                </Badge>
                            ) : undefined,
                        },
                        {
                            id: "invoices",
                            label: "历史开票",
                            badge: review.data ? (
                                <Badge
                                    variant="neutral"
                                    className="rounded-full px-1.5 py-0 text-tiny tabular-nums"
                                >
                                    {invoiceHistoryCount}
                                </Badge>
                            ) : undefined,
                        },
                    ]}
                />

                {activeTab === "source-sales" ? (
                    <div
                        id={`${prefix}-panel-source-sales`}
                        role="region"
                        aria-label="来源销售单"
                        className="space-y-5 pt-1"
                    >
                        {materials.isPending ? (
                            <Loading>正在读取提交时销售依据…</Loading>
                        ) : materials.isError ? (
                            <BusinessFailureState
                                id={`${prefix}-materials-retry`}
                                title="提交依据读取失败"
                                error={materials.error}
                                onRetry={() => void materials.refetch()}
                            />
                        ) : (
                            <>
                                {materials.data.display.source_sales?.length ? (
                                    materials.data.display.source_sales.map(
                                        (source) => (
                                            <Source
                                                key={source.revision_id}
                                                source={source}
                                                frozen
                                                id={`${prefix}-sales-${toAutomationIdSegment(source.revision_id)}`}
                                                onOpen={() =>
                                                    preview(
                                                        "source-sales",
                                                        source.revision_id,
                                                    )
                                                }
                                            />
                                        ),
                                    )
                                ) : (
                                    <p className="text-xs text-muted-foreground">
                                        本次申请的销售依据按当前生效版本展示。
                                    </p>
                                )}
                                {showCurrent ? (
                                    <div className="border-t border-border/60 pt-4">
                                        <Source
                                            source={currentBasis.source}
                                            frozen={false}
                                            id={`${prefix}-current-sales`}
                                            onOpen={() =>
                                                preview(
                                                    "current-source-sales",
                                                    currentBasis.source
                                                        .revision_id,
                                                )
                                            }
                                        />
                                    </div>
                                ) : null}
                                {!materials.data.display.source_sales?.length &&
                                !currentBasis ? (
                                    <p
                                        role="alert"
                                        className="text-xs text-muted-foreground"
                                    >
                                        {materials.data
                                            .current_sales_basis_error ??
                                            "销售依据暂不可读取，请刷新后重试。"}
                                    </p>
                                ) : null}
                            </>
                        )}
                    </div>
                ) : null}

                {activeTab === "receipts" ? (
                    <div
                        id={`${prefix}-panel-receipts`}
                        role="region"
                        aria-label="关联回款"
                        className="space-y-4 pt-1"
                    >
                        {review.isPending ? (
                            <Loading>正在读取回款与履约记录…</Loading>
                        ) : review.isError ? (
                            <BusinessFailureState
                                id={`${prefix}-receipts-retry`}
                                title="回款与履约记录读取失败"
                                error={review.error}
                                onRetry={() => void review.refetch()}
                            />
                        ) : (
                            <ReceiptsTabPanel data={review.data} />
                        )}
                    </div>
                ) : null}

                {activeTab === "invoices" ? (
                    <div
                        id={`${prefix}-panel-invoices`}
                        role="region"
                        aria-label="历史开票"
                        className="space-y-4 pt-1"
                    >
                        {review.isPending ? (
                            <Loading>正在读取历史开票与占用明细…</Loading>
                        ) : review.isError ? (
                            <BusinessFailureState
                                id={`${prefix}-invoices-retry`}
                                title="历史开票与占用明细读取失败"
                                error={review.error}
                                onRetry={() => void review.refetch()}
                            />
                        ) : (
                            <InvoicesTabPanel data={review.data} />
                        )}
                    </div>
                ) : null}
            </DetailWorkspaceSection>

            {!hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    title="合同与提交凭证"
                    subtitle={
                        attachmentCount > 0
                            ? `共 ${attachmentCount} 个文件`
                            : undefined
                    }
                    ariaLabel="合同与提交凭证"
                >
                    <div className="space-y-3">
                        {materials.data?.attachments.length ? (
                            <ApprovalMaterialFiles
                                instanceId={instanceId}
                                attachments={materials.data.attachments}
                                idPrefix={`${prefix}-submitted-file`}
                                onSelectFile={onSelectFile}
                                activeFileId={activeFileId}
                            />
                        ) : null}
                        {currentBasis &&
                        (showCurrent || !materials.data?.attachments.length) ? (
                            <div className="space-y-2">
                                <p className="text-xs text-muted-foreground">
                                    来源销售单 · V
                                    {currentBasis.source.revision_no} ·
                                    当前生效版本
                                </p>
                                <ApprovalMaterialFiles
                                    instanceId={instanceId}
                                    attachments={currentBasis.attachments}
                                    sourceRevisionId={
                                        currentBasis.source.revision_id
                                    }
                                    idPrefix={`${prefix}-current-file`}
                                    onSelectFile={onSelectFile}
                                    activeFileId={activeFileId}
                                    emptyMessage={
                                        currentBasis.unavailable_reason ??
                                        "此销售版本未关联可读取的合同或凭证，请联系销售核对归档资料。"
                                    }
                                />
                            </div>
                        ) : !materials.data?.attachments.length ? (
                            <p className="text-xs text-muted-foreground">
                                {materials.isPending
                                    ? "正在读取销售合同与凭证…"
                                    : "暂无提交附件或销售合同凭证，请核对后处理。"}
                            </p>
                        ) : null}
                        <p className="text-xs text-muted-foreground">
                            付款条件不等同于开票时点；请结合合同约定核对。审批通过后生成开票任务，实际登记发票后才计入已开票金额。
                        </p>
                    </div>
                </DetailWorkspaceSection>
            ) : null}
        </>
    )
}

function submittedInvoiceDetails(materials: ApprovalMaterials) {
    if (materials.display.invoice_request)
        return materials.display.invoice_request
    const fields = new Map(
        materials.display.source.extra_sections.map(({ label, value }) => [
            label,
            value,
        ]),
    )
    return {
        invoice_title:
            fields.get("购方名称") ??
            fields.get("开票抬头") ??
            materials.display.source.customer ??
            "",
        tax_number: fields.get("购方纳税人识别号") ?? fields.get("税号") ?? "",
        seller_name: fields.get("销方名称"),
        seller_tax_number: fields.get("销方纳税人识别号"),
        invoice_content: fields.get("开票内容") ?? "",
        reason: fields.get("申请事由") ?? "",
    }
}

function VerificationStrip({ data }: { data: InvoiceApprovalReview }) {
    const hasShortage = compareDecimal(data.amounts.shortage, "0", 2) > 0
    return (
        <div className="space-y-3.5">
            <p className="break-words text-xs text-muted-foreground">
                {data.account_label}
            </p>
            {hasShortage ? (
                <Alert variant="destructive" role="alert">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        当前开票占用超出额度{" "}
                        <MoneyValue value={data.amounts.shortage} />
                    </AlertTitle>
                    {data.warnings.length > 0 ? (
                        <AlertDescription>
                            <ul className="list-disc space-y-1 pl-4">
                                {data.warnings.map((text) => (
                                    <li key={text}>{text}</li>
                                ))}
                            </ul>
                        </AlertDescription>
                    ) : null}
                </Alert>
            ) : data.warnings.length > 0 ? (
                <Alert variant="warning">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>
                        本次申请未超出开票额度，审批前请核对以下提示
                    </AlertTitle>
                    <AlertDescription>
                        <ul className="list-disc space-y-1 pl-4">
                            {data.warnings.map((text) => (
                                <li key={text}>{text}</li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert variant="success">
                    <CheckCircle2Icon aria-hidden="true" />
                    <AlertTitle>本次申请未超出开票额度</AlertTitle>
                </Alert>
            )}
            <DetailWorkspaceMetrics
                columns={4}
                variant="inline"
                ariaLabel="开票额度与回款摘要"
            >
                <DetailWorkspaceMetricItem
                    label="可开票总额"
                    value={<MoneyValue value={data.invoiceable_total} />}
                />
                <DetailWorkspaceMetricItem
                    label="本次申请占用"
                    emphasis="primary"
                    value={<MoneyValue value={data.amounts.current_reserved} />}
                />
                <DetailWorkspaceMetricItem
                    label="扣除本次后可再申请"
                    value={<MoneyValue value={data.amounts.available} />}
                />
                <DetailWorkspaceMetricItem
                    label="已核销回款 / 未回款"
                    value={<MoneyValue value={data.settled_total} />}
                    detail={
                        <span>
                            未回款 <MoneyValue value={data.open_total} />
                        </span>
                    }
                />
            </DetailWorkspaceMetrics>
        </div>
    )
}

function ReceiptsTabPanel({ data }: { data: InvoiceApprovalReview }) {
    return (
        <>
            <DetailWorkspaceFacts columns={4} ariaLabel="回款与履约概况">
                <DetailWorkspaceFact
                    label="本笔已核销回款"
                    numeric
                    value={<MoneyValue value={data.settled_total} />}
                />
                <DetailWorkspaceFact
                    label="本笔未回款"
                    numeric
                    value={<MoneyValue value={data.open_total} />}
                />
                <DetailWorkspaceFact
                    label="销售状态"
                    value={data.sales_status}
                />
                <DetailWorkspaceFact
                    label="履约进度"
                    value={data.fulfillment_status}
                />
                <DetailWorkspaceFact
                    label="负责销售"
                    value={data.sales_owner || "未记录"}
                />
            </DetailWorkspaceFacts>
            <p className="text-xs leading-5 text-muted-foreground">
                仅统计已核销至本笔应收的回款；未回款或未履约不自动代表不能开票，请结合合同约定核对开票时点。
            </p>
            <FlatRecordTable
                title="相关回款记录"
                count={data.records.receipt_count}
                rows={data.records.receipts}
                numberHeader="回款单号"
            />
        </>
    )
}

function InvoicesTabPanel({ data }: { data: InvoiceApprovalReview }) {
    const hasOtherReservations =
        compareDecimal(data.amounts.other_pending, "0", 2) > 0 ||
        compareDecimal(data.amounts.other_approved, "0", 2) > 0
    const hasCurrentReservation =
        compareDecimal(data.amounts.current_reserved, "0", 2) > 0

    return (
        <>
            <DetailWorkspaceFacts columns={4} ariaLabel="开票额度分解">
                <DetailWorkspaceFact
                    label="本笔应收"
                    numeric
                    value={<MoneyValue value={data.gross_total} />}
                />
                <DetailWorkspaceFact
                    label="已实际开票（净额）"
                    numeric
                    value={<MoneyValue value={data.invoiced_total} />}
                />
                <DetailWorkspaceFact
                    label="其他审批中占用"
                    numeric
                    value={<MoneyValue value={data.amounts.other_pending} />}
                />
                <DetailWorkspaceFact
                    label="其他已批准待开票"
                    numeric
                    value={<MoneyValue value={data.amounts.other_approved} />}
                />
            </DetailWorkspaceFacts>
            <p className="text-xs leading-5 text-muted-foreground">
                {hasOtherReservations
                    ? "当前存在其他开票申请占用额度；"
                    : hasCurrentReservation
                      ? "当前仅本次申请占用额度；"
                      : "当前暂无有效申请占用额度；"}
                可再申请金额已扣除本次及其他申请的有效占用，已批准申请仅占用尚未开票部分。
            </p>

            <div className="space-y-2 border-t border-border/60 pt-3.5">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <h3 className="text-body-compact font-semibold">
                        占用额度的开票申请（{data.request_count}）
                    </h3>
                    {data.request_count > data.requests.length ? (
                        <span className="text-xs text-muted-foreground">
                            仅展示最近 {data.requests.length}{" "}
                            条；上方占用金额包含全部有效申请
                        </span>
                    ) : null}
                </div>
                {data.requests.length > 0 ? (
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.inlineTableWrapper}
                    >
                        <Table
                            aria-label="占用额度的开票申请"
                            className="w-full [&_:is(th,td):first-child]:ps-0"
                        >
                            <TableHeader>
                                <TableRow>
                                    <TableHead scope="col">申请单号</TableHead>
                                    <TableHead scope="col">状态</TableHead>
                                    <TableHead scope="col">申请日期</TableHead>
                                    <TableHead scope="col" data-align="end">
                                        申请金额
                                    </TableHead>
                                    <TableHead scope="col" data-align="end">
                                        已开金额
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {data.requests.map((row) => (
                                    <TableRow
                                        key={row.request_no}
                                        className={
                                            row.is_current
                                                ? "bg-info-soft/30"
                                                : undefined
                                        }
                                    >
                                        <TableCell className="num whitespace-normal break-all font-medium">
                                            {row.is_current ? (
                                                <span className="inline-flex flex-wrap items-center gap-1.5">
                                                    <span>
                                                        {row.request_no}
                                                    </span>
                                                    <Badge variant="info">
                                                        本次申请
                                                    </Badge>
                                                </span>
                                            ) : (
                                                row.request_no
                                            )}
                                        </TableCell>
                                        <TableCell className="text-xs text-muted-foreground">
                                            {row.status}
                                        </TableCell>
                                        <TableCell className="num text-xs text-muted-foreground">
                                            {displayUnixDate(row.created_at)}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="font-medium"
                                        >
                                            <MoneyValue value={row.amount} />
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="text-muted-foreground"
                                        >
                                            <MoneyValue
                                                value={row.invoiced_amount}
                                            />
                                        </TableCell>
                                    </TableRow>
                                ))}
                            </TableBody>
                        </Table>
                    </div>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        暂无占用额度的开票申请
                    </p>
                )}
            </div>

            <FlatRecordTable
                title="已开发票记录"
                count={data.records.invoice_count}
                rows={data.records.invoices}
                numberHeader="发票单号 / 编号"
            />
        </>
    )
}

function FlatRecordTable({
    title,
    count,
    rows,
    numberHeader,
}: {
    title: string
    count: number
    rows: InvoiceReviewRecord[]
    numberHeader: string
}) {
    return (
        <div className="space-y-2 border-t border-border/60 pt-3.5">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h3 className="text-body-compact font-semibold">
                    {title}（{count}）
                </h3>
                <span className="text-xs text-muted-foreground">
                    仅展示本笔应收的净分配金额
                    {count > rows.length ? `（最近 ${rows.length} 条）` : ""}
                </span>
            </div>
            {rows.length > 0 ? (
                <div
                    data-slot="detail-inline-table"
                    className={detailWorkspaceStyles.inlineTableWrapper}
                >
                    <Table
                        aria-label={title}
                        className="w-full [&_:is(th,td):first-child]:ps-0"
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead scope="col">
                                    {numberHeader}
                                </TableHead>
                                <TableHead scope="col">状态</TableHead>
                                <TableHead scope="col">日期</TableHead>
                                <TableHead scope="col" data-align="end">
                                    本笔净分配
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {rows.map((row, index) => (
                                <TableRow
                                    key={`${row.number}:${row.status}:${index}`}
                                >
                                    <TableCell className="num whitespace-normal break-all font-medium">
                                        {row.number}
                                    </TableCell>
                                    <TableCell className="text-xs text-muted-foreground">
                                        {row.status}
                                    </TableCell>
                                    <TableCell className="num text-xs text-muted-foreground">
                                        {displayDateString(row.date)}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="font-medium"
                                    >
                                        <MoneyValue value={row.amount} />
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            ) : (
                <p className="text-xs text-muted-foreground">暂无已登记记录</p>
            )}
        </div>
    )
}

function Source({
    source,
    frozen,
    id,
    onOpen,
}: {
    source: NonNullable<ApprovalMaterials["display"]["source_sales"]>[number]
    frozen: boolean
    id: string
    onOpen: () => void
}) {
    return (
        <div className="space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <Button
                    id={id}
                    variant="link"
                    className="h-auto max-w-full whitespace-normal p-0 text-left text-body-compact font-medium"
                    onClick={onOpen}
                >
                    <FileTextIcon aria-hidden="true" />
                    {displayBusinessText(
                        source.document_no,
                        source.document_id,
                    ) || "销售单号未记录"}{" "}
                    · V{source.revision_no}
                    <span className="text-xs font-normal">查看销售单</span>
                </Button>
                <Badge variant={frozen ? "neutral" : "info"}>
                    {frozen ? "提交时版本" : "当前生效版本"}
                </Badge>
            </div>
            <p className="text-xs text-muted-foreground">
                {frozen
                    ? "提交申请时的销售单内容"
                    : "当前生效销售版本，点击单号查看完整明细与合同凭证"}
            </p>
            <DetailWorkspaceFacts columns={2}>
                <DetailWorkspaceFact
                    label="销售客户"
                    value={source.source.customer || "未记录"}
                />
                <DetailWorkspaceFact
                    label="整单含税金额"
                    numeric
                    value={source.source.amount_label || "未记录"}
                />
                {source.source.extra_sections.map((field) => (
                    <DetailWorkspaceFact
                        key={field.label}
                        label={field.label}
                        value={field.value}
                    />
                ))}
            </DetailWorkspaceFacts>
            {source.source.lines.length > 0 ? (
                <ul className="divide-y divide-grid border-t border-grid text-body-compact">
                    {source.source.lines.map((line, index) => (
                        <li
                            key={`${index}:${line.title}`}
                            className="space-y-1 py-2"
                        >
                            <p>{line.title}</p>
                            <p className="text-xs text-muted-foreground">
                                {[line.quantity, line.due_label]
                                    .filter(Boolean)
                                    .join(" · ")}
                            </p>
                        </li>
                    ))}
                </ul>
            ) : null}
            {source.source.more_count > 0 ? (
                <p className="text-xs text-muted-foreground">
                    另有 {source.source.more_count} 行未在摘要中展开，
                    请查看完整销售单核对。
                </p>
            ) : null}
        </div>
    )
}

function Loading({ children }: { children: ReactNode }) {
    return (
        <p
            role="status"
            className="flex items-center gap-2 text-xs text-muted-foreground"
        >
            <Spinner />
            {children}
        </p>
    )
}
