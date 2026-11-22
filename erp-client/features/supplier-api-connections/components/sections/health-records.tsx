"use client"

import { ChevronDownIcon } from "lucide-react"
import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import type { HealthRecordView } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"

const CHECK_TYPE_LABEL: Record<string, string> = {
    AUTHENTICATION: "鉴权验证",
    CONNECTIVITY: "连通性检查",
    FULL: "完整检查",
    CATALOG: "商品同步检查",
    ORDER: "下单能力检查",
    HEALTH: "健康检查",
}

function checkTypeLabel(raw: string): string {
    return CHECK_TYPE_LABEL[raw.toUpperCase()] ?? "接口验证"
}

function CheckResult({ record }: { record: HealthRecordView }) {
    return (
        <div className="min-w-0 space-y-2 text-body-compact">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                <BusinessStatusBadge
                    context="list"
                    label={record.resultLabel}
                    tone={record.resultTone}
                />
                <span className="num text-muted-foreground">
                    {formatDateTime(record.at, "default")}
                </span>
                <span className="text-muted-foreground">
                    {checkTypeLabel(record.checkType)}
                </span>
                {record.latencyMs != null ? (
                    <span className="num text-xs text-muted-foreground">
                        {record.latencyMs} ms
                    </span>
                ) : null}
            </div>
            {record.errorSummary ? (
                <p className="break-words text-destructive">
                    {record.errorSummary}
                </p>
            ) : null}
            {record.autoRetryStopped ? (
                <p className="text-destructive">
                    自动重试已停止，请检查接口配置后重新测试。
                </p>
            ) : null}
            {record.result === "UNKNOWN" ? (
                <p className="text-muted-foreground">
                    结果待确认，页面将在检测窗口内自动更新。
                </p>
            ) : null}
        </div>
    )
}

/** 最近检查直接显示，历史结果仅在排查时展开。 */
export function HealthRecords({
    records,
    idPrefix = "supplier-api-connections-health",
}: {
    records: HealthRecordView[]
    idPrefix?: string
}) {
    const [latest, ...history] = records
    if (!latest) {
        return <p className="text-xs text-muted-foreground">尚未测试连接</p>
    }

    return (
        <div className="space-y-3 border-t border-border/60 pt-4">
            <div className="space-y-2" aria-live="polite">
                <p className="text-xs text-muted-foreground">最近检查</p>
                <CheckResult record={latest} />
            </div>
            {history.length > 0 ? (
                <Collapsible>
                    <CollapsibleTrigger
                        id={`${idPrefix}-history-toggle`}
                        render={<Button variant="ghost" size="sm" />}
                        className="group text-muted-foreground"
                    >
                        历史检查（<span className="num">{history.length}</span>
                        ）
                        <ChevronDownIcon
                            className="size-4 transition-transform group-data-panel-open:rotate-180"
                            aria-hidden="true"
                        />
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <ul className="divide-y pt-2">
                            {history.map((record) => (
                                <li key={record.recordId} className="py-3">
                                    <CheckResult record={record} />
                                </li>
                            ))}
                        </ul>
                    </CollapsibleContent>
                </Collapsible>
            ) : null}
        </div>
    )
}
