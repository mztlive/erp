"use client"

import * as React from "react"
import { SparklesIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { useRegionIndex } from "@/features/regions"
import {
    parseSmartAddressText,
    type SmartParsedAddress,
} from "../lib/smart-address-parse"

const FIELD_LABELS: Record<
    SmartParsedAddress["matchedFields"][number],
    string
> = {
    recipient: "收货人",
    phone: "联系电话",
    region: "收货地区",
    address: "详细地址",
}

/**
 * 粘贴整段收货文本（微信、邮件、表格复制）一键识别收货人、电话、地区与详细地址。
 * 识别结果仅作为辅助预填，不绕过下方表单校验。
 */
export function SalesDeliverySmartPaste({
    idPrefix,
    onParsed,
}: {
    idPrefix: string
    onParsed: (parsed: SmartParsedAddress) => void
}) {
    const regions = useRegionIndex()
    const [raw, setRaw] = React.useState("")
    const [feedback, setFeedback] = React.useState<{
        tone: "info" | "warning"
        text: string
    } | null>(null)

    const handleParse = () => {
        const parsed = parseSmartAddressText(raw, regions.data)
        if (parsed.matchedFields.length === 0) {
            setFeedback({
                tone: "warning",
                text: "未能识别有效收货信息，请确认包含姓名、电话或省市区后重试。",
            })
            return
        }
        onParsed(parsed)
        const names = parsed.matchedFields.map((field) => FIELD_LABELS[field])
        const missing = (
            ["recipient", "phone", "region", "address"] as const
        ).filter((field) => !parsed.matchedFields.includes(field))
        setFeedback({
            tone: missing.length === 0 ? "info" : "warning",
            text:
                missing.length === 0
                    ? "已自动识别收货人、联系电话、收货地区与详细地址，请核对。"
                    : `已识别${names.join("、")}，请手动补全${missing.map((field) => FIELD_LABELS[field]).join("、")}。`,
        })
    }

    return (
        <div className="space-y-2 rounded-lg border border-border/75 bg-surface-sunken/70 p-3">
            <div className="flex items-center justify-between gap-2">
                <Label
                    htmlFor={`${idPrefix}-smart-paste-input`}
                    className="inline-flex items-center gap-1.5 text-xs font-medium text-foreground/80"
                >
                    <SparklesIcon
                        className="size-3.5 text-primary"
                        aria-hidden="true"
                    />
                    智能识别收货地址
                </Label>
                <span className="text-xs text-muted-foreground">
                    支持从微信或表格直接粘贴一整段文本
                </span>
            </div>
            <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
                <Textarea
                    id={`${idPrefix}-smart-paste-input`}
                    rows={2}
                    value={raw}
                    placeholder="例：张三 13800138001 北京市朝阳区测试路 1 号"
                    className="min-h-14 flex-1 bg-card text-body-compact"
                    onChange={(event) => {
                        setRaw(event.target.value)
                        if (feedback) setFeedback(null)
                    }}
                    onKeyDown={(event) => {
                        if (
                            (event.metaKey || event.ctrlKey) &&
                            event.key === "Enter" &&
                            raw.trim()
                        ) {
                            event.preventDefault()
                            handleParse()
                        }
                    }}
                />
                <Button
                    id={`${idPrefix}-smart-paste-apply`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={!raw.trim()}
                    className="shrink-0 sm:mt-0.5"
                    onClick={handleParse}
                >
                    <SparklesIcon
                        data-icon="inline-start"
                        className="size-3.5"
                        aria-hidden="true"
                    />
                    智能识别
                </Button>
            </div>
            {feedback ? (
                <p
                    role="status"
                    className={
                        feedback.tone === "warning"
                            ? "text-xs text-warning-soft-foreground"
                            : "text-xs text-info-soft-foreground"
                    }
                >
                    {feedback.text}
                </p>
            ) : null}
        </div>
    )
}
