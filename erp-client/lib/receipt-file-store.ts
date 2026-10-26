/** 仅保存结果待确认命令的原始回款凭证，供刷新后按原命令重试。 */
function openReceiptFileStore(): Promise<IDBDatabase> {
    return new Promise((resolve, reject) => {
        const request = indexedDB.open("erp-receipt-files-v1", 1)
        request.onupgradeneeded = () => {
            if (!request.result.objectStoreNames.contains("values"))
                request.result.createObjectStore("values")
        }
        request.onsuccess = () => resolve(request.result)
        request.onerror = () =>
            reject(request.error ?? new Error("本机回款凭证文件存储不可用"))
        request.onblocked = () =>
            reject(new Error("本机回款凭证文件存储被占用，请关闭旧页面后重试"))
    })
}

async function receiptFileValue<T>(
    key: string,
    operation: "read" | "write" | "delete",
    value?: T,
): Promise<T | undefined> {
    const database = await openReceiptFileStore()
    return new Promise((resolve, reject) => {
        const transaction = database.transaction(
            "values",
            operation === "read" ? "readonly" : "readwrite",
        )
        const store = transaction.objectStore("values")
        const request =
            operation === "read"
                ? store.get(key)
                : operation === "write"
                  ? store.put(value, key)
                  : store.delete(key)
        let result: T | undefined
        request.onsuccess = () => {
            if (operation === "read") result = request.result as T | undefined
        }
        transaction.oncomplete = () => {
            database.close()
            resolve(result)
        }
        transaction.onerror = transaction.onabort = () => {
            database.close()
            reject(transaction.error ?? new Error("本机回款凭证文件保存失败"))
        }
    })
}

export function readReceiptFileValue<T>(key: string): Promise<T | undefined> {
    return receiptFileValue<T>(key, "read")
}

export async function writeReceiptFileValue<T>(
    key: string,
    value: T,
): Promise<void> {
    await receiptFileValue(key, "write", value)
}

export async function removeReceiptFileValue(key: string): Promise<void> {
    await receiptFileValue(key, "delete")
}
