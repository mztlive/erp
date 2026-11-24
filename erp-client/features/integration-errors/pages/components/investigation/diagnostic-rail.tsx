import { useEffect, useRef, useState } from "react"
import { CopyIcon } from "lucide-react"

import { AuditTimeline } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

import { INTEGRATION_ACTION_LABEL } from "../../../lib/presentation"
import {
    EVIDENCE_KIND_LABEL,
    type IntegrationResolutionItemView,
} from "../../../types"

export type DiagnosticTab = "payload" | "history"

function formatPayload(summary?: string) {
    const value = summary?.trim()
    if (!value || value === "—") return undefined
    try {
        JSON.parse(value)
    } catch {
        return value
    }

    // 只调整空白，保留服务器输出的大整数、十进制和转义字面值。
    const tokens = value.match(/"(?:\\.|[^"\\])*"|[{}[\],:]|[^\s{}[\],:]+/gu)
    if (!tokens) return value
    let depth = 0
    return tokens
        .map((token, index) => {
            const previous = tokens[index - 1]
            const opens = token === "{" || token === "["
            const closes = token === "}" || token === "]"
            const previousOpens = previous === "{" || previous === "["
            if (closes) depth -= 1
            const startsLine = closes
                ? !previousOpens
                : previousOpens || previous === ","
            const indent = startsLine ? `\n${"  ".repeat(depth)}` : ""
            if (opens) depth += 1
            return `${indent}${token}${token === ":" ? " " : ""}`
        })
        .join("")
}

function PayloadBlock({
    id,
    label,
    summary,
}: {
    id: string
    label: string
    summary?: string
}) {
    const text = formatPayload(summary)
    const [feedback, setFeedback] = useState<{
        text: string
        copied: boolean
    }>()
    return (
        <section
            aria-labelledby={`${id}-heading`}
            className="min-w-0 space-y-1.5"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-2">
                <h3
                    id={`${id}-heading`}
                    className="text-body-compact font-medium"
                >
                    {label}
                </h3>
                {text ? (
                    <Button
                        id={`${id}-copy`}
                        type="button"
                        size="xs"
                        variant="ghost"
                        aria-label={`复制${label}`}
                        onClick={async () => {
                            try {
                                await navigator.clipboard.writeText(
                                    summary ?? text,
                                )
                                setFeedback({ text, copied: true })
                            } catch {
                                setFeedback({ text, copied: false })
                            }
                        }}
                    >
                        <CopyIcon aria-hidden="true" />
                        复制
                    </Button>
                ) : null}
            </div>
            {text ? (
                <pre className="num min-w-0 rounded-md bg-surface-sunken p-3 text-xs wrap-anywhere whitespace-pre-wrap">
                    <code>{text}</code>
                </pre>
            ) : (
                <p className="text-xs text-muted-foreground">
                    暂无可展示的{label}
                </p>
            )}
            <p
                role="status"
                className="text-xs text-muted-foreground empty:hidden"
            >
                {feedback && feedback.text === text
                    ? feedback.copied
                        ? "已复制"
                        : "复制失败，请选中文本复制。"
                    : null}
            </p>
        </section>
    )
}

const auditLabel = (action: string) =>
    INTEGRATION_ACTION_LABEL[action.toUpperCase()] ??
    (/[\u3400-\u9fff]/u.test(action) ? action : "处理记录")

