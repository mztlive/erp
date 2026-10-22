import { QuantityValue, MoneyValue } from "@/components/business"
import { inquiryDate } from "../lib/format"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    type InquiryRequirement,
    type SalesInquiry,
    type ProcurementInquiry,
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
        <section className="divide-y" aria-label="需求内容">
            {showNote && requirement.note && (
                <p className="pb-3 whitespace-pre-wrap break-words text-sm text-muted-foreground">
                    {requirement.note}
                </p>
            )}
            {requirement.lines.map((line) => (
                <article key={line.line_id} className="space-y-2 py-3 text-sm">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div className="min-w-0 flex-1 break-words">
                            <h3 className="font-medium">{line.product_name}</h3>
                            <p className="mt-1 text-xs text-muted-foreground">
                                SKU {line.sku_code}
                                {line.specification
                                    ? ` · ${line.specification}`
                                    : ""}
                            </p>
                        </div>
                        <QuantityValue
                            value={line.quantity}
                            unit={line.unit_code}
                        />
                    </div>
                    <p className="text-sm text-muted-foreground">
                        要求交付：{inquiryDate(line.delivery_due_at)}
                        {line.delivery_terms ? ` · ${line.delivery_terms}` : ""}
                    </p>
                    {!procurement && line.sales_reference_price_gross && (
                        <p className="text-sm">
                            销售参考价：
                            <MoneyValue
                                value={line.sales_reference_price_gross}
                            />
                        </p>
                    )}
                    {line.allocations.length > 0 && (
                        <div className="space-y-2 border-l-2 border-muted pl-3">
                            {line.allocations.map((allocation) => (
                                <div
                                    key={allocation.allocation_id}
                                    className="space-y-1 text-xs"
                                >
                                    <div>
                                        <p>
                                            {allocation.recipient} ·{" "}
                                            {allocation.phone}
                                        </p>
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {allocation.region_name}{" "}
                                            {allocation.address}
                                        </p>
                                    </div>
                                    <QuantityValue
                                        value={allocation.quantity}
                                        unit={line.unit_code}
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
    feedback_published: "发布采购反馈",
    accepted: "销售接受结果",
    reinquired: "销售要求重询",
    closed: "关闭询价",
    reassigned: "改派采购",
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
            {events.length === 0 && reinquiries.length === 0 && (
                <p className="text-sm text-muted-foreground">暂无处理记录。</p>
            )}
            {events.length > 0 && (
                <section aria-label="处理记录">
                    <h3 className="text-sm font-medium">处理记录</h3>
                    <ol className="mt-2 divide-y">
                        {[...events]
                            .sort((left, right) => right.at - left.at)
                            .map((event, index) => (
                                <li
                                    key={`${toAutomationIdSegment(event.kind)}-${event.at}-${index}`}
                                    className="space-y-1 py-3 text-sm"
                                >
                                    <p>
                                        {EVENT_LABEL[event.kind] ?? "询价处理"}
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        {inquiryDate(event.at)}
                                    </p>
                                    {event.reason && (
                                        <p className="whitespace-pre-wrap break-words text-muted-foreground">
                                            {event.reason}
                                        </p>
                                    )}
                                </li>
                            ))}
                    </ol>
                </section>
            )}
            {inquiry.requirements.length > 0 && (
                <section aria-label="需求版本">
                    <h3 className="text-sm font-medium">
                        需求版本（{inquiry.requirements.length}）
                    </h3>
                    <div className="mt-2 divide-y">
                        {[...inquiry.requirements]
                            .reverse()
                            .map((requirement) => (
                                <details
                                    key={requirement.version}
                                    className="group"
                                >
                                    <summary
                                        id={`${idPrefix}-requirement-${requirement.version}`}
                                        className="cursor-pointer rounded-sm py-3 text-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                    >
                                        第 {requirement.version} 版
                                        <span className="ml-2 text-xs text-muted-foreground">
                                            {inquiryDate(
                                                requirement.submitted_at,
                                            )}
                                        </span>
                                    </summary>
                                    <div className="pb-3">
                                        {requirement.change_reason && (
                                            <p className="mb-3 whitespace-pre-wrap break-words text-sm text-muted-foreground">
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
                <div className="divide-y">
                    <h3 className="pb-2 text-sm font-medium">重询记录</h3>
                    {[...reinquiries]
                        .sort((left, right) => right.at - left.at)
                        .map((item, index) => (
                            <div
                                key={`${item.at}-${index}`}
                                className="py-3 text-sm"
                            >
                                <p>
                                    {inquiryDate(item.at)} · 第{" "}
                                    {item.previous_feedback_round} 轮后重询
                                </p>
                                <p className="mt-1 whitespace-pre-wrap text-muted-foreground">
                                    {item.data.reason}
                                    {item.data.concerns
                                        ? `；${item.data.concerns}`
                                        : ""}
                                </p>
                            </div>
                        ))}
                </div>
            )}
        </section>
    )
}
