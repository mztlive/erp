"use client"

import type { ReactNode } from "react"
import { ChevronRightIcon, FileTextIcon } from "lucide-react"
import {
    BusinessFailureState,
    MoneyValue,
    QuantityValue,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableHeader,
    TableBody,
    TableHead,
    TableRow,
    TableCell,
    TableCaption,
} from "@/components/ui/table"
import { Spinner } from "@/components/ui/spinner"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

/** 采购审批只消费实例授权的提交资料；来源入口保持在同一审批上下文。 */
export function WorkspacePurchaseApproval({
    instanceId,
    onPreview,
    children,
}: {
    instanceId: string
    onPreview: (target: WorkspacePaperTarget) => void
    children: ReactNode
}) {
    const query = useApprovalMaterials(instanceId, true)
    const prefix = `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
    const sectionClass = cn(
        workspaceTaskSurfacePadClassName,
        "border-b border-grid py-4",
    )
    const preview = (view: "all" | "source-sales", revisionId?: string) =>
        onPreview({
            kind: "approval_snapshot",
            objectId: instanceId,
            materialsView: view,
            sourceRevisionId: revisionId,
        })

    if (query.isPending || query.isFetching)
        return (
            <>
                <section
                    className={sectionClass}
                    aria-label="来源销售与采购依据"
                    role="status"
                >
                    <p className="flex items-center gap-2 text-sm text-muted-foreground">
                        <Spinner />
                        正在读取来源销售与采购依据…
                    </p>
                </section>
                {children}
            </>
        )
    if (query.isError)
        return (
            <>
                <section className={sectionClass}>
                    <BusinessFailureState
                        id={`${prefix}-retry`}
                        title="来源销售与采购依据读取失败"
                        error={query.error}
                        onRetry={() => void query.refetch()}
                    />
                </section>
                {children}
            </>
        )

    const { display, purchase_lines: lines } = query.data
    const sources = display.source_sales ?? []
    return (
        <>
            <section
                className={cn(sectionClass, "space-y-4 bg-info-soft/40")}
                aria-label="来源销售与采购依据"
            >
                <header className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                    <h3 className="text-sm font-semibold">
                        来源销售与采购依据
                    </h3>
                    <p className="text-xs text-muted-foreground">
                        依据本次提交锁定的销售版本
                    </p>
                </header>
                {sources.length ? (
                    sources.map((sales) => (
                        <div
                            key={sales.revision_id}
                            className="grid min-w-0 gap-4 @min-[640px]/document:grid-cols-[minmax(0,1fr)_minmax(12rem,0.7fr)]"
                        >
                            <dl className="grid min-w-0 grid-cols-[5rem_minmax(0,1fr)] content-start gap-x-4 gap-y-3 text-sm">
                                <dt className="text-muted-foreground">客户</dt>
                                <dd className="min-w-0 font-semibold wrap-anywhere">
                                    {displayName(sales.source.customer) ||
                                        "未记录客户名称"}
                                </dd>
                                <dt className="text-muted-foreground">
                                    销售单号
                                </dt>
                                <dd className="min-w-0 wrap-anywhere">
                                    <span className="num">
                                        {displayBusinessText(
                                            sales.document_no,
                                            sales.document_id,
                                        ) || "未记录销售单号"}
                                    </span>
                                    <span className="ml-2 text-muted-foreground">
                                        销售版本 V{sales.revision_no}
                                    </span>
                                </dd>
                            </dl>
                            <div className="flex min-w-0 flex-col justify-center">
                                <Button
                                    id={`${prefix}-source-${toAutomationIdSegment(sales.revision_id)}`}
                                    variant="outline"
                                    className="h-auto min-h-9 w-full justify-start whitespace-normal py-2 text-left"
                                    onClick={() =>
                                        preview(
                                            "source-sales",
                                            sales.revision_id,
                                        )
                                    }
                                >
                                    <FileTextIcon aria-hidden="true" />
                                    预览销售单
                                    <ChevronRightIcon
                                        className="ml-auto"
                                        aria-hidden="true"
                                    />
                                </Button>
                            </div>
                            {sales.source.extra_sections
                                .filter((field) => field.label === "合同或凭证")
                                .map((field) => (
                                    <p
                                        key={field.label}
                                        className="text-sm text-warning-soft-foreground @min-[640px]/document:col-span-2"
                                    >
                                        {field.value}
                                    </p>
                                ))}
                        </div>
                    ))
                ) : (
                    <p className="text-sm text-muted-foreground">
                        此次提交未保留来源销售资料，请联系申请人核对后再审批。
                    </p>
                )}
            </section>

            <section
                className={cn(sectionClass, "space-y-4")}
                aria-label="采购与销售对照"
            >
                <header className="flex items-baseline gap-2">
                    <h3 className="text-sm font-semibold">采购与销售对照</h3>
                    {lines && (
                        <span className="text-xs text-muted-foreground">
                            {lines.length} 行
                        </span>
                    )}
                </header>
                {lines?.length ? (
                    <div className="space-y-5">
                        {lines.map((line) => (
                            <PurchaseComparison
                                key={line.id}
                                line={line}
                                sourceLabel={
                                    sources.length > 1
                                        ? sources.find(
                                              (source) =>
                                                  source.revision_id ===
                                                  line.source?.revision_id,
                                          )
                                        : undefined
                                }
                            />
                        ))}
                    </div>
                ) : (
                    <>
                        <p className="text-sm text-muted-foreground">
                            此次提交暂无可用的逐行对照，请核对提交资料及附件。
                        </p>
                        {display.source.lines.length > 0 && (
                            <ul className="divide-y text-sm">
                                {display.source.lines.map((line, index) => (
                                    <li
                                        key={`${line.title}:${index}`}
                                        className="flex flex-wrap justify-between gap-2 py-2"
                                    >
                                        <span>
                                            {displayName(line.title) ||
                                                "未记录商品名称"}
                                        </span>
                                        <span className="text-muted-foreground">
                                            {[line.quantity, line.due_label]
                                                .filter(Boolean)
                                                .join(" · ")}
                                        </span>
                                    </li>
                                ))}
                            </ul>
                        )}
                        {display.source.more_count > 0 && (
                            <p className="text-xs text-muted-foreground">
                                另有 {display.source.more_count}{" "}
                                行未包含在摘要中，请核对提交资料及附件。
                            </p>
                        )}
                        <Button
                            id={`${prefix}-submission`}
                            variant="outline"
                            size="sm"
                            onClick={() => preview("all")}
                        >
                            <FileTextIcon aria-hidden="true" />
                            查看提交资料
                        </Button>
                    </>
                )}
            </section>
            {children}
        </>
    )
}

function PurchaseComparison({
    line,
    sourceLabel,
}: {
    line: ApprovalPurchaseLine
    sourceLabel?: {
        document_no: string
        document_id: string
        revision_no: number
    }
}) {
    const source = line.source
    const title = (name: string, spec: string | null) =>
        [
            displayName(name) || "未记录商品名称",
            spec && spec !== "无规格" ? spec : null,
        ]
            .filter(Boolean)
            .join(" · ")
    const rows: { label: string; sales: ReactNode; purchase: ReactNode }[] = [
        {
            label: "商品",
            sales: source ? title(source.title, source.specification) : "—",
            purchase: title(line.title, line.specification),
        },
        {
            label: "数量",
            sales: source ? (
                <QuantityValue value={source.quantity} unit={source.unit} />
            ) : (
                "—"
            ),
            purchase:
                line.quantity != null ? (
                    <QuantityValue
                        value={line.quantity}
                        unit={line.unit ?? ""}
                    />
                ) : (
                    "—"
                ),
        },
        {
            label: "含税单价",
            sales: <MoneyValue value={source?.unit_price_gross} />,
            purchase: <MoneyValue value={line.unit_cost_gross} />,
        },
        {
            label: "交付日期",
            sales: source
                ? new Date(source.fulfillment_due_at * 1000).toLocaleDateString(
                      "sv-SE",
                      {
                          timeZone: "Asia/Shanghai",
                          year: "numeric",
                          month: "2-digit",
                          day: "2-digit",
                      },
                  )
                : "—",
            purchase: line.expected_delivery_date ?? "—",
        },
    ]
    return (
        <article className="min-w-0 space-y-2">
            {sourceLabel && (
                <p className="text-xs text-muted-foreground">
                    {displayBusinessText(
                        sourceLabel.document_no,
                        sourceLabel.document_id,
                    ) || "未记录销售单号"}{" "}
                    · 销售版本 V{sourceLabel.revision_no}
                </p>
            )}
            <div className="overflow-hidden rounded-md border border-grid">
                <Table className="w-full table-fixed text-left text-sm">
                    <TableCaption className="sr-only">
                        第 {line.line_no} 行：
                        {title(line.title, line.specification)}的采购与销售对照
                    </TableCaption>
                    <TableHeader className="bg-muted/50">
                        <TableRow>
                            <TableHead
                                scope="col"
                                className="h-auto w-20 px-3 py-1.5 font-medium text-foreground whitespace-normal"
                            >
                                核对项
                            </TableHead>
                            <TableHead
                                scope="col"
                                className="h-auto border-l border-grid px-3 py-1.5 font-medium text-foreground whitespace-normal"
                            >
                                来源销售
                            </TableHead>
                            <TableHead
                                scope="col"
                                className="h-auto border-l border-grid px-3 py-1.5 font-medium text-foreground whitespace-normal"
                            >
                                本次采购
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {rows.map((row) => (
                            <TableRow
                                key={row.label}
                                className="border-t border-grid"
                            >
                                <TableHead
                                    scope="row"
                                    className="h-auto bg-transparent px-3 py-1.5 align-top font-normal text-foreground whitespace-normal"
                                >
                                    {row.label}
                                </TableHead>
                                <TableCell className="h-auto border-l border-grid px-3 py-1.5 align-top whitespace-normal wrap-anywhere">
                                    {row.sales}
                                </TableCell>
                                <TableCell className="h-auto border-l border-grid px-3 py-1.5 align-top whitespace-normal wrap-anywhere">
                                    {row.purchase}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
            </div>
            <p className="text-xs leading-5 text-muted-foreground">
                {source
                    ? "销售数量为该来源明细的需求量；本次采购仅对应其中的采购数量。销售日期为客户承诺交期，采购日期为预计交付日。"
                    : "此费用行不对应销售商品明细，不计算销售价格。"}
            </p>
        </article>
    )
}
