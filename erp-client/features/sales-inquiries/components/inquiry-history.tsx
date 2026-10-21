import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { inquiryDate } from "../lib/format"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryRequirement,
    type SalesInquiry,
    type ProcurementInquiry,
} from "../types"

export function InquiryRequirementSummary({
    requirement,
    procurement = false,
}: {
    requirement: InquiryRequirement
    procurement?: boolean
}) {
    return (
        <section className="space-y-4" aria-label="需求内容">
            <h2 className="text-lg font-medium">需求内容</h2>
            {requirement.note && (
                <p className="whitespace-pre-wrap text-sm text-muted-foreground">
                    {requirement.note}
                </p>
            )}
            {requirement.lines.map((line) => (
                <article
                    key={line.line_id}
                    className="space-y-3 rounded-lg border p-4"
                >
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                            <h3 className="font-medium">{line.product_name}</h3>
                            <p className="mt-1 text-xs text-muted-foreground">
                                SKU {line.sku_code} ·{" "}
                                {line.specification || "未标注规格"}
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
                        <div className="divide-y rounded-md bg-muted/40 px-3">
                            {line.allocations.map((allocation) => (
                                <div
                                    key={allocation.allocation_id}
                                    className="flex flex-wrap justify-between gap-2 py-3 text-sm"
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
    const acceptances = "acceptances" in inquiry ? inquiry.acceptances : []
    const reinquiries = "reinquiries" in inquiry ? inquiry.reinquiries : []
    const events = "events" in inquiry ? inquiry.events : []
    return (
        <section className="space-y-5" aria-label="询价轮次历史">
            <h2 className="text-lg font-medium">反馈与历史</h2>
            {inquiry.feedbacks.length === 0 && (
                <p className="rounded-lg border border-dashed p-6 text-sm text-muted-foreground">
                    采购尚未发布反馈。
                </p>
            )}
            {[...inquiry.feedbacks].reverse().map((feedback) => {
                const requirement =
                    inquiry.requirements.find(
                        (item) => item.version === feedback.requirement_version,
                    )?.data ?? inquiry.draft
                const accepted = acceptances.some(
                    (item) => item.feedback_round === feedback.round,
                )
                return (
                    <details
                        key={feedback.round}
                        open={feedback.round === inquiry.feedback_round}
                        className="rounded-lg border"
                    >
                        <summary
                            id={`${idPrefix}-feedback-round-${feedback.round}`}
                            className="flex cursor-pointer flex-wrap items-center gap-3 p-4"
                        >
                            <span className="font-medium">
                                第 {feedback.round} 轮反馈
                            </span>
                            <span className="text-xs text-muted-foreground">
                                需求第 {feedback.requirement_version} 版 ·{" "}
                                {inquiryDate(feedback.published_at)}
                            </span>
                            {accepted && (
                                <StatusBadge label="已接受" tone="success" />
                            )}
                        </summary>
                        <div className="space-y-4 border-t p-4">
                            {feedback.data.lines.map((result) => {
                                const line = requirement.lines.find(
                                    (item) => item.line_id === result.line_id,
                                )
                                return (
                                    <div
                                        key={result.line_id}
                                        className="space-y-3 rounded-lg border p-4"
                                    >
                                        <div className="flex flex-wrap justify-between gap-3">
                                            <div className="font-medium">
                                                {line?.product_name ??
                                                    "商品需求"}
                                            </div>
                                            <StatusBadge
                                                label={
                                                    INQUIRY_CONCLUSION_LABEL[
                                                        result.conclusion
                                                    ]
                                                }
                                                tone={
                                                    result.conclusion ===
                                                    "unavailable"
                                                        ? "warning"
                                                        : result.conclusion ===
                                                            "pending"
                                                          ? "neutral"
                                                          : "success"
                                                }
                                            />
                                        </div>
                                        <p className="text-sm">
                                            可供{" "}
                                            <QuantityValue
                                                value={
                                                    result.available_quantity
                                                }
                                                unit={line?.unit_code}
                                            />
                                        </p>
                                        {result.public_note && (
                                            <p className="whitespace-pre-wrap text-sm text-muted-foreground">
                                                {result.public_note}
                                            </p>
                                        )}
                                        {result.prices.length > 0 && (
                                            <Table>
                                                <TableHeader>
                                                    <TableRow>
                                                        <TableHead>
                                                            适用数量
                                                        </TableHead>
                                                        <TableHead>
                                                            含税供给单价
                                                        </TableHead>
                                                        <TableHead>
                                                            税率
                                                        </TableHead>
                                                        <TableHead>
                                                            交付日期
                                                        </TableHead>
                                                        <TableHead>
                                                            有效期
                                                        </TableHead>
                                                    </TableRow>
                                                </TableHeader>
                                                <TableBody>
                                                    {result.prices.map(
                                                        (price) => (
                                                            <TableRow
                                                                key={
                                                                    price.price_id
                                                                }
                                                            >
                                                                <TableCell>
                                                                    <QuantityValue
                                                                        value={
                                                                            price.quantity
                                                                        }
                                                                        unit={
                                                                            line?.unit_code
                                                                        }
                                                                    />
                                                                    <p className="mt-1 text-xs text-muted-foreground">
                                                                        最低{" "}
                                                                        {
                                                                            price.min_quantity
                                                                        }
                                                                        {price.max_quantity
                                                                            ? `，最高 ${price.max_quantity}`
                                                                            : ""}
                                                                    </p>
                                                                </TableCell>
                                                                <TableCell>
                                                                    <MoneyValue
                                                                        value={
                                                                            price.unit_price_gross
                                                                        }
                                                                    />
                                                                    {price.included_cost_note && (
                                                                        <p className="mt-1 text-xs text-muted-foreground">
                                                                            含：
                                                                            {
                                                                                price.included_cost_note
                                                                            }
                                                                        </p>
                                                                    )}
                                                                </TableCell>
                                                                <TableCell>
                                                                    <RateValue
                                                                        value={multiplyFixed(
                                                                            price.tax_rate,
                                                                            "100",
                                                                            {
                                                                                leftMaxScale: 6,
                                                                                rightMaxScale: 0,
                                                                                outputScale: 2,
                                                                            },
                                                                        )}
                                                                        precision={
                                                                            2
                                                                        }
                                                                    />
                                                                </TableCell>
                                                                <TableCell>
                                                                    {inquiryDate(
                                                                        price.delivery_due_at,
                                                                    )}
                                                                </TableCell>
                                                                <TableCell>
                                                                    {inquiryDate(
                                                                        price.valid_until,
                                                                    )}
                                                                </TableCell>
                                                            </TableRow>
                                                        ),
                                                    )}
                                                </TableBody>
                                            </Table>
                                        )}
                                        {result.prices.some(
                                            (price) =>
                                                price.allocations.length > 0,
                                        ) && (
                                            <div className="space-y-1 text-xs text-muted-foreground">
                                                {result.prices.flatMap(
                                                    (price) =>
                                                        price.allocations.map(
                                                            (allocation) => {
                                                                const address =
                                                                    line?.allocations.find(
                                                                        (
                                                                            item,
                                                                        ) =>
                                                                            item.allocation_id ===
                                                                            allocation.allocation_id,
                                                                    )
                                                                return (
                                                                    <p
                                                                        key={`${price.price_id}:${allocation.allocation_id}`}
                                                                    >
                                                                        {address?.recipient ??
                                                                            "收件地址"}{" "}
                                                                        ·{" "}
                                                                        {
                                                                            address?.region_name
                                                                        }{" "}
                                                                        {
                                                                            address?.address
                                                                        }{" "}
                                                                        · 可供{" "}
                                                                        {
                                                                            allocation.quantity
                                                                        }{" "}
                                                                        ·{" "}
                                                                        <MoneyValue
                                                                            value={
                                                                                price.unit_price_gross
                                                                            }
                                                                        />
                                                                    </p>
                                                                )
                                                            },
                                                        ),
                                                )}
                                            </div>
                                        )}
                                    </div>
                                )
                            })}
                            {feedback.data.fees.length > 0 && (
                                <div className="space-y-2">
                                    <h3 className="text-sm font-medium">
                                        额外成本
                                    </h3>
                                    {feedback.data.fees.map((fee) => (
                                        <div
                                            key={fee.fee_id}
                                            className="space-y-2 rounded-md bg-muted/40 p-3 text-sm"
                                        >
                                            <div className="flex flex-wrap items-center gap-3">
                                                <span className="font-medium">
                                                    {fee.name}
                                                </span>
                                                <MoneyValue
                                                    value={fee.amount_gross}
                                                />
                                                <Badge variant="outline">
                                                    {fee.confirmation ===
                                                    "estimated"
                                                        ? "暂估"
                                                        : "已确认"}
                                                </Badge>
                                                {fee.included_in_goods_price && (
                                                    <Badge variant="outline">
                                                        已含商品价
                                                    </Badge>
                                                )}
                                            </div>
                                            <p className="text-xs text-muted-foreground">
                                                有效至{" "}
                                                {inquiryDate(fee.valid_until)}
                                                {fee.valid_conditions
                                                    ? ` · ${fee.valid_conditions}`
                                                    : ""}
                                            </p>
                                            {fee.public_note && (
                                                <p className="text-muted-foreground">
                                                    {fee.public_note}
                                                </p>
                                            )}
                                        </div>
                                    ))}
                                </div>
                            )}
                        </div>
                    </details>
                )
            })}
            {inquiry.requirements.length > 0 && (
                <details className="rounded-lg border">
                    <summary
                        id={`${idPrefix}-requirement-history`}
                        className="cursor-pointer p-4 text-sm font-medium"
                    >
                        需求版本（{inquiry.requirements.length}）
                    </summary>
                    <div className="space-y-3 border-t p-4">
                        {[...inquiry.requirements]
                            .reverse()
                            .map((requirement) => (
                                <details
                                    key={requirement.version}
                                    className="rounded-md border"
                                >
                                    <summary
                                        id={`${idPrefix}-requirement-${requirement.version}`}
                                        className="cursor-pointer p-3 text-sm"
                                    >
                                        第 {requirement.version} 版 ·{" "}
                                        {inquiryDate(requirement.submitted_at)}
                                        {requirement.change_reason
                                            ? ` · ${requirement.change_reason}`
                                            : ""}
                                    </summary>
                                    <div className="border-t p-3">
                                        <InquiryRequirementSummary
                                            requirement={requirement.data}
                                        />
                                    </div>
                                </details>
                            ))}
                    </div>
                </details>
            )}
            {reinquiries.length > 0 && (
                <div className="space-y-2">
                    <h3 className="text-sm font-medium">重询记录</h3>
                    {reinquiries.map((item, index) => (
                        <div
                            key={`${item.at}-${index}`}
                            className="rounded-lg border p-3 text-sm"
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
            {events.length > 0 && (
                <details className="rounded-lg border">
                    <summary
                        id={`${idPrefix}-event-history`}
                        className="cursor-pointer p-4 text-sm font-medium"
                    >
                        处理记录
                    </summary>
                    <ol className="space-y-3 border-t p-4">
                        {events.map((event, index) => (
                            <li
                                key={`${toAutomationIdSegment(event.kind)}-${event.at}-${index}`}
                                className="text-sm"
                            >
                                <p>
                                    {EVENT_LABEL[event.kind] ?? "询价处理"} ·{" "}
                                    {inquiryDate(event.at)}
                                </p>
                                {event.reason && (
                                    <p className="mt-1 text-muted-foreground">
                                        {event.reason}
                                    </p>
                                )}
                            </li>
                        ))}
                    </ol>
                </details>
            )}
        </section>
    )
}
