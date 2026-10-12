"use client"

import { ApprovalMaterialFiles } from "./approval-material-files"
import { ApprovalSalesOrderPaper } from "./approval-sales-order-paper"
import {
    ApprovalSubmittedSummary,
    MaterialSectionHeading,
    sectionValue,
} from "./approval-submitted-summary"
import { BusinessFailureState } from "@/components/business"
import { Spinner } from "@/components/ui/spinner"
import { displayBusinessText } from "@/lib/display-name"
import { useApprovalMaterials } from "../hooks/use-approval-materials"
import { displayActorName, displayReadableName } from "../display"

/** 审批专用资料预览；不加载普通业务详情，不把冻结摘要当作完整单据。 */
export function ApprovalSubmittedMaterials({
    instanceId,
    enabled,
    view = "all",
    sourceRevisionId,
}: {
    instanceId: string
    enabled: boolean
    view?: "all" | "source-sales" | "attachments"
    sourceRevisionId?: string
}) {
    const query = useApprovalMaterials(instanceId, enabled)
    if (query.isPending || query.isFetching)
        return (
            <div className="flex min-h-64 items-center justify-center gap-2 rounded-lg bg-card p-8">
                <Spinner />
                正在读取提交资料…
            </div>
        )
    if (query.isError)
        return (
            <div className="rounded-lg bg-card p-8">
                <BusinessFailureState
                    id="approval-submitted-materials-retry"
                    title="提交资料读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </div>
        )
    const {
        display,
        attachments,
        sales_order: submittedSalesOrder,
        subject_version: version,
        current_sales_basis: currentBasis,
    } = query.data
    const sourceOrder =
        view === "source-sales"
            ? query.data.source_sales_orders?.find(
                  (order) => order.revision.id === sourceRevisionId,
              )
            : undefined
    const salesOrder =
        view === "source-sales" ? sourceOrder?.revision : submittedSalesOrder
    const sourceSnapshot = display.source_sales?.find(
        (sales) => sales.revision_id === sourceRevisionId,
    )
    const source =
        view === "source-sales" && sourceSnapshot
            ? sourceSnapshot.source
            : display.source
    const submitterName = displayActorName(
        source.submitter_name,
        submittedSalesOrder?.submitted_by,
    )
    const counterpartyName =
        displayReadableName(display.counterparty_label) ||
        displayReadableName(source.customer)
    const isSummary = view === "all" && !salesOrder
    return (
        <article className="space-y-6 rounded-lg bg-card p-6 shadow-lg sm:p-10">
            <header className="space-y-1.5 border-b pb-5 pr-8">
                {isSummary ? (
                    <>
                        <p className="text-xs font-medium text-muted-foreground">
                            审批提交资料 · 提交版本 {version}
                        </p>
                        <h2 className="text-xl font-semibold">
                            {counterpartyName || "审批提交资料"}
                        </h2>
                        {source.extra_sections.length === 0 &&
                            source.list_summary && (
                                <p className="text-sm">{source.list_summary}</p>
                            )}
                        <p className="text-sm text-muted-foreground">
                            提交资料保留自此次提交，后续变化不改写原资料。
                        </p>
                    </>
                ) : (
                    <>
                        <h2 className="text-xl font-semibold">
                            {view === "source-sales"
                                ? "销售单预览"
                                : view === "attachments"
                                  ? "合同、凭证及附件"
                                  : "销售单提交预览"}
                        </h2>
                        <p className="text-sm text-muted-foreground">
                            {view === "source-sales"
                                ? `销售版本 V${sourceSnapshot?.revision_no ?? "—"}`
                                : `提交版本 ${version}`}{" "}
                            · 提交资料保留自此次提交，后续变化不改写原资料。
                        </p>
                    </>
                )}
            </header>
            {view === "attachments" ? null : salesOrder ? (
                <ApprovalSalesOrderPaper
                    submission={salesOrder}
                    documentNo={
                        displayBusinessText(
                            sourceOrder?.document_no ?? query.data.document_no,
                            sourceSnapshot?.document_id ??
                                query.data.document_id,
                        ) || "未记录单据编号"
                    }
                    submitterName={submitterName ?? null}
                />
            ) : view === "source-sales" ? (
                <p role="status" className="text-sm text-muted-foreground">
                    此次审批的完整来源销售单暂不可读取，请刷新后重试；仍不可用时请联系管理员核对。
                </p>
            ) : (
                <ApprovalSubmittedSummary
                    source={source}
                    submitterName={submitterName}
                    impactSummary={display.impact_summary}
                />
            )}
            {view === "all" && (display.source_sales ?? []).length > 0 && (
                <section className="space-y-4 border-t pt-5">
                    <MaterialSectionHeading title="关联销售单" />
                    {display.source_sales
                        ?.filter(
                            (sales) =>
                                !sourceRevisionId ||
                                sales.revision_id === sourceRevisionId,
                        )
                        .map((sales) => (
                            <article
                                key={sales.revision_id}
                                className="space-y-3 rounded-lg border p-4 text-sm"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <p className="font-medium">
                                        {displayBusinessText(
                                            sales.document_no,
                                            sales.document_id,
                                        ) || "未记录销售单号"}{" "}
                                        · 销售版本 {sales.revision_no}
                                    </p>
                                    <p className="num font-semibold">
                                        <span className="mr-2 text-xs font-normal text-muted-foreground">
                                            整单销售金额
                                        </span>
                                        {sales.source.amount_label}
                                    </p>
                                </div>
                                {sales.source.customer && (
                                    <p>
                                        {displayReadableName(
                                            sales.source.customer,
                                        ) || "未记录客户名称"}
                                    </p>
                                )}
                                <dl className="grid gap-3 sm:grid-cols-2">
                                    {sales.source.extra_sections.map(
                                        (section) => (
                                            <div key={section.label}>
                                                <dt className="text-xs text-muted-foreground">
                                                    {section.label}
                                                </dt>
                                                <dd className="mt-1 break-words">
                                                    {sectionValue(section)}
                                                </dd>
                                            </div>
                                        ),
                                    )}
                                </dl>
                                <ul className="divide-y">
                                    {sales.source.lines.map((line, index) => (
                                        <li
                                            key={`${line.title}:${index}`}
                                            className="flex flex-wrap justify-between gap-2 py-2"
                                        >
                                            <span>
                                                {displayReadableName(
                                                    line.title,
                                                ) || "未记录商品名称"}
                                            </span>
                                            <span className="num text-muted-foreground">
                                                {line.quantity}
                                            </span>
                                        </li>
                                    ))}
                                </ul>
                                {sales.source.more_count > 0 && (
                                    <p className="text-muted-foreground">
                                        另有 {sales.source.more_count}{" "}
                                        行未展示，请结合销售合同或凭证核对。
                                    </p>
                                )}
                            </article>
                        ))}
                </section>
            )}
            {view === "all" &&
                currentBasis &&
                (!attachments.length ||
                    !display.source_sales?.length ||
                    display.source_sales.some(
                        (source) =>
                            source.revision_id !==
                            currentBasis.source.revision_id,
                    )) && (
                    <section className="space-y-4 border-t pt-5">
                        <MaterialSectionHeading
                            title={`当前销售依据 · V${currentBasis.source.revision_no}`}
                        />
                        <p className="text-sm text-muted-foreground">
                            以下为当前生效销售版本及该版本关联的合同与凭证。
                        </p>
                        <ApprovalSalesOrderPaper
                            submission={currentBasis.sales_order.revision}
                            documentNo={currentBasis.sales_order.document_no}
                            submitterName={null}
                        />
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            attachments={currentBasis.attachments}
                            sourceRevisionId={currentBasis.source.revision_id}
                            idPrefix="approval-submitted-current-file"
                            emptyMessage={
                                currentBasis.unavailable_reason ??
                                "此销售版本未关联可读取的合同或凭证，请联系销售核对归档资料。"
                            }
                        />
                    </section>
                )}
            {view === "all" && query.data.current_sales_basis_error && (
                <p role="alert" className="text-sm text-muted-foreground">
                    {query.data.current_sales_basis_error}
                </p>
            )}
            {(attachments.length > 0 || !currentBasis || view !== "all") && (
                <section className="space-y-3 border-t pt-5">
                    <MaterialSectionHeading
                        title={
                            display.source_sales?.length
                                ? "提交附件及关联销售合同、凭证"
                                : "提交合同、凭证及附件"
                        }
                        note={
                            attachments.length
                                ? `${attachments.length} 个文件`
                                : null
                        }
                    />
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                    />
                </section>
            )}
        </article>
    )
}
