"use client"

import {
    FormalActionResult,
    DetailWorkspaceSection,
} from "@/components/business"
import { GateRow } from "@/features/import-opening/components/batch-facts"
import type { ImportBatchView } from "@/features/import-opening/types"

/** 生产应用门禁：仅提交应用前阶段展示。 */
export function ProductionGateCard({ batch }: { batch: ImportBatchView }) {
    const applyBlocked = batch.actionBlockers.filter(
        (b) => b.action === "START_APPLY",
    )
    return (
        <DetailWorkspaceSection
            title="提交前检查"
            subtitle="需先完成数据预检与各部门业务核对，才能正式导入系统。"
        >
            <div className="space-y-3">
                <GateRow
                    ok={batch.productionGates.validationEnvPassed}
                    label="已完成预检测试并核对通过"
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
                    label="核对通过后数据与导入规则未发生变动"
                />
                <GateRow
                    ok={batch.productionGates.ruleVersionStable}
                    label="导入规则已锁定"
                />
                <GateRow
                    ok={batch.productionGates.workItemTypeRegistered}
                    label="各部门确认流程已就绪"
                />
                {applyBlocked.length > 0 ? (
                    <FormalActionResult
                        status="blocked"
                        title="暂不满足正式导入条件"
                        description={applyBlocked
                            .map((b) => b.message)
                            .join(" ")}
                        facts={applyBlocked.map((b) => ({
                            label: "阻断原因",
                            value: b.message,
                        }))}
                    />
                ) : (
                    <FormalActionResult
                        status="succeeded"
                        title={
                            batch.stage === "CONFIRM"
                                ? "检查已完成，可开始正式导入"
                                : "检查已完成"
                        }
                        description="提交时系统会再次核验权限与数据，确认无误后开始导入。"
                    />
                )}
            </div>
        </DetailWorkspaceSection>
    )
}
