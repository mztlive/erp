"use client"

import type { ReactNode } from "react"
import {
    CheckCircle2Icon,
    ChevronRightIcon,
    FileTextIcon,
    RefreshCwIcon,
} from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Spinner } from "@/components/ui/spinner"
import { InvoiceRequestDetails } from "@/features/invoice-requests/components/request-details"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText } from "@/lib/display-name"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { useInvoiceApprovalReview } from "../hooks/use-invoice-review"
import type {
    InvoiceApprovalReview,
    InvoiceReviewRecord,
} from "../api/invoice-review"
import type { WorkspaceWorkItem } from "../types"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

const sectionClass = cn(
    workspaceTaskSurfacePadClassName,
    "space-y-3 border-b border-grid py-4",
)

/** 当前核对依据和原提交材料分区展示，只消费审批实例授权的接口。 */
export function WorkspaceInvoiceApproval({
    instanceId,
    item,
    onPreview,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
}) {
    const review = useInvoiceApprovalReview(instanceId)
    const materials = useApprovalMaterials(instanceId, true, item)
    const prefix = `workspace-invoice-approval-${toAutomationIdSegment(instanceId)}`
    const currentBasis = materials.data?.current_sales_basis
    const showCurrent =
        currentBasis &&
        (!materials.data?.display.source_sales?.length ||
            materials.data.display.source_sales.some(
                (source) =>
                    source.revision_id !== currentBasis.source.revision_id,
            ))
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
            <section className={sectionClass} aria-label="本次提交的开票资料">
                <h3 className="text-base font-semibold">本次提交的开票资料</h3>
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
            </section>
            <section className={sectionClass} aria-label="当前票款核对">
                <header className="flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-base font-semibold">当前票款核对</h3>
                    <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                        {review.data && (
                            <time
                                dateTime={new Date(
                                    review.data.read_at * 1000,
                                ).toISOString()}
                            >
                                核对于{" "}
                                {new Date(
                                    review.data.read_at * 1000,
                                ).toLocaleString("zh-CN")}
                            </time>
                        )}
                        <Button
                            id={`${prefix}-refresh`}
                            size="sm"
                            variant="ghost"
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
                </header>
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
                    <CurrentReview
                        key={instanceId}
                        data={review.data}
                        idPrefix={prefix}
                    />
                )}
            </section>
            <details
                key={instanceId}
                id={`${prefix}-sales-basis`}
                className={sectionClass}
            >
                <summary
                    id={`${prefix}-sales-basis-toggle`}
                    className="cursor-pointer text-sm font-semibold"
                >
                    销售来源与开票依据
                </summary>
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
                            <p className="text-sm text-muted-foreground">
                                本次申请的销售依据按当前生效版本展示。
                            </p>
                        )}
                        {showCurrent && (
                            <Source
                                source={currentBasis.source}
                                frozen={false}
                                id={`${prefix}-current-sales`}
                                onOpen={() =>
                                    preview(
                                        "current-source-sales",
                                        currentBasis.source.revision_id,
                                    )
                                }
                            />
                        )}
                        {!materials.data.display.source_sales?.length &&
                            !currentBasis && (
                                <p
                                    role="alert"
                                    className="text-sm text-muted-foreground"
                                >
                                    {materials.data.current_sales_basis_error ??
                                        "销售依据暂不可读取，请刷新后重试。"}
                                </p>
                            )}
                    </>
                )}
            </details>
            <details
                key={`${instanceId}-materials`}
                id={`${prefix}-materials`}
                className={sectionClass}
            >
                <summary
                    id={`${prefix}-materials-toggle`}
                    className="cursor-pointer text-sm font-semibold"
                >
                    合同与提交凭证
                </summary>
                {materials.data?.attachments.length ? (
                    <div className="flex justify-end">
                        <Button
                            id={`${prefix}-attachments`}
                            size="sm"
                            variant="outline"
                            onClick={() => preview("attachments")}
                        >
                            <FileTextIcon aria-hidden="true" />
                            预览材料（{materials.data.attachments.length}）
                        </Button>
                    </div>
                ) : null}
                {materials.data?.attachments.length ? (
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={materials.data.attachments}
                        idPrefix={`${prefix}-submitted-file`}
                    />
                ) : null}
                {currentBasis &&
                (showCurrent || !materials.data?.attachments.length) ? (
                    <div className="space-y-2">
                        <p className="text-xs text-muted-foreground">
                            来源销售单 · V{currentBasis.source.revision_no} ·
                            当前生效版本
                        </p>
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            attachments={currentBasis.attachments}
                            sourceRevisionId={currentBasis.source.revision_id}
                            idPrefix={`${prefix}-current-file`}
                            emptyMessage={
                                currentBasis.unavailable_reason ??
                                "此销售版本未关联可读取的合同或凭证，请联系销售核对归档资料。"
                            }
                        />
                    </div>
                ) : !materials.data?.attachments.length ? (
                    <p className="text-sm text-muted-foreground">
                        {materials.isPending
                            ? "正在读取销售合同与凭证…"
                            : "销售合同与凭证读取失败，请刷新后重试。"}
                    </p>
                ) : null}
                <p className="text-xs text-muted-foreground">
                    付款条件不等同于开票时点；请结合合同约定核对。审批通过后生成开票任务，实际登记发票后才计入已开票金额。
                </p>
            </details>
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

