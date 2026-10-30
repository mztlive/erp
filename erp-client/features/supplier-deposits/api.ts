import { apiGet, apiPost } from "@/lib/api"
import { downloadFileAsset } from "@/features/file-assets/api"
import type { AccountDetail, AccountRow, DepositCommand } from "./types"

const ROOT = "/admin/supplier-deposits/accounts"
export function fetchDepositAccounts(page: number) {
    return apiGet<{ items: AccountRow[]; has_more: boolean }>(
        `${ROOT}?page=${page}&page_size=20`,
    )
}
export function fetchDepositAccount(id: string) {
    return apiGet<AccountDetail>(`${ROOT}/${encodeURIComponent(id)}`)
}
export function executeDepositCommand(command: DepositCommand) {
    const path = command.accountId
        ? `${ROOT}/${encodeURIComponent(command.accountId)}${command.path}`
        : ROOT
    return apiPost<unknown>(path, command.body)
}

/** 从受控文件接口读取已核实的对账凭证，不向页面展示内部文件编号。 */
export function downloadDepositResolutionEvidence(assetId: string) {
    return downloadFileAsset(assetId, "预存款对账核实凭证")
}
