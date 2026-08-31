import Link from "next/link"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { openWorkspaceLabel } from "@/lib/ui-text"
import type { CardFundsReviewItemView } from "@/features/card-funds-review/types"

export function EvidenceNavPanel({
    task,
    w05Href,
    w11Href,
}: {
    task: CardFundsReviewItemView
    w05Href: string
    w11Href: string
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid py-3">
                <CardTitle className="text-base">证据与导航</CardTitle>
            </CardHeader>
            <CardContent className="space-y-3 pt-4 text-sm">
                <p className="text-muted-foreground">{task.workItem.impact}</p>
                <Separator />
                <div className="flex flex-col gap-2">
                    <Button
                        id="card-contracts-funds-review-evidence-open-w05"
                        type="button"
                        variant="outline"
                        size="sm"
                        render={<Link href={w05Href} />}
                    >
                        打开销售单
                    </Button>
                    <Button
                        id="card-contracts-funds-review-evidence-open-w11"
                        type="button"
                        variant="outline"
                        size="sm"
                        render={<Link href={w11Href} />}
                    >
                        {openWorkspaceLabel("W11")}
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