function CurrentReview({
    data,
    idPrefix,
}: {
    data: InvoiceApprovalReview
    idPrefix: string
}) {
    const hasShortage = compareDecimal(data.amounts.shortage, "0", 2) > 0
    const hasOtherReservations =
        compareDecimal(data.amounts.other_pending, "0", 2) > 0 ||
        compareDecimal(data.amounts.other_approved, "0", 2) > 0
    const hasCurrentReservation =
        compareDecimal(data.amounts.current_reserved, "0", 2) > 0
    const amounts = [
        ["可开票总额", data.invoiceable_total],
        ["本次申请占用", data.amounts.current_reserved],
        ["扣除本次后可再申请", data.amounts.available],
    ]
    return (
        <>
            <p className="break-words text-sm text-muted-foreground">
                {data.account_label}
            </p>
            {(data.warnings.length > 0 || hasShortage) && (
                <Alert variant="destructive">
                    <AlertTitle>
                        {hasShortage ? (
                            <>
                                当前开票占用超出额度{" "}
                                <MoneyValue value={data.amounts.shortage} />
                            </>
                        ) : (
                            "审批前请核对"
                        )}
                    </AlertTitle>
                    {data.warnings.length > 0 && (
                        <AlertDescription>
                            <ul className="list-disc space-y-1 pl-4">
                                {data.warnings.map((text) => (
                                    <li key={text}>{text}</li>
                                ))}
                            </ul>
                        </AlertDescription>
                    )}
                </Alert>
            )}
            {!hasShortage && (
                <Alert
                    variant={data.warnings.length ? "default" : "success"}
                    className="rounded-lg"
                >
                    <CheckCircle2Icon aria-hidden="true" />
                    <AlertTitle>本次申请未超出开票额度</AlertTitle>
                </Alert>
            )}
            <dl className="grid gap-4 py-2 @min-[480px]/document:grid-cols-3 @min-[480px]/document:gap-0 @min-[480px]/document:divide-x @min-[480px]/document:divide-grid">
                {amounts.map(([label, value]) => (
                    <div
                        key={label}
                        className="min-w-0 @min-[480px]/document:px-5 @min-[480px]/document:first:pl-0 @min-[480px]/document:last:pr-0"
                    >
                        <dt className="text-sm text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="mt-2 break-words text-2xl font-semibold tabular-nums">
                            <MoneyValue value={value} />
                        </dd>
                    </div>
                ))}
            </dl>
            <details id={`${idPrefix}-amounts`} className="group/amounts">
                <summary
                    id={`${idPrefix}-amounts-toggle`}
                    className="flex cursor-pointer list-none flex-wrap items-center justify-between gap-2 rounded-sm py-2 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
                >
                    <span className="text-muted-foreground">
                        {hasOtherReservations
                            ? "有其他申请占用额度"
                            : hasCurrentReservation
                              ? "仅本次申请占用"
                              : "暂无申请占用额度"}
                    </span>
                    <span className="flex items-center gap-1">
                        <span className="group-open/amounts:hidden">
                            查看额度明细
                        </span>
                        <span className="hidden group-open/amounts:inline">
                            收起额度明细
                        </span>
                        <ChevronRightIcon
                            aria-hidden="true"
                            className="size-4 transition-transform group-open/amounts:rotate-90"
                        />
                    </span>
                </summary>
                <div className="space-y-3 pt-3">
                    <dl className="grid grid-cols-2 gap-x-5 gap-y-3 @min-[640px]/document:grid-cols-4">
                        <Fact label="本笔应收">
                            <MoneyValue value={data.gross_total} />
                        </Fact>
                        <Fact label="已实际开票（净额）">
                            <MoneyValue value={data.invoiced_total} />
                        </Fact>
                        <Fact label="其他审批中占用">
                            <MoneyValue value={data.amounts.other_pending} />
                        </Fact>
                        <Fact label="其他已批准待开票">
                            <MoneyValue value={data.amounts.other_approved} />
                        </Fact>
                    </dl>
                    <p className="text-sm leading-6 text-muted-foreground">
                        可再申请金额已扣除本次及其他申请的有效占用；已批准申请仅占用尚未开票部分。
                    </p>
                    <details
                        id={`${idPrefix}-related`}
                        className="border-t border-grid pt-3"
                    >
                        <summary
                            id={`${idPrefix}-related-toggle`}
                            className="cursor-pointer text-sm font-medium"
                        >
                            占用额度的开票申请（{data.request_count}）
                        </summary>
                        <ul className="mt-2 divide-y divide-grid text-sm">
                            {data.requests.map((row) => (
                                <li
                                    key={row.request_no}
                                    className="flex flex-wrap items-start justify-between gap-2 py-2"
                                >
                                    <div className="min-w-0">
                                        <p className="break-all">
                                            {row.is_current
                                                ? "本次申请"
                                                : row.request_no}
                                        </p>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {row.status} ·{" "}
                                            {new Date(
                                                row.created_at * 1000,
                                            ).toLocaleDateString("zh-CN")}
                                        </p>
                                    </div>
                                    <div className="text-right">
                                        <p>
                                            申请{" "}
                                            <MoneyValue value={row.amount} />
                                        </p>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            已开{" "}
                                            <MoneyValue
                                                value={row.invoiced_amount}
                                            />
                                        </p>
                                    </div>
                                </li>
                            ))}
                        </ul>
                        {data.request_count > data.requests.length && (
                            <p className="text-xs text-muted-foreground">
                                仅展示最近 {data.requests.length}{" "}
                                条；上方占用金额包含全部有效申请。
                            </p>
                        )}
                    </details>
                    <RecordList
                        id={`${idPrefix}-invoices`}
                        title="已开发票记录"
                        count={data.records.invoice_count}
                        rows={data.records.invoices}
                    />
                </div>
            </details>
            <details
                id={`${idPrefix}-collection`}
                className="group/collection border-t border-grid pt-3"
            >
                <summary
                    id={`${idPrefix}-collection-toggle`}
                    className="flex cursor-pointer list-none flex-wrap items-center gap-x-5 gap-y-2 rounded-sm py-1 outline-none focus-visible:ring-2 focus-visible:ring-ring [&::-webkit-details-marker]:hidden"
                >
                    <span className="self-start text-sm font-semibold leading-6">
                        回款与履约
                    </span>
                    <span className="min-w-0 flex-1 basis-48 text-sm leading-6">
                        <span className="flex flex-wrap items-center gap-x-2">
                            <span>
                                本笔未回款{" "}
                                <MoneyValue value={data.open_total} />
                            </span>
                            <span
                                className="text-muted-foreground"
                                aria-hidden="true"
                            >
                                ·
                            </span>
                            <span>履约：{data.fulfillment_status}</span>
                        </span>
                        <span className="mt-1 block text-muted-foreground">
                            按合同约定核对开票时点。
                        </span>
                    </span>
                    <span className="ml-auto flex items-center gap-1 text-sm">
                        <span className="group-open/collection:hidden">
                            查看详情
                        </span>
                        <span className="hidden group-open/collection:inline">
                            收起详情
                        </span>
                        <ChevronRightIcon
                            aria-hidden="true"
                            className="size-4 transition-transform group-open/collection:rotate-90"
                        />
                    </span>
                </summary>
                <div className="space-y-3 pt-4">
                    <dl className="grid grid-cols-2 gap-x-5 gap-y-3 @min-[640px]/document:grid-cols-4">
                        <Fact label="本笔已核销回款">
                            <MoneyValue value={data.settled_total} />
                        </Fact>
                        <Fact label="销售状态">{data.sales_status}</Fact>
                        <Fact label="履约进度">{data.fulfillment_status}</Fact>
                        <Fact label="负责销售">
                            {data.sales_owner || "未记录"}
                        </Fact>
                    </dl>
                    <p className="text-sm leading-6 text-muted-foreground">
                        仅统计已核销至本笔应收的回款；未回款或未履约不自动代表不能开票。
                    </p>
                    <RecordList
                        id={`${idPrefix}-receipts`}
                        title="相关回款记录"
                        count={data.records.receipt_count}
                        rows={data.records.receipts}
                    />
                </div>
            </details>
        </>
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
                    className="h-auto max-w-full whitespace-normal p-0 text-left text-sm font-medium"
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
            </div>
            <p className="text-xs text-muted-foreground">
                {frozen
                    ? "提交申请时的销售单内容"
                    : "当前生效销售版本，点击单号查看完整明细与合同凭证"}
            </p>
            <dl className="grid gap-x-6 gap-y-3 @min-[640px]/document:grid-cols-2">
                <Fact label="销售客户">
                    {source.source.customer || "未记录"}
                </Fact>
                <Fact label="整单含税金额">
                    {source.source.amount_label || "未记录"}
                </Fact>
                {source.source.extra_sections.map((field) => (
                    <Fact key={field.label} label={field.label}>
                        {field.value}
                    </Fact>
                ))}
            </dl>
            <ul className="divide-y divide-grid border-t border-grid text-sm">
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
            {source.source.more_count > 0 && (
                <p className="text-xs text-muted-foreground">
                    另有 {source.source.more_count} 行未在摘要中展开
                    ，请查看完整销售单核对。
                </p>
            )}
        </div>
    )
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
    return (
        <div className="min-w-0">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="mt-1 break-words text-sm">{children}</dd>
        </div>
    )
}
function Loading({ children }: { children: ReactNode }) {
    return (
        <p
            role="status"
            className="flex items-center gap-2 text-sm text-muted-foreground"
        >
            <Spinner />
            {children}
        </p>
    )
}