export function InvestigationDiagnosticRail({
    idPrefix,
    item,
    tab,
    onTabChange,
    focusPayload,
    onPayloadFocused,
}: {
    idPrefix: string
    item: IntegrationResolutionItemView
    tab: DiagnosticTab
    onTabChange: (tab: DiagnosticTab) => void
    focusPayload: boolean
    onPayloadFocused: () => void
}) {
    const payloadTrigger = `${idPrefix}-tab-payload`
    const [payloadPulse, setPayloadPulse] = useState(false)
    const pulseTimer = useRef<number | null>(null)
    useEffect(
        () => () => {
            if (pulseTimer.current !== null) {
                window.clearTimeout(pulseTimer.current)
            }
        },
        [],
    )
    useEffect(() => {
        if (!focusPayload || tab !== "payload") return
        const trigger = document.getElementById(payloadTrigger)
        if (!trigger) return
        const reduceMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches
        trigger.focus({ preventScroll: true })
        trigger.scrollIntoView({
            block: "nearest",
            behavior: reduceMotion ? "auto" : "smooth",
        })
        setPayloadPulse(true)
        if (pulseTimer.current !== null) {
            window.clearTimeout(pulseTimer.current)
        }
        pulseTimer.current = window.setTimeout(
            () => setPayloadPulse(false),
            2200,
        )
        onPayloadFocused()
    }, [focusPayload, onPayloadFocused, payloadTrigger, tab])

    const latest = item.attempts.reduce<
        IntegrationResolutionItemView["attempts"][number] | undefined
    >(
        (current, attempt) =>
            !current || attempt.attemptNumber > current.attemptNumber
                ? attempt
                : current,
        undefined,
    )
    const messageSummary = item.message?.maskedPayloadSummary
    const timeline = [
        ...item.auditTrail.map((entry) => ({ ...entry, source: "处理" })),
        ...item.evidenceTimeline.map((entry) => ({ ...entry, source: "凭证" })),
    ].sort((a, b) => b.at.localeCompare(a.at))

    return (
        <Tabs
            value={tab}
            onValueChange={(value) => {
                if (value === "payload" || value === "history")
                    onTabChange(value)
            }}
            className="min-h-0 min-w-0 flex-1 gap-0"
        >
            <TabsList
                variant="line"
                aria-label="诊断载荷与重放日志"
                className="w-full shrink-0 gap-3 border-b border-border/60"
            >
                <TabsTrigger value="payload" id={payloadTrigger}>
                    诊断载荷
                </TabsTrigger>
                <TabsTrigger value="history" id={`${idPrefix}-tab-history`}>
                    重放与轨迹
                </TabsTrigger>
            </TabsList>
            <TabsContent
                id={`${idPrefix}-payload-panel`}
                value="payload"
                className={cn(
                    "min-h-0 min-w-0 space-y-4 overflow-y-auto py-4",
                    payloadPulse &&
                        "rounded-md bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                )}
            >
                <PayloadBlock
                    id={`${idPrefix}-request`}
                    label="请求摘要"
                    summary={latest?.requestSummary}
                />
                <PayloadBlock
                    id={`${idPrefix}-response`}
                    label="响应摘要"
                    summary={latest?.responseSummary}
                />
                {messageSummary && messageSummary !== "—" ? (
                    <PayloadBlock
                        id={`${idPrefix}-message`}
                        label="脱敏消息摘要"
                        summary={messageSummary}
                    />
                ) : null}
            </TabsContent>
            <TabsContent
                id={`${idPrefix}-history-panel`}
                value="history"
                className="min-h-0 min-w-0 space-y-4 overflow-y-auto py-4 wrap-anywhere"
            >
                <section aria-label="执行尝试与重放" className="space-y-2">
                    <h3 className="text-body-compact font-medium">
                        执行尝试与重放
                    </h3>
                    {item.attempts.length ? (
                        <ul className="space-y-2">
                            {item.attempts.map((attempt) => (
                                <li
                                    key={attempt.attemptNumber}
                                    className="space-y-1 border-l-2 border-border pl-3 text-xs"
                                >
                                    <p className="font-medium">
                                        第{" "}
                                        <span className="num">
                                            {attempt.attemptNumber}
                                        </span>{" "}
                                        次尝试
                                        {" · "}
                                        <time
                                            className="num text-muted-foreground"
                                            dateTime={attempt.attemptedAt}
                                        >
                                            {formatDateTime(
                                                attempt.attemptedAt,
                                                "default",
                                            )}
                                        </time>
                                    </p>
                                    <p className="text-muted-foreground">
                                        {attempt.result}
                                    </p>
                                    {attempt.nextRetryAt ? (
                                        <p className="text-muted-foreground">
                                            下次重试{" "}
                                            <time
                                                className="num"
                                                dateTime={attempt.nextRetryAt}
                                            >
                                                {formatDateTime(
                                                    attempt.nextRetryAt,
                                                    "default",
                                                )}
                                            </time>
                                        </p>
                                    ) : null}
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            暂无执行记录
                        </p>
                    )}
                </section>
                {item.linkedEvidence.length ? (
                    <section
                        aria-label="核验凭证"
                        className="space-y-1.5 border-t border-border/60 pt-3"
                    >
                        <h3 className="text-body-compact font-medium">
                            核验凭证
                        </h3>
                        <ul className="space-y-1 text-xs">
                            {item.linkedEvidence.map((entry) => (
                                <li key={`${entry.kind}-${entry.recordId}`}>
                                    {entry.label}{" "}
                                    <span className="text-muted-foreground">
                                        {EVIDENCE_KIND_LABEL[entry.kind]}
                                    </span>
                                </li>
                            ))}
                        </ul>
                    </section>
                ) : null}
                <section
                    aria-label="处理审计"
                    className="space-y-2 border-t border-border/60 pt-3"
                >
                    <h3 className="text-body-compact font-medium">处理审计</h3>
                    <AuditTimeline
                        entries={timeline.map((entry) => ({
                            id: `${entry.source}-${entry.id}`,
                            action: auditLabel(entry.action),
                            operator: entry.actor,
                            occurredAt: entry.at,
                            occurredAtLabel: (
                                <span className="num">
                                    {formatDateTime(entry.at, "default")}
                                </span>
                            ),
                            source: entry.source,
                            note: entry.detail,
                        }))}
                    />
                </section>
            </TabsContent>
        </Tabs>
    )
}
