import { surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { ProfitLossPeriodBasisConfig } from "@/features/actual-profit-loss/types"
import { W16_FORMULA_HINT } from "@/features/actual-profit-loss/lib/url-state"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function AnalysisBlockedPanel({
    basisConfig,
    onSelectBasis,
}: {
    basisConfig: ProfitLossPeriodBasisConfig
    onSelectBasis: (code: string) => void
}) {
    return (
        <>
            <Alert variant="destructive">
                <AlertTitle>期间归属口径尚未配置</AlertTitle>
                <AlertDescription className="space-y-3">
                    <p>尚未设置默认归属口径，请选择上方任一口径后开始分析。</p>
                    <p className="text-xs text-muted-foreground">
                        公式说明：{W16_FORMULA_HINT}
                    </p>
                    <ul className="list-disc space-y-1 pl-5 text-sm">
                        {basisConfig.allowedPeriodBases.map((opt) => (
                            <li key={opt.code}>
                                <button
                                    id={`actual-profit-loss-blocked-basis-${toAutomationIdSegment(opt.code)}`}
                                    type="button"
                                    className="font-medium text-primary underline-offset-2 hover:underline"
                                    onClick={() => onSelectBasis(opt.code)}
                                >
                                    {opt.label}
                                </button>
                                <span className="text-muted-foreground">
                                    {" "}
                                    — {opt.explanation}
                                </span>
                            </li>
                        ))}
                    </ul>
                </AlertDescription>
            </Alert>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>公式与边界（查询阻断中）</CardTitle>
                    <CardDescription>选定口径后加载数据。</CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm text-muted-foreground">
                    <p>{W16_FORMULA_HINT}</p>
                    <p>
                        公式仅统计实际发生成本（冲减计入）；计划与已确认金额仅作对照。卡券与消费成本不在本页。
                    </p>
                </CardContent>
            </Card>
        </>
    )
}