function RecordList({
    id,
    title,
    count,
    rows,
}: {
    id: string
    title: string
    count: number
    rows: InvoiceReviewRecord[]
}) {
    return (
        <details id={id} className="border-t border-grid pt-3">
            <summary
                id={`${id}-toggle`}
                className="cursor-pointer text-sm font-medium"
            >
                {title}（{count}）
            </summary>
            {rows.length ? (
                <ul className="mt-2 divide-y divide-grid text-sm">
                    {rows.map((row) => (
                        <li
                            key={`${row.number}:${row.status}`}
                            className="flex flex-wrap justify-between gap-2 py-2"
                        >
                            <div className="min-w-0">
                                <p className="break-all">{row.number}</p>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {row.status} ·{" "}
                                    {new Date(row.date).toLocaleDateString(
                                        "zh-CN",
                                    )}
                                </p>
                            </div>
                            <p>
                                本笔净分配 <MoneyValue value={row.amount} />
                            </p>
                        </li>
                    ))}
                </ul>
            ) : (
                <p className="mt-2 text-sm text-muted-foreground">
                    暂无已登记记录
                </p>
            )}
            <p className="mt-1 text-xs text-muted-foreground">
                仅展示本笔应收的净分配金额
                {count > rows.length ? `，最近 ${rows.length} 条` : ""}。
            </p>
        </details>
    )
}
