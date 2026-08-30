import Link from "next/link"
import { ExternalLinkIcon } from "lucide-react"

import { taxAmountToneClass } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { formatMoneyDisplay } from "@/features/actual-profit-loss/lib/presentation"
import type { CostEntryDetail } from "@/features/actual-profit-loss/types"
import { cn } from "@/lib/utils"

export function CostEntryDetailBody({ entry }: { entry: CostEntryDetail }) {
    return (
        <div className="space-y-4">
            <DescriptionList columns="two">
                <DescriptionItem>
                    <DescriptionTerm>费用类型</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.costTypeLabel}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>阶段</DescriptionTerm>
                    <DescriptionDetails>
                        <Badge variant="secondary">{entry.stageLabel}</Badge>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>范围</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.costScopeLabel}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>供应商</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.supplierName ?? "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>含税金额</DescriptionTerm>
                    <DescriptionDetails>
                        <span
                            className={cn(
                                "num",
                                taxAmountToneClass("含税金额"),
                            )}
                        >
                            {formatMoneyDisplay(entry.amountGross)}
                        </span>
                        <span className="ml-1 text-xs text-muted-foreground">
                            仅展示，不参与利润
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>税率 / 税额</DescriptionTerm>
                    <DescriptionDetails>
                        <span className="num">
                            {entry.taxRate} /{" "}
                            <span className={taxAmountToneClass("税额")}>
                                {formatMoneyDisplay(entry.taxAmount)}
                            </span>
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>不含税金额</DescriptionTerm>
                    <DescriptionDetails>
                        <span
                            className={cn(
                                "num font-medium",
                                taxAmountToneClass("不含税金额"),
                            )}
                        >
                            {formatMoneyDisplay(entry.amountNet)}
                        </span>
                        <span className="ml-1 text-xs text-muted-foreground">
                            利润口径
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>发生时间</DescriptionTerm>
                    <DescriptionDetails>
                        <span className="num">
                            {formatDateTime(entry.occurredAt, "full")}
                        </span>
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>来源类型</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.sourceTypeLabel}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>来源单据</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.sourceDocumentNo}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>来源明细</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.sourceLineLabel ?? "—"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>来源单据版本</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.sourceVersion}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>销售单 / 明细</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.salesOrderNo}
                        {entry.salesOrderLineLabel
                            ? ` · ${entry.salesOrderLineLabel}`
                            : ""}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>原成本引用</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.originalCostEntryLabel ?? "—（非冲减）"}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>凭证授权摘要</DescriptionTerm>
                    <DescriptionDetails>
                        {entry.voucherSummary ?? "—"}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            {entry.correctionHref ? (
                <Alert>
                    <AlertTitle>前往纠错来源</AlertTitle>
                    <AlertDescription className="flex flex-col gap-2">
                        <span>
                            本页不执行变更确认。打开原业务对象使用变更/冲减流程后，返回本页等待数据刷新。
                        </span>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            render={
                                <Link
                                    href={entry.correctionHref}
                                    target="_blank"
                                />
                            }
                        >
                            {entry.correctionLabel ?? "打开来源"}
                            <ExternalLinkIcon className="ml-1 size-3.5" />
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}
