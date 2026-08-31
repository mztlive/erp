"use client"

import { InfoIcon } from "lucide-react"

import { BusinessStatusBadge } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type { BusinessTag } from "../types"

export function BusinessTagDialog({
    tag,
    onOpenChange,
}: {
    tag: BusinessTag | null
    onOpenChange: (open: boolean) => void
}) {
    return (
        <Dialog open={tag != null} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="customers-quality-tag-dialog-close">
                <DialogHeader>
                    <DialogTitle className="flex items-center gap-2">
                        <InfoIcon className="size-4" aria-hidden="true" />
                        经营标签说明
                    </DialogTitle>
                    <DialogDescription>
                        标签由系统固定规则生成，页面不提供人工修改入口。
                    </DialogDescription>
                </DialogHeader>
                {tag ? (
                    <div className="space-y-3 text-sm">
                        <div className="flex flex-wrap items-center gap-2">
                            <BusinessStatusBadge
                                context="list"
                                label={tag.label}
                                tone={tag.tone}
                            />
                            <Badge variant="outline">
                                规则版本 {tag.ruleVersion}
                            </Badge>
                            <Badge variant="neutral">
                                {tag.type === "scale"
                                    ? "规模"
                                    : tag.type === "profit"
                                      ? "利润贡献"
                                      : "回款风险"}
                            </Badge>
                        </div>
                        <p className="text-muted-foreground">
                            {tag.explanation}
                        </p>
                        {tag.type === "profit" ? (
                            <p className="text-xs text-muted-foreground">
                                卡券收入进入规模和回款分析，但不进入利润贡献标签与实际盈亏。
                            </p>
                        ) : null}
                    </div>
                ) : null}
            </DialogContent>
        </Dialog>
    )
}
