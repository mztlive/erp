import {
    BusinessStatusBadge,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { REVIEW_TYPE_LABEL } from "@/features/card-funds-review/types"
import type { CardFundsReviewItemView } from "@/features/card-funds-review/types"

export function ReviewChainPanel({ task }: { task: CardFundsReviewItemView }) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30 py-3">
                <CardTitle className="text-base">复核记录（只读）</CardTitle>
                <CardDescription>
                    历史复核记录只读，不可修改或删除；本次将形成复核号{" "}
                    {task.reviewChain.nextReviewNo}
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {task.reviewChain.items.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        尚无历史复核。本次通过/驳回将形成首条复核记录。
                    </p>
                ) : (
                    task.reviewChain.items.map((item) => (
                        <div
                            key={item.reviewId}
                            className="rounded-lg border border-border px-3 py-2 text-sm"
                        >
                            <div className="flex flex-wrap items-center gap-2">
                                <span className="font-medium">
                                    复核号 {item.reviewNo}
                                </span>
                                <Badge variant="outline">
                                    {REVIEW_TYPE_LABEL[item.reviewType]}
                                </Badge>
                                <BusinessStatusBadge
                                    context="list"
                                    label={
                                        item.reviewResult === "APPROVED"
                                            ? "通过"
                                            : "驳回"
                                    }
                                    tone={
                                        item.reviewResult === "APPROVED"
                                            ? "success"
                                            : "destructive"
                                    }
                                />
                                <Badge variant="secondary">只读</Badge>
                            </div>
                            <p className="mt-1 text-muted-foreground">
                                {item.reviewerLabel} · {item.completedAt}
                            </p>
                        </div>
                    ))
                )}
            </CardContent>
        </Card>
    )
}
