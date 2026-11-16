/** 仅保存结果待确认命令的原始回款凭证，供刷新后按原命令重试。 */
import { createIndexedDbValueStore } from "@/lib/indexeddb-value-store"

const store = createIndexedDbValueStore("erp-receipt-files-v1", {
    unavailable: "当前浏览器无法暂存回款凭证文件，请重新选择",
    blocked: "检测到其他页面正在操作该文件，请关闭旧页面后重试",
    writeFailed: "回款凭证文件暂存失败，请重新选择",
})

export function readReceiptFileValue<T>(key: string): Promise<T | undefined> {
    return store.read<T>(key)
}

export async function writeReceiptFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await store.write(key, value)
}

export async function removeReceiptFileValue(key: string): Promise<void> {
    await store.remove(key)
}
