import type { ContractImportTask } from "@/features/contracts/api/upload"

/** 同一条件同时控制按钮、重试与完成回调，历史结果不能替代本次追加。 */
export function contractImportContextError(
    task: ContractImportTask,
    customerId?: string,
    revisionTarget?: { contractId: string; version: number },
): string | undefined {
    if (revisionTarget) {
        if (
            task.revision_target?.contract_id !== revisionTarget.contractId ||
            (task.result && task.result.id !== revisionTarget.contractId)
        )
            return "此记录不属于当前合同的新版本导入，请上传当前合同的文件。"
        if (task.revision_target.version !== revisionTarget.version)
            return "此记录基于其他合同版本创建，请关闭窗口后重新上传。"
    }
    if (
        task.status !== "succeeded" &&
        customerId &&
        task.expected_customer_id !== customerId
    )
        return "此记录未按当前客户创建，请重新上传当前客户的合同。"
    if (
        task.status === "succeeded" &&
        customerId &&
        task.customer_id !== customerId
    )
        return "此合同与当前开单客户不一致，请选择对应客户的合同。"
    return undefined
}
