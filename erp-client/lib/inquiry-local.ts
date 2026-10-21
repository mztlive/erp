/** 询价本地输入按账号和单据隔离，不存储登录凭证。 */
export function inquiryStorageKey(userId: string, scope: string): string {
    return `erp.sales-inquiries.v1:${encodeURIComponent(userId)}:${encodeURIComponent(scope)}`
}

/** 读取本地输入；损坏的旧记录不覆盖服务端内容。 */
export function readInquiryLocalValue<T>(key: string): T | undefined {
    const value = localStorage.getItem(key)
    if (!value) return undefined
    try {
        return JSON.parse(value) as T
    } catch {
        return undefined
    }
}

/** 写入失败应显示给用户，提交命令不可在未保存身份时发出。 */
export function writeInquiryLocalValue(key: string, value: unknown): void {
    localStorage.setItem(key, JSON.stringify(value))
}
