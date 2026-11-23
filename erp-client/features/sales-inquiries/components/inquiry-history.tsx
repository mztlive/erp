import { ChevronRightIcon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { inquiryDate, inquiryDateInput } from "../lib/format"
import type {
    InquiryRequirement,
    ProcurementInquiry,
    SalesInquiry,
} from "../types"

export function InquiryRequirementSummary({
    requirement,
    procurement = false,
    showNote = true,
}: {
    requirement: InquiryRequirement
    procurement?: boolean
    showNote?: boolean
}) {
    return (
        <section className="divide-y divide-border/60" aria-label="需求内容">
            {showNote && requirement.note && (
                <p className="pb-3 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                    {requirement.note}
                </p>
            )}
            {requirement.lines.map((line) => (
                <article
                    key={line.line_id}
                    className="space-y-1.5 py-2.5 text-body-compact"
                >
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div className="min-w-0 flex-1 break-words">
                            <h3 className="font-medium text-foreground">
                                {line.product_name}
                            </h3>
                            <p className="num mt-0.5 text-xs text-muted-foreground">
                                SKU {line.sku_code}
                                {line.specification &&
                                line.specification !== "无规格"
                                    ? ` · ${line.specification}`
                                    : ""}
                            </p>
                        </div>
                        <QuantityValue
                            value={line.quantity}
                            unit={line.unit_code}
                            className="shrink-0 font-medium"
                        />
                    </div>
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
                        <span className="num">
                            交付：
                            {inquiryDateInput(line.delivery_due_at) || "未设置"}
                        </span>
                        {!procurement && line.sales_reference_price_gross && (
                            <span>
                                销售价{" "}
                                <MoneyValue
                                    value={line.sales_reference_price_gross}
                                    display="exact"
                                    showHint={false}
                                />
                            </span>
                        )}
                        {line.delivery_terms && (
                            <span className="wrap-anywhere">
                                {line.delivery_terms}
                            </span>
                        )}
                    </div>
                    {line.allocations.length > 0 && (
                        <div className="space-y-1.5 border-l-2 border-border pl-2.5 pt-1">
                            {line.allocations.map((allocation) => (
                                <div
                                    key={allocation.allocation_id}
                                    className="flex items-start justify-between gap-2 text-xs"
                                >
                                    <div className="min-w-0">
                                        <p className="text-foreground">
                                            {allocation.recipient} ·{" "}
                                            <span className="num text-muted-foreground">
                                                {allocation.phone}
                                            </span>
                                        </p>
                                        <p className="text-muted-foreground wrap-anywhere">
                                            {allocation.region_name}{" "}
                                            {allocation.address}
                                        </p>
                                    </div>
                                    <QuantityValue
                                        value={allocation.quantity}
                                        unit={line.unit_code}
                                        className="shrink-0 text-muted-foreground"
                                    />
                                </div>
                            ))}
                        </div>
                    )}
                </article>
            ))}
        </section>
    )
}

const EVENT_LABEL: Record<string, string> = {
    created: "创建询价",
    submitted: "提交需求",
    claimed: "采购接手",
    supplemented: "要求补充需求",
    supplement_requested: "要求补充需求",
    feedback_saved: "保存处理进度",
    published: "发布采购反馈",
    quote_renewed: "采购发起新一轮报价",
    feedback_published: "发布采购反馈",
    accepted: "销售接受结果",
    reinquired: "销售要求重询",
    closed: "关闭询价",
    reassigned: "改派采购",
}

function eventDotClass(kind: string): string {
    switch (kind) {
        case "accepted":
            return "bg-success"
        case "published":
        case "feedback_published":
        case "submitted":
            return "bg-primary"
        case "supplemented":
        case "supplement_requested":
        case "reinquired":
            return "bg-warning"
        case "closed":
            return "bg-muted-foreground"
        default:
            return "bg-border"
    }
}

export function InquiryHistory({
    inquiry,
    idPrefix = "sales-inquiry",
}: {
    inquiry: SalesInquiry | ProcurementInquiry
    idPrefix?: string
}) {
    const reinquiries = "reinquiries" in inquiry ? inquiry.reinquiries : []
    const events = "events" in inquiry ? inquiry.events : []
    return (
        <section className="space-y-6" aria-label="询价操作记录">
            {events.length === 0 &&
                reinquiries.length === 0 &&
                inquiry.requirements.length === 0 && (
                    <p className="rounded-lg border border-dashed border-border/70 bg-surface-sunken/50 px-4 py-8 text-center text-xs text-muted-foreground">
                        暂无处理记录
                    </p>
                )}
            {events.length > 0 && (
                <section aria-label="处理记录" className="space-y-3">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        处理时间线
                    </h3>
                    <ol className="relative space-y-4 border-l border-border/70 pl-4">
                        {[...events]
                            .sort((left, right) => right.at - left.at)
                            .map((event, index) => (
                                <li
                                    key={`${toAutomationIdSegment(event.kind)}-${event.at}-${index}`}
                                    className="relative space-y-1 text-body-compact"
                                >
                                    <span
                                        aria-hidden="true"
                                        className={`absolute top-1.5 -left-[1.28rem] size-2 rounded-full ring-2 ring-card ${eventDotClass(event.kind)}`}
                                    />
                                    <div className="flex flex-wrap items-baseline justify-between gap-2">
                                        <p className="font-medium text-foreground">
                                            {EVENT_LABEL[event.kind] ??
                                                "询价处理"}
                                        </p>
                                        <p className="num text-xs text-muted-foreground">
                                            {inquiryDate(event.at)}
                                        </p>
                                    </div>
                                    {event.reason && (
                                        <p className="rounded-md bg-muted/45 px-2.5 py-1.5 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                                            {event.reason}
                                        </p>
                                    )}
                                </li>
                            ))}
                    </ol>
                </section>
            )}
            {inquiry.requirements.length > 0 && (
                <section aria-label="需求版本" className="space-y-2.5">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        需求版本（
                        <span className="num">
                            {inquiry.requirements.length}
                        </span>
                        ）
                    </h3>
                    <div className="divide-y divide-border/60 rounded-lg border border-border/70">
                        {[...inquiry.requirements]
                            .reverse()
                            .map((requirement) => (
                                <details
                                    key={requirement.version}
                                    className="group"
                                >
                                    <summary
                                        id={`${idPrefix}-requirement-${requirement.version}`}
                                        className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2.5 text-body-compact hover:bg-muted/40 focus-visible:outline-2 focus-visible:outline-ring"
                                    >
                                        <span className="flex items-center gap-1.5 font-medium text-foreground">
                                            <ChevronRightIcon
                                                aria-hidden="true"
                                                className="size-3.5 text-muted-foreground transition-transform group-open:rotate-90"
                                            />
                                            <span className="num">
                                                第 {requirement.version} 版
                                            </span>
                                            <span className="num rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-normal text-muted-foreground">
                                                {requirement.data.lines.length}{" "}
                                                项
                                            </span>
                                        </span>
                                        <span className="num text-xs text-muted-foreground">
                                            {inquiryDate(
                                                requirement.submitted_at,
                                            )}
                                        </span>
                                    </summary>
                                    <div className="border-t border-border/60 px-3 py-2.5">
                                        {requirement.change_reason && (
                                            <p className="mb-2.5 rounded-md bg-muted/45 px-2.5 py-1.5 whitespace-pre-wrap break-words text-xs text-muted-foreground">
                                                变更说明：
                                                {requirement.change_reason}
                                            </p>
                                        )}
                                        <InquiryRequirementSummary
                                            requirement={requirement.data}
                                            procurement={
                                                !(
                                                    "sales_owner_user_id" in
                                                    inquiry
                                                )
                                            }
                                        />
                                    </div>
                                </details>
                            ))}
                    </div>
                </section>
            )}
            {reinquiries.length > 0 && (
                <section aria-label="重询记录" className="space-y-2.5">
                    <h3 className="text-xs font-medium text-muted-foreground">
                        重询记录（
                        <span className="num">{reinquiries.length}</span>）
                    </h3>
                    <div className="space-y-2">
                        {[...reinquiries]
                            .sort((left, right) => right.at - left.at)
                            .map((item, index) => (
                                <div
                                    key={`${item.at}-${index}`}
                                    className="space-y-1 rounded-lg border border-border/60 bg-surface-sunken/50 px-3 py-2.5 text-body-compact"
                                >
                                    <div className="flex flex-wrap items-baseline justify-between gap-2 text-xs">
                                        <span className="font-medium text-foreground">
                                            第{" "}
                                            <span className="num">
                                                {item.previous_feedback_round}
                                            </span>{" "}
                                            轮后
                                            {item.initiated_by_procurement
                                                ? "采购主动报价"
                                                : "销售重询"}
                                        </span>
                                        <span className="num text-muted-foreground">
                                            {inquiryDate(item.at)}
                                        </span>
                                    </div>
                                    <p className="whitespace-pre-wrap text-xs text-muted-foreground wrap-anywhere">
                                        {item.data.reason}
                                        {item.data.concerns
                                            ? `；${item.data.concerns}`
                                            : ""}
                                    </p>
                                </div>
                            ))}
                    </div>
                </section>
            )}
        </section>
    )
}
