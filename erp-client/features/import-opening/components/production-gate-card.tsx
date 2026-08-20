"use client"

import {
    FormalActionResult,
    surfacePanelClassName,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { GateRow } from "@/features/import-opening/components/batch-facts"
import type { ImportBatchView } from "@/features/import-opening/types"

/** 生产应用门禁：仅提交应用前阶段展示。 */
export function ProductionGateCard({ batch }: { batch: ImportBatchView }) {
    const applyBlocked = batch.actionBlockers.filter(
        (b) => b.action === "START_APPLY",
    )
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>提交前检查</CardTitle>
                <CardDescription>
                    验证环境校验与业务确认是生产应用前置条件；系统管理员不能代替确认。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                <GateRow
                    ok={batch.productionGates.validationEnvPassed}
                    label="验证环境校验与确认已通过并关联"
                />
                <GateRow
                    ok={batch.productionGates.allConfirmationsComplete}
                    label="全部必要责任确认完成"
                />
                <GateRow
                    ok={batch.productionGates.noBlockingIssues}
                    label="无阻塞校验问题"
                />
                <GateRow
                    ok={batch.productionGates.trialVersionMatches}
                    label="试算版本与确认一致（未因规则变化失效）"
                />
                <GateRow
                    ok={batch.productionGates.ruleVersionStable}
                    label="规则版本稳定"
                />
                <GateRow
                    ok={batch.productionGates.workItemTypeRegistered}
                    label="导入确认任务与专用提交命令已接线"
                />
                {applyBlocked.length > 0 ? (
                    <FormalActionResult
                        status="blocked"
                        title="提交生产应用已阻断"
                        description={applyBlocked
                            .map((b) => b.message)
                            .join(" ")}
                        facts={applyBlocked.map((b) => ({
                            label: b.code,
                            value: b.message,
                        }))}
                    />
                ) : (
                    <FormalActionResult
                        status="succeeded"
                        title={
                            batch.stage === "CONFIRM"
                                ? "检查已完成，可提交应用"
                                : "检查已完成"
                        }
                        description="提交时系统会再次核验权限与数据，确认无误后开始导入。"
                    />
                )}
            </CardContent>
        </Card>
    )
}
