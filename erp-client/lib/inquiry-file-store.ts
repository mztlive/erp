/** 询价命令与凭证使用结构化存储，保留 File 原始内容供刷新后核对。 */
import { createIndexedDbValueStore } from "@/lib/indexeddb-value-store"

const store = createIndexedDbValueStore("erp-inquiry-files-v1", {
    unavailable: "当前浏览器无法暂存所选文件，请重新选择",
    blocked: "检测到其他页面正在操作该文件，请关闭旧页面后重试",
    writeFailed: "所选文件暂存失败，请重新选择",
})

export function readInquiryFileValue<T>(key: string): Promise<T | undefined> {
    return store.read<T>(key)
}

export async function writeInquiryFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await store.write(key, value)
}

export async function removeInquiryFileValue(key: string): Promise<void> {
    await store.remove(key)
}
