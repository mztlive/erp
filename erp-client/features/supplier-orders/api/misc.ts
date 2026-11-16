/**
 * W26 供应商订单 · 尚未交付的后端端点（安全占位）。
 * 地址揭示 / 协同说明：后端详情不返回明文地址（仅加密快照）、无 NOTE 端点 → blocked。
 */

import type {
    FormalActionResponse,
    NoteInput,
    RevealAddressInput,
    RevealAddressResult,
} from "@/features/supplier-orders/types"

/**
 * 地址揭示：后端详情不返回明文地址（仅加密快照），无 reveal 端点。
 */
export async function revealSupplierOrderAddress(
    input: RevealAddressInput,
): Promise<FormalActionResponse<RevealAddressResult>> {
    void input
    return {
        status: "blocked",
        message:
            "暂不支持在此直接查看完整明文地址，请前往关联采购单或销售单查看。",
    }
}

export async function clearAddressReveal(orderId: string): Promise<void> {
    void orderId
    // no server session to clear
}

/**
 * 协同说明：后端无 NOTE 端点 → blocked。
 */
export async function addCollaborationNote(
    input: NoteInput,
): Promise<FormalActionResponse<{ lockVersion: number }>> {
    void input
    return {
        status: "blocked",
        message: "暂不支持保存协同备注。",
    }
}
