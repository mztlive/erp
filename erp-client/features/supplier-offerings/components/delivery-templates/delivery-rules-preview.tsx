"use client"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

import {
    BULK_TERM_LABEL,
    DROPSHIP_TERM_LABEL,
    type DeliveryRuleSetView,
    type DeliveryTemplateView,
    type DeliveryTerm,
} from "../../types"

type Mode = "dropship" | "bulk"

const TERM_VARIANT: Record<DeliveryTerm, "success" | "warning" | "neutral"> = {
    FREIGHT_INCLUDED: "success",
    FREIGHT_EXTRA: "warning",
    NOT_DELIVERABLE: "neutral",
}

/** 未设置模板时按全国配送、代发与集采均包邮判定。 */
const NATIONWIDE: DeliveryRuleSetView = {
    default_term: "FREIGHT_INCLUDED",
    rules: [],
}

function termLabel(mode: Mode, term: DeliveryTerm) {
    return mode === "dropship" && term !== "FREIGHT_EXTRA"
        ? DROPSHIP_TERM_LABEL[term]
        : BULK_TERM_LABEL[term]
}

function TermBadge({ mode, term }: { mode: Mode; term: DeliveryTerm }) {
    return (
        <Badge variant={TERM_VARIANT[term]} className="shrink-0">
            {termLabel(mode, term)}
        </Badge>
    )
}

function RuleColumn({
    mode,
    title,
    hint,
    set,
}: {
    mode: Mode
    title: string
    hint?: string
    set: DeliveryRuleSetView
}) {
    const groups = new Map<DeliveryTerm, string[]>()
    for (const rule of set.rules) {
        const names = groups.get(rule.term) ?? []
        names.push(rule.region_name)
        groups.set(rule.term, names)
    }
    return (
        <div className="min-w-0 space-y-2.5">
            <div className="flex items-baseline gap-2">
                <h4 className="text-sm font-medium">{title}</h4>
                {hint ? (
                    <span className="text-xs text-muted-foreground">
                        {hint}
                    </span>
                ) : null}
            </div>
            <dl className="space-y-2 text-sm">
                <div className="flex items-start gap-3">
                    <dt className="w-16 shrink-0 pt-0.5 text-xs text-muted-foreground">
                        其他地区
                    </dt>
                    <dd>
                        <TermBadge mode={mode} term={set.default_term} />
                    </dd>
                </div>
                {[...groups.entries()].map(([term, names]) => (
                    <div key={term} className="flex items-start gap-3">
                        <dt className="w-16 shrink-0 pt-0.5">
                            <TermBadge mode={mode} term={term} />
                        </dt>
                        <dd className="min-w-0 leading-6 break-words">
                            {names.join("、")}
                        </dd>
                    </div>
                ))}
            </dl>
        </div>
    )
}

/** 配送模板规则：代发、集采两栏，先给其他地区判定，再按判定列出例外地区。 */
export function DeliveryRulesPreview({
    template,
    className,
}: {
    /** 生效模板；为空时按全国配送、包邮展示。 */
    template: Pick<DeliveryTemplateView, "dropship" | "bulk"> | null
    className?: string
}) {
    return (
        <div className={cn("grid gap-5 sm:grid-cols-2", className)}>
            <RuleColumn
                mode="dropship"
                title="一件代发"
                hint="价格已含运费"
                set={template?.dropship ?? NATIONWIDE}
            />
            <RuleColumn
                mode="bulk"
                title="集采"
                set={template?.bulk ?? NATIONWIDE}
            />
        </div>
    )
}
