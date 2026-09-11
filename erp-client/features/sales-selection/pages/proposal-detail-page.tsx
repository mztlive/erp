"use client"

import { PageScaffold } from "@/components/business"
import { useProposalQuery } from "@/features/sales-selection/queries"
import { Button } from "@/components/ui/button"
import { FORM_LABEL, SUBMIT_MODE_LABEL } from "@/features/sales-selection/types"

export function ProposalDetailPage({ proposalId }: { proposalId: string }) {
    const query = useProposalQuery(proposalId)
    const proposal = query.data
    if (query.isError)
        return (
            <PageScaffold density="compact">
                <p role="alert">销售方案加载失败</p>
                <Button
                    id="selection-proposal-retry"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
            </PageScaffold>
        )
    if (!proposal) {
        return <PageScaffold density="compact">正在加载销售方案…</PageScaffold>
    }
    return (
        <PageScaffold density="compact" className="gap-4">
            <div>
                <p className="text-sm text-muted-foreground">销售方案</p>
                <h1 className="text-xl font-semibold">
                    {proposal.proposal_no}
                </h1>
                <p className="text-sm text-muted-foreground">
                    {proposal.customer_name} · {FORM_LABEL[proposal.form]} ·{" "}
                    {SUBMIT_MODE_LABEL[proposal.submit_mode]}
                </p>
            </div>
            {proposal.total_amount ? (
                <p className="text-lg font-medium">
                    合计 ¥ {proposal.total_amount}
                </p>
            ) : (
                <p className="text-sm text-muted-foreground">
                    本次仅确认可选范围，无成交合计。
                </p>
            )}
            <section>
                <h2 className="mb-2 font-medium">已选商品</h2>
                {proposal.display_lines.map((line, index) => (
                    <p key={line.display_item_id} className="text-sm">
                        {proposal.form === "PACKAGE"
                            ? `套餐 ${index + 1}`
                            : (proposal.sku_lines.find(
                                  (sku) =>
                                      sku.display_item_id ===
                                      line.display_item_id,
                              )?.name ?? "商品")}
                        {line.quantity ? ` × ${line.quantity}` : ""}
                        {line.line_amount
                            ? ` · ¥ ${line.line_amount}`
                            : ` · ¥ ${line.unit_price}`}
                    </p>
                ))}
            </section>
            <section className="grid gap-3">
                <h2 className="font-medium">商品明细</h2>
                {proposal.sku_lines.map((line, index) => (
                    <div
                        key={`${line.display_item_id}-${index}`}
                        className="rounded-lg border p-3"
                    >
                        <p className="font-medium">{line.name}</p>
                        <p className="text-sm text-muted-foreground">
                            {line.specification
                                ?.map((attr) => `${attr.name}：${attr.value}`)
                                .join(" · ")}
                        </p>
                        <p>
                            单价 ¥{line.unit_price}
                            {line.quantity != null &&
                                ` · 数量 ${line.quantity} ${line.unit ?? ""}`}
                            {line.line_amount != null &&
                                ` · 金额 ¥${line.line_amount}`}
                        </p>
                    </div>
                ))}
            </section>
        </PageScaffold>
    )
}
